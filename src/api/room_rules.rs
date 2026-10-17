//! The owner's room rules served (docs/boundary-api.md, 「Owner room rules」): a room's rules read, and one rule
//! replaced or revoked on a message the receiver confirms the owner sent.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{Answer, Api, ApiError, Body, refusal, whole};
use crate::Failure;
use crate::room_rules::{Unchanged, change_rule, read_rules};

/// The room rules' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new().route("/room-rules/read", post(read)).route("/room-rules/set", post(set))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadAsked {
  chat_id: i64,
}

async fn read(State(api): State<Arc<Api>>, Body(asked): Body<ReadAsked>) -> Answer {
  Ok(Json(json!({ "rules": read_rules(&api.data_root, asked.chat_id)? })))
}

#[derive(Debug, Deserialize)]
struct RuleAsked {
  subject: Value,
  text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetAsked {
  chat_id: i64,
  message_id: i64,
  id: String,
  rule: Option<RuleAsked>,
}

async fn set(State(api): State<Arc<Api>>, Body(asked): Body<SetAsked>) -> Answer {
  // A rule is changed only on a message the room's owner sent, before anything else of the request is read.
  if !api
    .receiver
    .sender_is_owner(asked.chat_id, asked.message_id)
    .await
    .map_err(Failure::from)?
  {
    return Ok(refusal(
      "not-owner",
      "Room rules change only on a message the room's owner sent.",
      Map::new(),
      vec![],
    ));
  }

  let rule = match &asked.rule {
    None => None,
    Some(rule) => match whole(&rule.subject) {
      Some(subject) => Some((subject, rule.text.as_str())),
      None => return Err(ApiError::bad_request("invalid rule: the subject is no safe whole number")),
    },
  };

  match change_rule(&api.data_root, asked.chat_id, &asked.id, rule) {
    Ok(rules) => Ok(Json(json!({ "rules": rules }))),
    Err(Unchanged::Invalid(problem)) => Err(ApiError::bad_request(problem)),
    Err(Unchanged::Busy) => Err(ApiError::conflict(
      "busy",
      format!("room {}: another change of its rules holds the lock", asked.chat_id),
    )),
    Err(Unchanged::Failed(failure)) => Err(failure.into()),
  }
}
