//! The brain's routes (docs/boundary-api.md, 「Routes the brain serves」): what the gate reads a turn's raw material
//! by and hands its facts back on, the memory tools' and the fragment table's routes, the shipped libraries, the
//! digests, the brain holds and the chat-session summaries, each subject in a module of its own. The brain selects
//! nothing; it answers for one agent at a time, and every route takes the brain's token and answers by the wire
//! conventions.

pub mod digests;
pub mod fragments;
pub mod gate;
pub mod holds;
pub mod memories;
pub mod records;
pub mod summaries;
pub mod table;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use brain_support::database::one;
use sea_orm::DatabaseConnection;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use self::memories::Withering;
use crate::Failure;
use crate::editors::fragments::Table;
use crate::editors::ground::Ground;
use crate::ports::agency::Agency;
use crate::ports::gate::Gate;
use crate::ports::receiver::Receiver;
use crate::schedules::launches::Launches;

/// What the routes stand on: the brain's database and the address the fragment tool opens it by, the agency, the
/// receiver and the gate, the brain root, the data root and the prompts folder, the chat agent's registration key
/// and the owner's number, the unit the service runs as and the words that start its binary, the token every caller
/// presents, and the withering stamps the conditions route keeps for the service's life.
pub struct Api {
  pub db: DatabaseConnection,
  pub database: String,
  pub agency: Arc<dyn Agency>,
  pub receiver: Arc<dyn Receiver>,
  pub gate: Arc<dyn Gate>,
  pub brain_root: PathBuf,
  pub data_root: PathBuf,
  pub prompts_root: PathBuf,
  pub chat_key: String,
  pub owner: Option<i64>,
  pub service_unit: String,
  pub program: Vec<String>,
  pub token: String,
  pub withering: Withering,
}

impl Api {
  /// What an editor run started inside a request stands on: no unit and no job run of its own.
  pub fn ground(&self) -> Ground<'_> {
    self.ground_under(None)
  }

  /// What a run launched apart stands on: the unit it runs under, which is also the run its clock, its brain hold
  /// and a failure's record are named by.
  pub fn ground_under<'a>(&'a self, unit: Option<&'a str>) -> Ground<'a> {
    Ground {
      db: &self.db,
      database: &self.database,
      agency: self.agency.as_ref(),
      receiver: self.receiver.as_ref(),
      gate: self.gate.as_ref(),
      brain_root: &self.brain_root,
      data_root: &self.data_root,
      prompts_root: &self.prompts_root,
      chat_key: &self.chat_key,
      service_unit: &self.service_unit,
      unit,
      own_run: unit,
    }
  }

  /// The runs the brain launches apart.
  pub fn launches(&self) -> Launches<'_> {
    Launches {
      ground: self.ground(),
      program: &self.program,
    }
  }

  /// The fragment table over the brain as the agency registers its folders now.
  pub async fn table(&self) -> Result<Table<'_>, Failure> {
    self.ground().table().await
  }
}

/// A non-2xx answer: its status, its code and the English sentence saying what went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
  pub status: StatusCode,
  pub code: &'static str,
  pub error: String,
}

impl ApiError {
  /// A body the route cannot read, the sentence naming what is wrong with it.
  pub fn bad_request(error: impl Into<String>) -> Self {
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

/// What a route answers: a JSON object, or the wire's error.
pub type Answer = Result<Json<Value>, ApiError>;

/// A reading carried out: its lines.
pub fn reading(lines: Vec<String>) -> Json<Value> {
  Json(json!({ "answered": true, "lines": lines }))
}

/// A request understood and not carried out: its code, its English sentence and the code's own parameters, with
/// the lines it wrote before it stopped where it wrote any.
pub fn refusal(code: &str, message: &str, params: Map<String, Value>, lines: Vec<String>) -> Json<Value> {
  let mut refusal = params;

  refusal.insert("code".to_owned(), json!(code));
  refusal.insert("message".to_owned(), json!(message));

  let mut answer = Map::new();

  answer.insert("answered".to_owned(), json!(false));
  answer.insert("refusal".to_owned(), Value::Object(refusal));

  if !lines.is_empty() {
    answer.insert("lines".to_owned(), json!(lines));
  }

  Json(Value::Object(answer))
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

// Whether the service can answer: its database answers, and its brain folder is a git repository.
async fn health(State(api): State<Arc<Api>>) -> Json<Value> {
  let reason = if let Err(why) = one(&api.db, "SELECT 1 AS \"one\"", vec![]).await {
    Some(format!("The database does not answer: {why}"))
  } else if !api.brain_root.join(".git").exists() {
    Some("The brain folder is no git repository.".to_owned())
  } else {
    None
  };

  Json(json!({ "ready": reason.is_none(), "reason": reason }))
}

/// The routes, on what they stand on.
pub fn router(api: Arc<Api>) -> Router {
  Router::new()
    .route("/health", get(health))
    .merge(gate::routes())
    .merge(records::routes())
    .merge(table::routes())
    .merge(digests::routes())
    .merge(holds::routes())
    .merge(summaries::routes())
    .fallback(no_route)
    .layer(middleware::from_fn_with_state(api.clone(), authorised))
    .with_state(api)
}
