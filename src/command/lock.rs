//! `bmo brain hold` and `bmo brain wait` (docs/boundary-api.md, 「The command」): the locks that say who is
//! editing the store, asked of 「Locks」's routes.

use bmo_brain_client::{BrainClient, Lock, LocksQuery, NewLock};
use clap::{Args, Subcommand};

use super::moment_text;

/// How long `brain wait` waits where no span is given: the longest the route waits.
const WAIT_WITHIN_MS: i64 = 600_000;

/// The forms `bmo brain` takes.
#[derive(Subcommand)]
pub enum Word {
  /// Takes the lock and prints it.
  Hold(Hold),
  /// Waits until no lock stands, or the wait runs out, and prints the locks that stand.
  Wait(Wait),
}

/// A lock to take.
#[derive(Args)]
pub struct Hold {
  /// The caller's name for who holds it.
  #[arg(long)]
  holder: String,
  /// Why it is taken.
  #[arg(long, allow_hyphen_values = true)]
  reason: String,
  /// How long it stands, in milliseconds; until it is let go where none is given.
  #[arg(long = "ttl-ms")]
  ttl_ms: Option<i64>,
}

/// How long to wait for no lock to stand.
#[derive(Args)]
pub struct Wait {
  /// How long to wait, in milliseconds, at most ten minutes.
  #[arg(long = "wait-ms", default_value_t = WAIT_WITHIN_MS)]
  wait_ms: i64,
  /// The last id seen.
  #[arg(long)]
  after: Option<i64>,
  /// How many at most.
  #[arg(long)]
  limit: Option<i64>,
}

/// One lock as the command prints it.
pub fn line(lock: &Lock) -> String {
  let until = lock.expires_at.map_or_else(|| "it is let go".to_owned(), moment_text);
  format!("brain: lock {} held by {} until {}: {}", lock.id, lock.holder_name, until, lock.reason)
}

/// Runs the form its words name.
pub async fn run(client: &BrainClient, word: Word) -> Result<Vec<String>, String> {
  match word {
    Word::Hold(asked) => hold(client, asked).await,
    Word::Wait(asked) => wait(client, asked).await,
  }
}

async fn hold(client: &BrainClient, asked: Hold) -> Result<Vec<String>, String> {
  let taken = client
    .lock(&NewLock {
      holder_name: asked.holder,
      reason: asked.reason,
      ttl_ms: asked.ttl_ms,
    })
    .await
    .map_err(|why| format!("brain: {why}"))?;

  Ok(vec![line(&taken)])
}

async fn wait(client: &BrainClient, asked: Wait) -> Result<Vec<String>, String> {
  let page = client
    .locks(&LocksQuery {
      wait_ms: Some(asked.wait_ms),
      after: asked.after,
      limit: asked.limit,
    })
    .await
    .map_err(|why| format!("brain: {why}"))?;

  if page.items.is_empty() {
    return Ok(vec!["brain: no lock stands.".to_owned()]);
  }
  Ok(page.items.iter().map(line).collect())
}
