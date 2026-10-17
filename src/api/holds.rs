//! The brain holds (docs/people-book.md, 「A job editing the brain holds the editors off」; docs/boundary-api.md,
//! 「Brain holds」): a job's hold set for its run, which jobs hold the brain and which agent folders' editing locks
//! a brain editor has taken, and the wait until no editor holds any. A hold ends with its job's run; nothing here
//! lifts one.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::{Answer, Api, ApiError, Body};
use crate::editors::fragments::store::now_seconds;
use crate::editors::lock::{brain_holders, brain_locks_taken, place_brain_hold};

/// How long the wait lasts at most, and how often it looks again.
pub const WAIT_MOST: Duration = Duration::from_secs(540);
pub const WAIT_POLL: Duration = Duration::from_secs(5);

/// The brain holds' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/brain/hold", post(hold).get(holders))
    .route("/brain/wait", post(wait))
}

#[derive(Debug, Deserialize)]
struct HoldAsked {
  run: String,
}

async fn hold(State(api): State<Arc<Api>>, Body(asked): Body<HoldAsked>) -> Answer {
  if asked.run.trim().is_empty() {
    return Err(ApiError::bad_request("A hold is set for a job's run, by its name."));
  }

  place_brain_hold(&api.db, &asked.run, now_seconds()).await?;

  Ok(Json(json!({ "held": true, "taken": brain_locks_taken(&api.data_root) })))
}

// Which jobs hold the brain, a hold whose job's run is over cleared as it is read, and which locks are taken.
async fn holders(State(api): State<Arc<Api>>) -> Answer {
  let holders = brain_holders(&api.db, api.agency.as_ref(), None).await?;

  Ok(Json(json!({ "holders": holders, "taken": brain_locks_taken(&api.data_root) })))
}

/// Waits until no brain editor holds any editing lock, looking every `poll` for `most` at most: whether every lock
/// is free at the end.
pub async fn locks_free(data_root: &std::path::Path, most: Duration, poll: Duration) -> bool {
  let deadline = tokio::time::Instant::now() + most;

  while !brain_locks_taken(data_root).is_empty() && tokio::time::Instant::now() < deadline {
    tokio::time::sleep(poll).await;
  }

  brain_locks_taken(data_root).is_empty()
}

async fn wait(State(api): State<Arc<Api>>) -> Answer {
  let free = locks_free(&api.data_root, WAIT_MOST, WAIT_POLL).await;
  let line = if free {
    "every editing lock is free; the brain may be written"
  } else {
    "an editing lock is still taken after nine minutes; ask again"
  };

  Ok(Json(json!({ "free": free, "answered": true, "lines": [line] })))
}
