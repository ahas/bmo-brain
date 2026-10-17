//! The console's routes (docs/memories.md, 「The console」; docs/fragments.md, 「A standing set by hand」;
//! docs/boundary-api.md, 「Memory rows」 and 「Fragments」): every memory row, or one agent's or one room's, newest
//! first; one row taken down with its reason or put back; and one fragment switched off or on by a person's hand.
//! The switches are the only writes the console makes, and nothing here removes a row.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use brain_support::database::{all, one};
use brain_support::js::MAX_SAFE_INTEGER;
use sea_orm::{DatabaseConnection, Value as Bound};
use serde_json::{Map, Value, json};

use super::fragments::Fragment;
use super::memories::{MemoryRow, row_of};
use super::{Answer, Api, ApiError, Body, whole};
use crate::Failure;
use crate::editors::fragments::kind_labels;
use crate::editors::fragments::store::{now_seconds, set_hand};

/// The console's routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/memories", get(memories))
    .route("/memories/active", post(memory_active))
    .route("/fragments/active", post(fragment_active))
}

/// A room's id as JavaScript's parseInt reads one off a query: leading digits after an optional sign, past any
/// whitespace, within the whole numbers a double holds exactly.
pub fn leading_whole(text: &str) -> Option<i64> {
  let text = text.trim_start();
  let (sign, digits) = match text.strip_prefix('-') {
    Some(rest) => (-1, rest),
    None => (1, text.strip_prefix('+').unwrap_or(text)),
  };
  let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
  let value = sign * digits.parse::<i64>().ok()?;

  (value.abs() <= MAX_SAFE_INTEGER).then_some(value)
}

// A number a body may leave out, which a fragment belonging to every room or to nobody names: the outer none is
// the refusal, since what arrived was neither.
fn whole_or_null(value: Option<&Value>) -> Option<Option<i64>> {
  match value {
    None | Some(Value::Null) => Some(None),
    Some(value) => whole(value).map(Some),
  }
}

async fn memories(State(api): State<Arc<Api>>, Query(asked): Query<HashMap<String, String>>) -> Answer {
  let mut terms = vec![];
  let mut values: Vec<Bound> = vec![];

  if let Some(agent) = asked.get("agent") {
    values.push(agent.clone().into());
    terms.push(format!(r#""agent" = ${}"#, values.len()));
  }

  if let Some(chat) = asked.get("chat") {
    let Some(chat_id) = leading_whole(chat) else {
      return Err(ApiError::bad_request(format!("chat takes a whole number, not {chat}")));
    };

    values.push(chat_id.into());
    terms.push(format!(r#""chat_id" = ${}"#, values.len()));
  }

  let filter = if terms.is_empty() { String::new() } else { format!("WHERE {}", terms.join(" AND ")) };
  let rows = all(&api.db, &format!(r#"SELECT * FROM "memories" {filter} ORDER BY "id" DESC"#), values)
    .await
    .map_err(Failure::from)?;
  let memories = rows.iter().map(row_of).collect::<Result<Vec<_>, _>>()?;

  Ok(Json(json!({ "memories": memories })))
}

async fn memory_by_slug(db: &DatabaseConnection, slug: &str) -> Result<Option<MemoryRow>, Failure> {
  let row = one(db, r#"SELECT * FROM "memories" WHERE "slug" = $1"#, vec![slug.into()]).await?;

  row.as_ref().map(row_of).transpose()
}

// Words a body carries with their edges trimmed, or nothing where it carries none.
fn text<'a>(body: &'a Map<String, Value>, field: &str) -> &'a str {
  body.get(field).and_then(Value::as_str).map(str::trim).unwrap_or_default()
}

async fn memory_active(State(api): State<Arc<Api>>, Body(body): Body<Map<String, Value>>) -> Answer {
  let (slug, why) = (text(&body, "slug"), text(&body, "why"));
  let Some(active) = body.get("active").and_then(Value::as_bool).filter(|_| !slug.is_empty()) else {
    return Err(ApiError::bad_request("slug (string) and active (boolean) are required"));
  };
  let Some(standing) = memory_by_slug(&api.db, slug).await? else {
    return Err(ApiError::not_found(format!("No memory is named {slug}.")));
  };

  if !active && why.is_empty() {
    return Err(ApiError::bad_request(
      "why (string) is required: a memory is not taken down without a reason.",
    ));
  }

  match change(&api.db, &standing, active, why).await? {
    Ok(memory) => Ok(Json(json!({ "ok": true, "memory": memory }))),
    Err(refusal) => Err(ApiError::conflict(refusal, format!("memory {slug}: {refusal}"))),
  }
}

// The row as it stands after the change, or the refusal code of why it was refused.
async fn change(
  db: &DatabaseConnection,
  standing: &MemoryRow,
  active: bool,
  why: &str,
) -> Result<Result<MemoryRow, &'static str>, Failure> {
  // The scope a row belongs to is its own: read again, so a row moved or gone since is refused.
  let Some(current) = memory_by_slug(db, &standing.slug).await? else {
    return Ok(Err("missing"));
  };

  if current.chat_id != standing.chat_id {
    return Ok(Err("other-room"));
  }

  if current.active == active {
    return Ok(Err(if active { "already-up" } else { "already-down" }));
  }

  let row = if active {
    one(
      db,
      r#"UPDATE "memories" SET "active" = 1, "down_reason" = NULL, "down_at" = NULL WHERE "slug" = $1 RETURNING *"#,
      vec![standing.slug.clone().into()],
    )
    .await?
  } else {
    one(
      db,
      r#"UPDATE "memories" SET "active" = 0, "down_reason" = $1, "down_at" = $2 WHERE "slug" = $3 RETURNING *"#,
      vec![why.into(), now_seconds().into(), standing.slug.clone().into()],
    )
    .await?
  };

  Ok(match row {
    Some(row) => Ok(row_of(&row)?),
    None => Err("missing"),
  })
}

async fn fragment_active(State(api): State<Arc<Api>>, Body(body): Body<Map<String, Value>>) -> Answer {
  let kinds = kind_labels(&api.db).await?;
  let kind = body
    .get("kind")
    .and_then(Value::as_str)
    .filter(|kind| kinds.iter().any(|(known, _)| known == kind));
  let slug = text(&body, "slug");
  let active = body.get("active").and_then(Value::as_bool);
  let (Some(kind), Some(active)) = (kind, active.filter(|_| !slug.is_empty())) else {
    return Err(ApiError::bad_request(
      "kind (a fragment kind), slug (string) and active (boolean) are required",
    ));
  };
  let (Some(chat_id), Some(owner)) = (whole_or_null(body.get("chatId")), whole_or_null(body.get("owner")))
  else {
    return Err(ApiError::bad_request("chatId and owner take a whole number or null"));
  };
  // The row is the named agent's, the chat agent's where none is named.
  let agent = match body.get("agent") {
    None | Some(Value::Null) => api.chat_key.as_str(),
    Some(Value::String(agent)) if !agent.trim().is_empty() => agent.trim(),
    Some(_) => return Err(ApiError::bad_request("agent takes an agent's registration name or null")),
  };
  let set = set_hand(&api.db, agent, (kind, chat_id, owner, slug), active, now_seconds())
    .await
    .map_err(Failure::from)?;
  let Some(row) = set else {
    return Err(ApiError::not_found(format!("No fragment is named {kind}/{slug}.")));
  };

  Ok(Json(json!({ "ok": true, "fragment": Fragment::of(row) })))
}
