//! A database of a test's own on the suite's server (`docs/tests.md`): the migrations run once per test binary
//! into a template, and each test is handed a copy of it, which the server makes in a fraction of the time a
//! migration takes, or a blank database where the test migrates its own. A database is dropped when its test is
//! done, and the test fails if it cannot be; before a binary makes its first database, those a binary that is
//! gone left behind are dropped.

use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

use bmo_brain::store::MIGRATOR;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection, Executor, PgConnection};

/// What a template's name begins with, followed by the process that made it.
const TEMPLATE: &str = "bmo_brain_test_tmpl_";
/// What a copy's name begins with, followed by the process that made it and a count.
const COPY: &str = "bmo_brain_test_copy_";
/// The names a checkout behind this module still makes its databases under, each followed by the process and,
/// but for a template, a count; the sweep drops them as it drops its own.
const FORMER: [&str; 3] = ["brain_rs_tmpl_", "brain_rs_copy_", "brain_tool_"];
/// How many times a drop ends the database's sessions and tries again before it gives up.
const DROP_ATTEMPTS: usize = 3;

static COPIES: AtomicU32 = AtomicU32::new(0);
static SWEPT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
static MADE: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

/// The suite's PostgreSQL server, as the run names it, and the machine's own socket where nothing does.
fn test_address() -> String {
  std::env::var("BMO_TEST_POSTGRES")
    .ok()
    .filter(|named| !named.is_empty())
    .unwrap_or_else(|| "postgres:///bmo_test?host=/var/run/postgresql".into())
}

/// The options that open `database` on the suite's server.
pub fn options(database: &str) -> PgConnectOptions {
  PgConnectOptions::from_str(&test_address())
    .expect("the test server's address reads")
    .database(database)
    .disable_statement_logging()
}

/// The suite server's address with `database` in place of its own, as a caller handed one opens it.
fn address_of(database: &str) -> String {
  let address = test_address();
  let (base, query) = address
    .split_once('?')
    .map_or((address.as_str(), None), |(base, query)| (base, Some(query)));
  let base = base.rsplit_once('/').map(|(head, _)| head).unwrap_or(base);

  match query {
    Some(query) => format!("{base}/{database}?{query}"),
    None => format!("{base}/{database}"),
  }
}

async fn server() -> Result<PgConnection, sqlx::Error> {
  PgConnection::connect_with(&options("postgres")).await
}

/// The databases a dead process of this harness left behind dropped, once per binary and before it makes any
/// of its own.
async fn swept() {
  SWEPT.get_or_init(sweep).await;
}

// The template this binary's copies are made from, migrated the first time a test asks.
async fn template() -> String {
  MADE
    .get_or_init(|| async {
      swept().await;

      let name = format!("{TEMPLATE}{}", std::process::id());
      let mut admin = server().await.expect("the test server answers");

      create(&mut admin, &name, "template0 ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'").await;
      admin.close().await.expect("the server connection closes");

      let mut db = PgConnection::connect_with(&options(&name)).await.expect("the template answers");

      MIGRATOR.run(&mut db).await.expect("the template migrates");
      db.close().await.expect("the template's connection closes");

      name
    })
    .await
    .clone()
}

async fn create(admin: &mut PgConnection, database: &str, template: &str) {
  admin
    .execute(format!("CREATE DATABASE \"{database}\" TEMPLATE {template}").as_str())
    .await
    .expect("the database is made");
}

/// Drops `database`. This role's own sessions on it are ended first, since a drop that forces every session off
/// needs the right to end other roles' processes, an autovacuum worker's among them, which the suite's role need
/// not hold; the plain drop then stops an autovacuum worker itself and waits for the ended sessions to leave. A
/// session that opened between the two is ended on the next attempt.
async fn drop_database(admin: &mut PgConnection, database: &str) -> Result<(), sqlx::Error> {
  let mut attempt = 1;

  loop {
    sqlx::query(
      "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
       WHERE datname = $1 AND pid <> pg_backend_pid() AND usename = current_user",
    )
    .bind(database)
    .execute(&mut *admin)
    .await?;

    match admin.execute(format!("DROP DATABASE IF EXISTS \"{database}\"").as_str()).await {
      Ok(_) => return Ok(()),
      Err(error) if attempt == DROP_ATTEMPTS => return Err(error),
      Err(_) => attempt += 1,
    }
  }
}

/// Runs `work` to its end on a runtime of its own on a thread of its own, since a guard dropped inside a test's
/// runtime may not block on it.
fn on_own_runtime<T: Send + 'static>(work: impl Future<Output = T> + Send + 'static) -> T {
  std::thread::spawn(move || {
    tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .expect("a runtime is built")
      .block_on(work)
  })
  .join()
  .expect("the scratch work ends")
}

/// Drops every template and copy a dead process of this harness left, under its own names and the former ones.
/// A database is left alone while its process runs on this machine or a client's session is on it, so a run of
/// another checkout's harness against the same server keeps its own; this process has made none yet, so its own
/// id counts as gone. A database the sweep cannot drop is named on the error stream and left for the next run.
async fn sweep() {
  let mut admin = match server().await {
    Ok(admin) => admin,
    Err(error) => {
      eprintln!("the scratch sweep reaches no test server: {error}");
      return;
    }
  };
  let unattended: Vec<String> = match sqlx::query_scalar(
    "SELECT datname::text FROM pg_database d WHERE NOT EXISTS \
     (SELECT 1 FROM pg_stat_activity a WHERE a.datname = d.datname AND a.backend_type = 'client backend')",
  )
  .fetch_all(&mut admin)
  .await
  {
    Ok(names) => names,
    Err(error) => {
      eprintln!("the scratch sweep lists no databases: {error}");
      Vec::new()
    }
  };

  for name in unattended {
    let Some(process) = process_of(&name) else {
      continue;
    };

    if lives(process) {
      continue;
    }

    if let Err(error) = drop_database(&mut admin, &name).await {
      eprintln!("the scratch sweep leaves {name}: {error}");
    }
  }

  let _ = admin.close().await;
}

/// The process id a template's or a copy's name carries, if the name is one the sweep drops: one of its prefixes
/// followed by the id alone or by the id and a count.
fn process_of(name: &str) -> Option<u32> {
  let rest = [TEMPLATE, COPY]
    .into_iter()
    .chain(FORMER)
    .find_map(|prefix| name.strip_prefix(prefix))?;
  let (process, count) = rest.split_once('_').unwrap_or((rest, "0"));

  count.parse::<u32>().ok()?;
  process.parse().ok()
}

/// Whether process `id`, other than this one, is running on this machine. Where the process table cannot be read
/// every process counts as alive, so nothing is dropped on a guess.
fn lives(id: u32) -> bool {
  let table = std::path::Path::new("/proc");

  id != std::process::id() && (!table.join("self").exists() || table.join(id.to_string()).exists())
}

/// A database of the test's own, dropped when the guard goes; the test fails if it cannot be.
pub struct Scratch {
  name: String,
}

impl Scratch {
  /// A copy of this binary's migrated, empty template.
  pub async fn copy() -> Scratch {
    let template = template().await;

    Self::made(&format!("\"{template}\"")).await
  }

  /// A database with nothing in it, not even the migrations' record.
  pub async fn blank() -> Scratch {
    Self::made("template0 ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'").await
  }

  async fn made(template: &str) -> Scratch {
    swept().await;

    let name = format!("{COPY}{}_{}", std::process::id(), COPIES.fetch_add(1, Ordering::SeqCst));
    let mut admin = server().await.expect("the test server answers");

    create(&mut admin, &name, template).await;
    admin.close().await.expect("the server connection closes");

    Scratch { name }
  }

  /// The options that open the database.
  pub fn options(&self) -> PgConnectOptions {
    options(&self.name)
  }

  /// The database's address, as a caller handed one opens it.
  pub fn address(&self) -> String {
    address_of(&self.name)
  }

  /// A connection to the database.
  pub async fn connect(&self) -> PgConnection {
    PgConnection::connect_with(&self.options()).await.expect("the database answers")
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let name = std::mem::take(&mut self.name);
    let dropped = on_own_runtime(async move {
      let mut admin = server().await.map_err(|error| error.to_string())?;
      let dropped = drop_database(&mut admin, &name).await.map_err(|error| format!("{name}: {error}"));
      let _ = admin.close().await;

      dropped
    });

    if let Err(error) = dropped
      && !std::thread::panicking()
    {
      panic!("the scratch database is dropped: {error}");
    }
  }
}
