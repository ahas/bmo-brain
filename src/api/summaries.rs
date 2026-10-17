//! The chat-session summaries (docs/boundary-api.md, 「Chat-session summaries」): the task the summariser is handed
//! for an agent's chat session, and the handoff note a chat agent's folded session leaves for its next one, kept as
//! it came in the brain's settings, one per agent, the next keep overwriting it, read until a clear forgets it.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::{Answer, Api, ApiError, Body};
use crate::editors::prompts::chat_summary_task;
use crate::{Failure, settings};

/// The summaries' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/session-summary/task", post(task))
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
    return Err(ApiError::bad_request("A summary route names its agent, by its registration name."));
  }

  Ok(())
}

// The summariser's task for the agent's chat session, the agent named by its registration's label, else its name,
// and by the name asked for where the agency registers none.
async fn task(State(api): State<Arc<Api>>, Body(asked): Body<Named>) -> Answer {
  named(&asked.agent)?;

  let registration = api
    .agency
    .registrations(Some(&asked.agent))
    .await
    .map_err(Failure::from)?
    .into_iter()
    .find(|registration| registration.name == asked.agent);
  let label = registration
    .and_then(|registration| registration.label)
    .filter(|label| !label.trim().is_empty())
    .unwrap_or_else(|| asked.agent.clone());
  let task = chat_summary_task(&api.prompts_root, &label).map_err(|why| ApiError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    code: "failed",
    error: format!("The summariser's task could not be read: {why}."),
    refusal: None,
  })?;

  Ok(Json(json!({ "task": task })))
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
