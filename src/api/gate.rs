//! The routes the gate reads a turn's raw material by and hands its facts back on (docs/boundary-api.md, 「Memory
//! rows」, 「Fragments」 and 「Rooms leaving」): a room's standing memory rows, its matcher's candidates and what the
//! matcher made of them, the rows a turn's events wake, the fragment rows a room may be offered and what a turn
//! attached, and a forgotten room let go of.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use memory_rows::events::Event;
use serde::Deserialize;
use serde_json::{Value, json};

use super::fragments::{self, Hit};
use super::{Answer, Api, Body, memories};
use crate::Failure;
use crate::editors::fragments::store::now_seconds;

/// The gate's routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/memory/standing", post(memory_standing))
    .route("/memory/conditions", post(memory_conditions))
    .route("/memory/matched", post(memory_matched))
    .route("/memory/events", post(memory_events))
    .route("/fragments/offer", post(fragments_offer))
    .route("/fragments/hits", post(fragments_hits))
    .route("/rooms/forgotten", post(rooms_forgotten))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRoom {
  agent: String,
  chat_id: i64,
}

async fn memory_standing(State(api): State<Arc<Api>>, Body(asked): Body<AgentRoom>) -> Answer {
  let rows = memories::standing(&api.db, asked.chat_id, &asked.agent, now_seconds()).await?;

  Ok(Json(json!({ "rows": rows })))
}

async fn memory_conditions(State(api): State<Arc<Api>>, Body(asked): Body<AgentRoom>) -> Answer {
  let room = api.receiver.roads(asked.chat_id).await.map_err(Failure::from)?.kind;
  let rows =
    memories::conditions(&api.db, &api.withering, asked.chat_id, room, &asked.agent, now_seconds()).await?;
  let rows: Vec<Value> = rows
    .into_iter()
    .map(|row| json!({ "slug": row.slug, "condition": row.condition, "action": row.action }))
    .collect();

  Ok(Json(json!({ "rows": rows })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Matched {
  agent: String,
  chat_id: i64,
  matched: Vec<String>,
  spent: Vec<String>,
}

async fn memory_matched(State(api): State<Arc<Api>>, Body(asked): Body<Matched>) -> Answer {
  let room = api.receiver.roads(asked.chat_id).await.map_err(Failure::from)?.kind;

  memories::matched(
    &api.db,
    asked.chat_id,
    room,
    &asked.agent,
    (&asked.matched, &asked.spent),
    now_seconds(),
  )
  .await?;

  Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Events {
  agent: String,
  chat_id: i64,
  events: Vec<Event>,
}

async fn memory_events(State(api): State<Arc<Api>>, Body(asked): Body<Events>) -> Answer {
  let room = api.receiver.roads(asked.chat_id).await.map_err(Failure::from)?.kind;
  let rows =
    memories::woken(&api.db, asked.chat_id, room, &asked.agent, &asked.events, now_seconds()).await?;

  Ok(Json(json!({ "rows": rows })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Offer {
  agent: String,
  chat_id: i64,
  kinds: Vec<String>,
}

async fn fragments_offer(State(api): State<Arc<Api>>, Body(asked): Body<Offer>) -> Answer {
  let fragments =
    fragments::offer(&api.db, api.receiver.as_ref(), &asked.agent, asked.chat_id, &asked.kinds).await?;

  Ok(Json(json!({ "fragments": fragments })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hits {
  agent: String,
  // The turn's room, which the wire names and nothing reads: each attached row names its own.
  #[allow(dead_code)]
  chat_id: i64,
  attached: Vec<Hit>,
}

async fn fragments_hits(State(api): State<Arc<Api>>, Body(asked): Body<Hits>) -> Answer {
  fragments::hits(&api.db, &asked.agent, &asked.attached, now_seconds()).await?;

  Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Forgotten {
  chat_id: i64,
}

async fn rooms_forgotten(State(api): State<Arc<Api>>, Body(asked): Body<Forgotten>) -> Answer {
  api.withering.forget(asked.chat_id);

  Ok(Json(json!({ "ok": true })))
}
