//! A room and an agent leaving the brain, as the routes answer them (docs/boundary-api.md, 「Rooms leaving」 and
//! 「Agents forgetting」): what a room holds counted, or removed; an agent's memory counted, or reset; an agent's
//! editing lock forgotten; and the pieces a caller's own reset of an agent stands on: its folder fenced, its lock's
//! path, its book of a room made to read from a moment, and what its stopped runs left put back.

use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, json};

use super::{Answer, Api, Body, refusal};
use crate::Failure;
use crate::editors::archive::set_reading_since;
use crate::editors::commits::put_back;
use crate::editors::fragments::store::now_seconds;
use crate::editors::lock::{self, brain_lock_path};
use crate::lifecycle::{Reset, agent_folder, agent_holding, remove_room, reset_agent, room_holding};

/// The rooms' and agents' leaving routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/rooms/removed", post(room_removed))
    .route("/agents/reset", post(agent_reset))
    .route("/agents/removed", post(agent_removed))
    .route("/agents/fence", post(agent_fence))
    .route("/agents/lock", post(agent_lock))
    .route("/agents/book-since", post(agent_book_since))
    .route("/agents/put-back", post(agent_put_back))
}

// The refusal of a reset or put-back naming the agent whose folder is the brain root, which holds every other
// agent's folder.
fn brain_root() -> Answer {
  Ok(refusal(
    "brain-root",
    "The agent's memory is the brain root, which holds every other agent's folder; neither a reset nor a put-back takes it.",
    Map::new(),
    vec![],
  ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomAsked {
  chat_id: i64,
  #[serde(default)]
  preview: bool,
}

async fn room_removed(State(api): State<Arc<Api>>, Body(asked): Body<RoomAsked>) -> Answer {
  let (holding, folders) =
    room_holding(&api.db, api.agency.as_ref(), &api.brain_root, asked.chat_id).await?;

  if asked.preview {
    return Ok(Json(json!({ "took": holding, "couldNot": [] })));
  }

  let could_not =
    remove_room(&api.db, &api.data_root, &api.brain_root, asked.chat_id, &holding, &folders).await?;

  Ok(Json(json!({ "took": holding, "couldNot": could_not })))
}

#[derive(Debug, Deserialize)]
struct AgentAsked {
  agent: String,
  #[serde(default)]
  preview: bool,
}

async fn agent_reset(State(api): State<Arc<Api>>, Body(asked): Body<AgentAsked>) -> Answer {
  let folder = agent_folder(api.agency.as_ref(), &asked.agent).await?;

  // The brain root is the chat agent's whole memory, and no reset takes it.
  if asked.agent == api.chat_key || folder.as_deref() == Some("") {
    return brain_root();
  }

  let holding = agent_holding(&api.db, &api.brain_root, &asked.agent, folder.as_deref()).await?;

  if asked.preview {
    return Ok(Json(json!({ "took": holding, "couldNot": [] })));
  }

  let could_not = reset_agent(&Reset {
    db: &api.db,
    receiver: api.receiver.as_ref(),
    brain: &api.brain_root,
    data_root: &api.data_root,
    agent: &asked.agent,
    folder: folder.as_deref(),
    now: now_seconds(),
  })
  .await?;

  Ok(Json(json!({ "took": holding, "couldNot": could_not })))
}

// A body naming one agent and nothing else.
#[derive(Debug, Deserialize)]
struct AgentNamed {
  agent: String,
}

async fn agent_removed(State(api): State<Arc<Api>>, Body(asked): Body<AgentNamed>) -> Answer {
  let left = match agent_folder(api.agency.as_ref(), &asked.agent).await? {
    Some(folder) if !folder.is_empty() => lock::forget(&api.data_root, &folder),
    _ => vec![],
  };

  Ok(Json(json!({ "ok": true, "left": left })))
}

async fn agent_fence(State(api): State<Arc<Api>>, Body(asked): Body<AgentNamed>) -> Answer {
  if let Some(folder) = agent_folder(api.agency.as_ref(), &asked.agent).await? {
    lock::fence(&api.data_root, &folder).map_err(|why| {
      Failure::Disk(format!("{}: {why}", brain_lock_path(&api.data_root, &folder).display()))
    })?;
  }

  Ok(Json(json!({ "ok": true })))
}

async fn agent_lock(State(api): State<Arc<Api>>, Body(asked): Body<AgentNamed>) -> Answer {
  // The lock's path as it stands under any data root, which is the path relative to this one.
  let path = agent_folder(api.agency.as_ref(), &asked.agent)
    .await?
    .map(|folder| brain_lock_path(Path::new(""), &folder).to_string_lossy().into_owned());

  Ok(Json(json!({ "path": path })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BookSinceAsked {
  agent: String,
  chat_id: i64,
  at: i64,
}

async fn agent_book_since(State(api): State<Arc<Api>>, Body(asked): Body<BookSinceAsked>) -> Answer {
  set_reading_since(&api.db, &asked.agent, asked.chat_id, Some(asked.at)).await?;

  Ok(Json(json!({ "ok": true })))
}

async fn agent_put_back(State(api): State<Arc<Api>>, Body(asked): Body<AgentNamed>) -> Answer {
  let folder = agent_folder(api.agency.as_ref(), &asked.agent).await?;

  if asked.agent == api.chat_key || folder.as_deref() == Some("") {
    return brain_root();
  }

  if let Some(folder) = folder {
    put_back(&api.brain_root, &folder)
      .map_err(|why| Failure::Disk(format!("putting back {folder}: {why}")))?;
  }

  Ok(Json(json!({ "ok": true })))
}
