//! The brain's store: its database, the rules every member is held to, and the events each change publishes
//! in the same transaction as the change. Each resource is a module of its own; the routes and the clock both
//! work through them.

pub mod base;
pub mod baseline;
pub mod brains;
pub mod consolidation;
pub mod duplicates;
pub mod events;
pub mod files;
pub mod held;
pub mod learning;
pub mod misses;
pub mod orgs;
pub mod recall;
pub mod rules;
pub mod sheets;
pub mod tallies;
pub mod vectors;
pub mod versions;
pub mod words;

use std::time::{SystemTime, UNIX_EPOCH};

use bmo_brain_client::Page;
use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use tokio::sync::watch;

/// The migrations, built into the binary.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Why a request was not carried out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
  /// The request is not the shape the route takes; the sentence names what is wrong.
  BadRequest(String),
  /// The writer's standing does not allow the change.
  Forbidden(String),
  /// Nothing stands at the key named.
  NotFound(String),
  /// The member's state does not allow the change.
  Conflict(String),
  /// A holder keeps the member from change: only its own next declaration or a withdrawal of its key ends it.
  Locked(String),
  /// The member stood once and is let go of.
  Gone(String),
  /// The store failed.
  Failed(String),
}

impl std::fmt::Display for Fault {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::BadRequest(why)
      | Self::Forbidden(why)
      | Self::NotFound(why)
      | Self::Conflict(why)
      | Self::Locked(why)
      | Self::Gone(why)
      | Self::Failed(why) => out.write_str(why),
    }
  }
}

impl From<sqlx::Error> for Fault {
  fn from(error: sqlx::Error) -> Self {
    Self::Failed(format!("The brain's database failed: {error}."))
  }
}

/// What a store call answers.
pub type Held<T> = Result<T, Fault>;

pub use rules::Rules;

/// The database, the rules the brain reads by, and the signal every committed change raises so waiting
/// readers look again.
#[derive(Debug, Clone)]
pub struct Store {
  pub pool: PgPool,
  pub rules: Rules,
  pub vectors: std::sync::Arc<vectors::Vectors>,
  moved: watch::Sender<u64>,
}

impl Store {
  /// The store over a database address, not yet connected: it connects on its first use.
  pub fn lazy(address: &str) -> Result<Self, String> {
    let options: PgConnectOptions =
      address.parse().map_err(|why| format!("the database address: {why}"))?;
    Ok(Self::over(PgPoolOptions::new().max_connections(16).connect_lazy_with(options)))
  }

  /// The store over a pool already made, reading by the default rules.
  pub fn over(pool: PgPool) -> Self {
    Self {
      pool,
      rules: Rules::default(),
      vectors: std::sync::Arc::new(vectors::Vectors::new(None)),
      moved: watch::Sender::new(0),
    }
  }

  /// The same store searching by meaning with the embedding model named, or by words alone where none is.
  pub fn embedding(mut self, config: Option<vectors::EmbeddingConfig>) -> Self {
    self.vectors = std::sync::Arc::new(vectors::Vectors::new(config));
    self
  }

  /// The same store reading by these rules.
  pub fn by(mut self, rules: Rules) -> Self {
    self.rules = rules;
    self
  }

  /// The migrations run, from the baseline (docs/design.md, 「The schema」).
  pub async fn migrate(&self) -> Result<(), String> {
    baseline::migrate(&self.pool).await
  }

  /// Why the store cannot answer, or nothing where it can.
  pub async fn unready(&self) -> Option<String> {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&self.pool).await {
      Ok(_) => None,
      Err(why) => Some(format!("The database does not answer: {why}.")),
    }
  }

  /// A signal raised after every committed change.
  pub fn moved(&self) -> watch::Receiver<u64> {
    self.moved.subscribe()
  }

  pub(crate) async fn begin(&self) -> Held<Transaction<'static, Postgres>> {
    Ok(self.pool.begin().await?)
  }

  /// A transaction committed, and every waiting reader told.
  pub(crate) async fn commit(&self, tx: Transaction<'static, Postgres>) -> Held<()> {
    tx.commit().await?;
    self.moved.send_modify(|count| *count = count.wrapping_add(1));
    self.vectors.wake();
    Ok(())
  }

  pub(crate) async fn connection(&self) -> Held<sqlx::pool::PoolConnection<Postgres>> {
    Ok(self.pool.acquire().await?)
  }
}

/// Now, in epoch milliseconds.
pub fn now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
    .unwrap_or(0)
}

/// How many items a page holds where the caller names none, and at most.
pub const PAGE: i64 = 50;
pub const MOST: i64 = 500;

/// The page size asked for, or why it is refused.
pub(crate) fn limit(asked: Option<i64>) -> Held<i64> {
  match asked {
    None => Ok(PAGE),
    Some(limit) if (1..=MOST).contains(&limit) => Ok(limit),
    Some(limit) => Err(Fault::BadRequest(format!("limit is {limit}; it is 1 to {MOST}."))),
  }
}

/// A page of `limit` items out of `limit + 1` read, saying whether more stand.
pub(crate) fn page<T>(mut items: Vec<T>, limit: i64) -> Page<T> {
  let limit = usize::try_from(limit).unwrap_or(usize::MAX);
  let more = items.len() > limit;
  items.truncate(limit);
  Page { items, more }
}

/// The lock every event is published under, so event numbers are committed in the order they grow.
const EVENT_ORDER: i64 = 0x0062_7261_696e;

/// An event published in the transaction of the change it tells of.
pub(crate) async fn publish<T: Serialize>(
  conn: &mut PgConnection,
  kind: &str,
  scope: Option<&str>,
  data: &T,
) -> Held<()> {
  let data = serde_json::to_value(data).map_err(|why| Fault::Failed(why.to_string()))?;
  sqlx::query("SELECT pg_advisory_xact_lock($1)")
    .bind(EVENT_ORDER)
    .execute(&mut *conn)
    .await?;
  sqlx::query("INSERT INTO events (kind, scope, at, data) VALUES ($1, $2, $3, $4)")
    .bind(kind)
    .bind(scope)
    .bind(now())
    .bind(sqlx::types::Json(data))
    .execute(&mut *conn)
    .await?;
  Ok(())
}

/// A key: 1 to 200 characters, none of them a control character or whitespace. A key never stands in a
/// path, so a slash is as good a character as any other.
pub(crate) fn key(field: &str, value: &str) -> Held<()> {
  let length = value.chars().count();
  let fits = (1..=200).contains(&length) && !value.chars().any(|c| c.is_control() || c.is_whitespace());
  if fits {
    Ok(())
  } else {
    Err(Fault::BadRequest(format!(
      "{field} {value:?} is no key: 1 to 200 characters, none of them a control character or whitespace."
    )))
  }
}

/// A text that is not blank and at most `most` characters.
pub(crate) fn text(field: &str, value: &str, most: usize) -> Held<()> {
  if value.trim().is_empty() {
    return Err(Fault::BadRequest(format!("{field} is blank.")));
  }
  let length = value.chars().count();
  if length > most {
    return Err(Fault::BadRequest(format!("{field} is {length} characters; it is at most {most}.")));
  }
  Ok(())
}

/// The most characters a row's text holds.
pub const TEXT_MOST: usize = 64_000;

/// The most characters one of a memory's conditions holds.
pub const CONDITION_MOST: usize = 1_000;

/// A comma-separated list as its items, blanks left out.
pub(crate) fn listed(value: Option<&str>) -> Vec<String> {
  value
    .unwrap_or_default()
    .split(',')
    .map(str::trim)
    .filter(|item| !item.is_empty())
    .map(str::to_owned)
    .collect()
}

/// A pattern matching a text holding these words, any case, with LIKE's own marks taken literally.
pub(crate) fn holding(words: &str) -> String {
  let escaped = words.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
  format!("%{escaped}%")
}
