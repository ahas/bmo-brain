//! A room and an agent leaving the brain, as the routes answer them (docs/boundary-api.md, 「Rooms leaving」 and
//! 「Agents forgetting」): what a room holds counted, or removed; an agent's memory counted, or reset; and an agent's
//! editing lock forgotten.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, json};

use super::{Answer, Api, Body, refusal};
use crate::editors::fragments::store::now_seconds;
use crate::editors::lock;
use crate::lifecycle::{Reset, agent_folder, agent_holding, remove_room, reset_agent, room_holding};

/// The rooms' and agents' leaving routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/rooms/removed", post(room_removed))
    .route("/agents/reset", post(agent_reset))
    .route("/agents/removed", post(agent_removed))
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
    return Ok(refusal(
      "brain-root",
      "The agent's memory is the brain root, which a reset does not take.",
      Map::new(),
      vec![],
    ));
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

#[derive(Debug, Deserialize)]
struct RemovedAsked {
  agent: String,
}

async fn agent_removed(State(api): State<Arc<Api>>, Body(asked): Body<RemovedAsked>) -> Answer {
  let left = match agent_folder(api.agency.as_ref(), &asked.agent).await? {
    Some(folder) if !folder.is_empty() => lock::forget(&api.data_root, &folder),
    _ => vec![],
  };

  Ok(Json(json!({ "ok": true, "left": left })))
}
