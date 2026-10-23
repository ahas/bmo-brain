//! What the service's tests share: a migrated database of the test's own, and the service answering over it on
//! a port of its own, with a client that reaches it.
//!
//! The migrations run once per test binary into a template database, and each test is handed a copy of it,
//! which the server makes in a fraction of the time a migration takes. A copy is dropped when its test is
//! done, and the template and copies a binary left behind are dropped by the next binary to make a template.

#![allow(dead_code)]

use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

use bmo_brain::api::console::Console;
use bmo_brain::api::{Brain, router};
use bmo_brain::store::{MIGRATOR, Store};
use bmo_brain_client::BrainClient;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Connection, Executor, PgConnection};
use tokio::task::JoinHandle;

/// The token the served brain takes.
pub const TOKEN: &str = "the-brain-token";
/// The console's token the served brain takes.
pub const CONSOLE_TOKEN: &str = "the-console-token";

// What a template's and a copy's names begin with, ahead of the process that made them.
const TEMPLATE: &str = "brain_rs_tmpl_";
const COPY: &str = "brain_rs_copy_";

static COPIES: AtomicU32 = AtomicU32::new(0);
static MADE: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

/// The suite's PostgreSQL server, as the run names it, and the machine's own socket where nothing does.
fn test_address() -> String {
  std::env::var("BMO_TEST_POSTGRES")
    .unwrap_or_else(|_| "postgres:///bmo_test?host=/var/run/postgresql".into())
}

fn options(database: &str) -> PgConnectOptions {
  PgConnectOptions::from_str(&test_address())
    .unwrap()
    .database(database)
    .disable_statement_logging()
}

async fn server() -> PgConnection {
  PgConnection::connect_with(&options("postgres")).await.unwrap()
}

fn alive(pid: &str) -> bool {
  std::path::Path::new("/proc").join(pid).exists()
}

// The template this binary's copies are made from, migrated the first time a test asks. The databases a
// binary that is gone left behind are dropped first.
async fn template() -> String {
  MADE
    .get_or_init(|| async {
      let mut admin = server().await;
      let left: Vec<String> = sqlx::query_scalar(
        "SELECT datname::text FROM pg_database WHERE datname LIKE $1 OR datname LIKE $2",
      )
      .bind(format!("{}%", TEMPLATE.replace('_', "\\_")))
      .bind(format!("{}%", COPY.replace('_', "\\_")))
      .fetch_all(&mut admin)
      .await
      .unwrap();
      for old in left {
        let pid = old
          .trim_start_matches(TEMPLATE)
          .trim_start_matches(COPY)
          .split('_')
          .next()
          .unwrap_or_default();
        if !alive(pid) {
          let _ = admin.execute(format!("DROP DATABASE IF EXISTS \"{old}\" WITH (FORCE)").as_str()).await;
        }
      }
      let name = format!("{TEMPLATE}{}", std::process::id());
      admin
        .execute(
          format!(
            "CREATE DATABASE \"{name}\" TEMPLATE template0 ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'"
          )
          .as_str(),
        )
        .await
        .unwrap();
      admin.close().await.unwrap();
      let mut db = PgConnection::connect_with(&options(&name)).await.unwrap();
      MIGRATOR.run(&mut db).await.unwrap();
      db.close().await.unwrap();
      name
    })
    .await
    .clone()
}

/// A copy dropped, its connections ended with it. It goes on a thread of its own, since a guard dropped inside
/// the runtime may not block on it.
fn drop_copy(database: String) {
  let _ = std::thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async move {
      let mut admin = server().await;
      let _ = admin
        .execute(format!("DROP DATABASE IF EXISTS \"{database}\" WITH (FORCE)").as_str())
        .await;
      let _ = admin.close().await;
    });
  })
  .join();
}

/// The service over a migrated, empty database of the test's own, answering on a port of its own until the
/// guard goes.
pub struct Served {
  database: String,
  pub store: Store,
  pub client: BrainClient,
  pub base: String,
  serving: JoinHandle<()>,
}

impl Served {
  pub async fn new() -> Served {
    Self::serving(None).await
  }

  /// The service serving the console bundle built into this folder, where it holds one.
  pub async fn with_bundle(folder: &std::path::Path) -> Served {
    Self::serving(Some(folder)).await
  }

  async fn serving(bundle: Option<&std::path::Path>) -> Served {
    let template = template().await;
    let database = format!("{COPY}{}_{}", std::process::id(), COPIES.fetch_add(1, Ordering::SeqCst));
    let mut admin = server().await;
    admin
      .execute(format!("CREATE DATABASE \"{database}\" TEMPLATE \"{template}\"").as_str())
      .await
      .unwrap();
    admin.close().await.unwrap();
    let pool = PgPoolOptions::new().max_connections(8).connect_with(options(&database)).await.unwrap();
    let store = Store::over(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let console = match bundle {
      Some(folder) => Console::over(CONSOLE_TOKEN.to_owned(), folder),
      None => Console {
        token: CONSOLE_TOKEN.to_owned(),
        bundle: None,
      },
    };
    let brain = Brain {
      store: store.clone(),
      token: TOKEN.to_owned(),
      console: Some(console),
    };
    let serving = tokio::spawn(async move {
      axum::serve(listener, router(brain)).await.unwrap();
    });
    let client = BrainClient::new(&base, TOKEN).unwrap();
    Served {
      database,
      store,
      client,
      base,
      serving,
    }
  }

  /// A client presenting another token.
  pub fn with_token(&self, token: &str) -> BrainClient {
    BrainClient::new(&self.base, token).unwrap()
  }

  /// The database's address, as a caller handed one opens it.
  pub fn address(&self) -> String {
    let address = test_address();
    let (base, query) = address
      .split_once('?')
      .map_or((address.as_str(), None), |(base, query)| (base, Some(query)));
    let base = base.rsplit_once('/').map(|(head, _)| head).unwrap_or(base);

    match query {
      Some(query) => format!("{base}/{}?{query}", self.database),
      None => format!("{base}/{}", self.database),
    }
  }

  /// The events published so far, every kind.
  pub async fn event_kinds(&self) -> Vec<String> {
    let page = self.client.events(&bmo_brain_client::EventsQuery::default()).await.unwrap();
    page.items.into_iter().map(|event| event.kind).collect()
  }
}

/// A memory row as a test puts it in the table: no route adds one, so a test writes the row the console reads.
#[derive(Debug, Clone, Default)]
pub struct Row {
  pub scope: String,
  pub subject_key: Option<String>,
  pub kind: String,
  pub text: String,
  pub conditions: Vec<bmo_brain_client::Object>,
  pub labels: bmo_brain_client::Labels,
  pub due_at: Option<i64>,
}

/// A row of a scope, a kind and a text, every other field empty.
pub fn row(scope: &str, kind: &str, text: &str) -> Row {
  Row {
    scope: scope.to_owned(),
    kind: kind.to_owned(),
    text: text.to_owned(),
    ..Row::default()
  }
}

/// A fragment as a test puts it in the table: no route writes one.
#[derive(Debug, Clone, Default)]
pub struct Piece {
  pub key: String,
  pub kind: String,
  pub scope: Option<String>,
  pub subject_key: Option<String>,
  pub title: Option<String>,
  pub text: String,
  pub library_name: Option<String>,
}

/// A fragment of a key, a kind, a scope and a text, every other field empty.
pub fn piece(key: &str, kind: &str, scope: Option<&str>, text: &str) -> Piece {
  Piece {
    key: key.to_owned(),
    kind: kind.to_owned(),
    scope: scope.map(str::to_owned),
    text: text.to_owned(),
    ..Piece::default()
  }
}

impl Served {
  /// A standing row put in the table; its id.
  pub async fn put_row(&self, row: Row) -> i64 {
    sqlx::query_scalar(
      "INSERT INTO scoped_memories (scope, subject_key, kind, text, state, conditions, labels, due_at, \
       created_at, changed_at) VALUES ($1, $2, $3, $4, 'standing', $5, $6, $7, 1, 1) RETURNING id",
    )
    .bind(&row.scope)
    .bind(&row.subject_key)
    .bind(&row.kind)
    .bind(&row.text)
    .bind(sqlx::types::Json(&row.conditions))
    .bind(sqlx::types::Json(&row.labels))
    .bind(row.due_at)
    .fetch_one(&self.store.pool)
    .await
    .unwrap()
  }

  /// An active fragment put in the table, switched off for none; its id.
  pub async fn put_fragment(&self, piece: Piece) -> i64 {
    sqlx::query_scalar(
      "INSERT INTO fragments (key, kind, scope, subject_key, title, path, text, version, library_name, state, \
       switched_off, labels, created_at, changed_at) \
       VALUES ($1, $2, $3, $4, $5, NULL, $6, 'v1', $7, 'active', '{}', '{}', 1, 1) RETURNING id",
    )
    .bind(&piece.key)
    .bind(&piece.kind)
    .bind(&piece.scope)
    .bind(&piece.subject_key)
    .bind(&piece.title)
    .bind(&piece.text)
    .bind(&piece.library_name)
    .fetch_one(&self.store.pool)
    .await
    .unwrap()
  }

  /// A description of a fragment kind put in the table.
  pub async fn put_kind(&self, name: &str, description: &str) {
    sqlx::query("INSERT INTO fragment_kinds (name, description) VALUES ($1, $2)")
      .bind(name)
      .bind(description)
      .execute(&self.store.pool)
      .await
      .unwrap();
  }
}

impl Drop for Served {
  fn drop(&mut self) {
    self.serving.abort();
    drop_copy(std::mem::take(&mut self.database));
  }
}

/// The failure a call came to: its status and code.
pub fn refused<T: std::fmt::Debug>(answer: bmo_brain_client::Answer<T>) -> (u16, String) {
  match answer {
    Err(bmo_brain_client::ClientFailure::Answered {
      status,
      answer: Some(answer),
    }) => (status, answer.code),
    other => panic!("a refusal was due, and the call came to {other:?}"),
  }
}

/// Labels from pairs.
pub fn labels(pairs: &[(&str, &str)]) -> bmo_brain_client::Labels {
  pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
}
