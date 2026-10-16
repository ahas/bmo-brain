//! A database read through a place it was handed, whichever database stands there: a `postgres:`
//! address, or a SQLite file opened read-only. A statement read this way numbers its placeholders `$1`,
//! `$2` in the order they first appear, which both databases take.

use std::path::Path;
use std::time::{Duration, Instant};

use sqlx::any::install_default_drivers;
use sqlx::{AnyConnection, Connection};

/// Whether a place names a PostgreSQL database rather than a file.
pub fn is_postgres_place(place: &str) -> bool {
  place.starts_with("postgres://") || place.starts_with("postgresql://")
}

/// Where a database handed by its file stands now: a data root's `bmo.db` is read at the address its
/// `env/archive-url` setting holds where that holds one, so a reader handed the file before the database
/// moved reads where it moved to.
pub fn current_place(place: &Path) -> String {
  let text = place.to_string_lossy().into_owned();

  if is_postgres_place(&text) || place.file_name().is_none_or(|name| name != "bmo.db") {
    return text;
  }

  let setting = place.parent().map(|root| root.join("env").join("archive-url"));
  let address = setting
    .and_then(|path| std::fs::read_to_string(path).ok())
    .map(|text| text.trim().to_owned());

  match address {
    Some(address) if !address.is_empty() => address,
    _ => text,
  }
}

// The server's own codes for going down, not being up yet, or having no connection left to give.
const GONE_STATES: [&str; 4] = ["57P01", "57P02", "57P03", "53300"];

/// Whether an error is the database server going away rather than the statement's own fault: the
/// connection refused, absent or cut, a connection exception, or the server's codes for a restart.
pub fn server_went_away(error: &sqlx::Error) -> bool {
  match error {
    sqlx::Error::Io(_) => true,
    sqlx::Error::Database(fault) => fault
      .code()
      .is_some_and(|code| code.starts_with("08") || GONE_STATES.contains(&code.as_ref())),
    _ => false,
  }
}

// A restart of the server is over in seconds; a minute covers one that waits on a checkpoint.
const RECONNECT_MS: u64 = 60_000;
const FIRST_PAUSE_MS: u64 = 100;
const LAST_PAUSE_MS: u64 = 2_000;

/// Work run again whole wherever the server went away under it, with a doubling pause between
/// attempts, until the reconnect budget (`BMO_ARCHIVE_RECONNECT_MS`, a minute by default) is spent;
/// every other error, and the last one past the budget, is answered as it came. The work opens its
/// own connection, so each attempt stands on a fresh one.
pub async fn retrying<T, F, Fut>(mut work: F) -> Result<T, sqlx::Error>
where
  F: FnMut() -> Fut,
  Fut: Future<Output = Result<T, sqlx::Error>>,
{
  let budget = Duration::from_millis(positive_from_env("BMO_ARCHIVE_RECONNECT_MS", RECONNECT_MS));
  let started = Instant::now();
  let mut pause = Duration::from_millis(FIRST_PAUSE_MS);
  let mut attempt = 1;

  loop {
    match work().await {
      Err(error) if server_went_away(&error) && started.elapsed() + pause <= budget => {
        attempt += 1;
        eprintln!(
          "archive: the database went away ({error}); running the work again in {} ms, attempt {attempt}",
          pause.as_millis()
        );
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(Duration::from_millis(LAST_PAUSE_MS));
      }
      answer => return answer,
    }
  }
}

/// A connection that reads the database at a place and never writes it.
pub async fn open_read(place: &Path) -> Result<AnyConnection, sqlx::Error> {
  install_default_drivers();

  let place = current_place(place);
  if is_postgres_place(&place) {
    return retrying(|| AnyConnection::connect(&place)).await;
  }

  AnyConnection::connect(&format!("sqlite://{place}?mode=ro")).await
}

/// A whole number above zero the environment names, or the default.
pub fn positive_from_env(name: &str, default: u64) -> u64 {
  std::env::var(name)
    .ok()
    .and_then(|value| value.trim().parse::<u64>().ok())
    .filter(|value| *value > 0)
    .unwrap_or(default)
}
