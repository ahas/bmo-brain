//! The routes the gate reads a turn's raw material by and hands its facts back on (docs/boundary-api.md, 「Memory
//! rows」, 「Fragments」 and 「Rooms leaving」): a room's standing memory rows, its matcher's candidates and what the
//! matcher made of them, the rows a turn's events wake, the fragment rows a room may be offered and what a turn
//! attached, and a forgotten room let go of; and a shipped library's entries its owner hands in, with the words a
//! screen calls every kind by (「Shipped libraries」). The brain selects nothing here; it answers for one agent at a
//! time, and every route takes the brain's token and answers by the wire conventions.

pub mod fragments;
pub mod memories;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use memory_rows::events::Event;
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use self::fragments::Hit;
use self::memories::Withering;
use crate::Failure;
use crate::editors::fragments::store::{ShippedEntry, now_seconds};
use crate::editors::fragments::{ShippedFailure, Table, kind_labels};
use crate::ports::agency::Agency;
use crate::ports::receiver::Receiver;

/// What the routes stand on: the brain's database, the agency and the receiver, the brain root and the chat agent's
/// registration key whose memory it is, the token every caller presents, and the withering stamps the conditions
/// route keeps for the service's life.
pub struct Api {
  pub db: DatabaseConnection,
  pub agency: Arc<dyn Agency>,
  pub receiver: Arc<dyn Receiver>,
  pub brain_root: PathBuf,
  pub chat_key: String,
  pub token: String,
  pub withering: Withering,
}

/// A non-2xx answer: its status, its code and the English sentence saying what went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
  pub status: StatusCode,
  pub code: &'static str,
  pub error: String,
}

impl ApiError {
  fn bad_request(error: impl Into<String>) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      code: "bad-request",
      error: error.into(),
    }
  }
}

impl From<Failure> for ApiError {
  fn from(failure: Failure) -> Self {
    Self {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      code: "failed",
      error: failure.to_string(),
    }
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (self.status, Json(json!({ "code": self.code, "error": self.error }))).into_response()
  }
}

/// A JSON body of the route's shape, or a 400 naming what is wrong with it.
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
  type Rejection = ApiError;

  async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
    match Json::<T>::from_request(request, state).await {
      Ok(Json(body)) => Ok(Self(body)),
      Err(rejection) => Err(ApiError::bad_request(rejected(&rejection))),
    }
  }
}

fn rejected(rejection: &JsonRejection) -> String {
  format!("The body is not the shape this route takes: {}", rejection.body_text())
}

// Every route needs the brain's token as a bearer.
async fn authorised(State(api): State<Arc<Api>>, request: Request, next: Next) -> Response {
  let presented = request
    .headers()
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "));

  if presented != Some(api.token.as_str()) {
    return ApiError {
      status: StatusCode::UNAUTHORIZED,
      code: "unauthorized",
      error: "The bearer token is missing or wrong.".to_owned(),
    }
    .into_response();
  }

  next.run(request).await
}

async fn no_route() -> ApiError {
  ApiError {
    status: StatusCode::NOT_FOUND,
    code: "no-route",
    error: "No route answers at this method and path.".to_owned(),
  }
}

/// The routes, on what they stand on.
pub fn router(api: Arc<Api>) -> Router {
  Router::new()
    .route("/memory/standing", post(memory_standing))
    .route("/memory/conditions", post(memory_conditions))
    .route("/memory/matched", post(memory_matched))
    .route("/memory/events", post(memory_events))
    .route("/fragments/offer", post(fragments_offer))
    .route("/fragments/hits", post(fragments_hits))
    .route("/rooms/forgotten", post(rooms_forgotten))
    .route("/libraries/reconcile", post(libraries_reconcile))
    .route("/fragments/kinds", get(fragments_kinds))
    .fallback(no_route)
    .layer(middleware::from_fn_with_state(api.clone(), authorised))
    .with_state(api)
}

type Answer = Result<Json<Value>, ApiError>;

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reconcile {
  kind: String,
  label: String,
  entries: Vec<ShippedEntry>,
}

async fn libraries_reconcile(State(api): State<Arc<Api>>, Body(asked): Body<Reconcile>) -> Answer {
  if asked.kind.trim().is_empty() || asked.label.trim().is_empty() {
    return Err(ApiError::bad_request(
      "A shipped library is handed in with a kind and the word a screen calls it by.",
    ));
  }

  let table =
    Table::of(&api.db, api.agency.as_ref(), api.receiver.as_ref(), &api.brain_root, &api.chat_key).await?;

  match table.reconcile_shipped(&asked.kind, &asked.label, &asked.entries).await {
    Ok(done) => Ok(Json(json!({ "answered": true, "lines": [done.line()] }))),
    Err(ShippedFailure::BrainLibrary) => Ok(Json(json!({
      "answered": false,
      "refusal": {
        "code": "brain-library",
        "message": "The kind is one of the brain's own libraries, whose rows only its editors write.",
        "kind": asked.kind,
      },
    }))),
    Err(ShippedFailure::Failed(failure)) => Err(failure.into()),
  }
}

async fn fragments_kinds(State(api): State<Arc<Api>>) -> Answer {
  let kinds: Vec<Value> = kind_labels(&api.db)
    .await?
    .into_iter()
    .map(|(kind, label)| json!({ "kind": kind, "label": label }))
    .collect();

  Ok(Json(json!({ "kinds": kinds })))
}
