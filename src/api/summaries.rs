//! The chat-session summaries (docs/boundary-api.md, 「Chat-session summaries」): the handoff note a chat agent's
//! folded session leaves for its next one, kept as it came in the brain's settings, one per agent, the next keep
//! overwriting it, read until a clear forgets it.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::{Answer, Api, ApiError, Body};
use crate::{Failure, settings};

/// The summaries' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/session-summary/keep", post(keep))
    .route("/session-summary/read", post(read))
    .route("/session-summary/clear", post(clear))
}

/// The settings key an agent's handoff note is kept under.
pub fn summary_key(agent: &str) -> String {
  format!("{agent}_pending_summary")
}

#[derive(Debug, Deserialize)]
struct Kept {
  agent: String,
  note: String,
}

#[derive(Debug, Deserialize)]
struct Named {
  agent: String,
}

fn named(agent: &str) -> Result<(), ApiError> {
  if agent.trim().is_empty() {
    return Err(ApiError::bad_request("A summary is kept for an agent, by its registration name."));
  }

  Ok(())
}

async fn keep(State(api): State<Arc<Api>>, Body(asked): Body<Kept>) -> Answer {
  named(&asked.agent)?;
  settings::set(&api.db, &summary_key(&asked.agent), &asked.note)
    .await
    .map_err(Failure::from)?;

  Ok(Json(json!({ "ok": true })))
}

async fn read(State(api): State<Arc<Api>>, Body(asked): Body<Named>) -> Answer {
  named(&asked.agent)?;

  let note = settings::get(&api.db, &summary_key(&asked.agent)).await.map_err(Failure::from)?;

  Ok(Json(json!({ "note": note })))
}

async fn clear(State(api): State<Arc<Api>>, Body(asked): Body<Named>) -> Answer {
  named(&asked.agent)?;
  settings::forget(&api.db, &summary_key(&asked.agent)).await.map_err(Failure::from)?;

  Ok(Json(json!({ "ok": true })))
}
