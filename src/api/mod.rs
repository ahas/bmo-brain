//! The brain's routes (docs/boundary-api.md): each a thin call of the store, behind the bearer token, every
//! failure answered in the common error shape.

mod brains;
pub mod console;
mod events;

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, MatchedPath, Request, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use bmo_brain_client::{ErrorCode, Health, Role, WireError};
use serde::de::DeserializeOwned;
use tokio::sync::watch;

use crate::store::{Fault, Store};

use self::console::{Console, Reach, SCREEN_ROLE};

/// What the routes stand on: the store, the service's token, the console the brain serves a bundle to, where it
/// serves one, and the service's stop as its open streams hear it.
#[derive(Debug, Clone)]
pub struct Brain {
  pub store: Store,
  pub token: String,
  pub console: Option<Console>,
  pub stopping: Stopping,
}

/// The service's stop as an open stream hears it: raised once, when the service is asked to end, so a stream
/// that would otherwise never end lets the service's shutdown finish.
#[derive(Debug, Clone)]
pub struct Stopping(watch::Receiver<bool>);

impl Stopping {
  /// The sender that raises the stop, and the stop the streams hear.
  pub fn new() -> (watch::Sender<bool>, Self) {
    let (raise, heard) = watch::channel(false);
    (raise, Self(heard))
  }

  /// Ends once the stop is raised, and never where its sender is gone without raising it.
  pub async fn heard(mut self) {
    if self.0.wait_for(|raised| *raised).await.is_err() {
      std::future::pending::<()>().await;
    }
  }
}

/// Whose token a call carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Caller {
  /// The service's: every route but the console bundle's own.
  Service,
  /// The console's, relaying for a reader: what the screen uses (docs/boundary-api.md, 「The console bundle」).
  Console,
}

/// The header the console names the reader's role in.
const OWNER_ROLE: &str = "owner-role";

/// A failure, answered with its code's status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
  pub code: ErrorCode,
  pub error: String,
}

impl ApiError {
  pub fn new(code: ErrorCode, error: impl Into<String>) -> Self {
    Self {
      code,
      error: error.into(),
    }
  }
}

impl From<Fault> for ApiError {
  fn from(fault: Fault) -> Self {
    let code = match &fault {
      Fault::BadRequest(_) => ErrorCode::BadRequest,
      Fault::Forbidden(_) => ErrorCode::Forbidden,
      Fault::NotFound(_) => ErrorCode::NotFound,
      Fault::Conflict(_) => ErrorCode::Conflict,
      Fault::Gone(_) => ErrorCode::Gone,
      Fault::Failed(_) => ErrorCode::Failed,
    };
    Self::new(code, fault.to_string())
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    let status = StatusCode::from_u16(self.code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = WireError {
      code: self.code.name().to_owned(),
      error: self.error,
    };
    (status, Json(body)).into_response()
  }
}

/// What a route answers.
pub type Answer<T> = Result<T, ApiError>;

/// A JSON body of the route's shape, or a 400 naming what is wrong with it.
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
  type Rejection = ApiError;

  async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
    match Json::<T>::from_request(request, state).await {
      Ok(Json(body)) => Ok(Self(body)),
      Err(rejection) => Err(refused(&rejection)),
    }
  }
}

fn refused(rejection: &JsonRejection) -> ApiError {
  ApiError::new(
    ErrorCode::BadRequest,
    format!("The body is not the shape this route takes: {}", rejection.body_text()),
  )
}

/// A query of the route's shape, or a 400 naming what is wrong with it.
pub struct Query<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequestParts<S> for Query<T> {
  type Rejection = ApiError;

  async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
    match axum::extract::Query::<T>::from_request_parts(parts, state).await {
      Ok(axum::extract::Query(query)) => Ok(Self(query)),
      Err(rejection) => Err(query_refused(&rejection)),
    }
  }
}

fn query_refused(rejection: &QueryRejection) -> ApiError {
  ApiError::new(
    ErrorCode::BadRequest,
    format!("The query is not the shape this route takes: {}", rejection.body_text()),
  )
}

/// A key the path names, or a 400 where it does not read.
pub struct Key<T>(pub T);

impl<T: DeserializeOwned + Send, S: Send + Sync> FromRequestParts<S> for Key<T> {
  type Rejection = ApiError;

  async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
    match axum::extract::Path::<T>::from_request_parts(parts, state).await {
      Ok(axum::extract::Path(key)) => Ok(Self(key)),
      Err(rejection) => Err(path_refused(&rejection)),
    }
  }
}

fn path_refused(rejection: &PathRejection) -> ApiError {
  ApiError::new(ErrorCode::BadRequest, format!("The path does not read: {}", rejection.body_text()))
}

// Every route needs a token as a bearer: the service's, or the console's where the brain serves one. An empty
// token lets nobody in.
async fn authorised(State(brain): State<Brain>, mut request: Request, next: Next) -> Response {
  let presented = request
    .headers()
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .filter(|token| !token.is_empty());
  let console = brain.console.as_ref().map(|console| console.token.as_str());
  let caller = match presented {
    Some(token) if token == brain.token => Caller::Service,
    Some(token) if Some(token) == console => Caller::Console,
    _ => {
      return ApiError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong.")
        .into_response();
    }
  };
  request.extensions_mut().insert(caller);
  next.run(request).await
}

// The console bundle's own routes are the console's token's alone. The console's token reaches the manifest and
// the words with no role, the routes the screen uses with an `Owner-Role` allowing the screen's, and nothing
// else.
async fn guarded(request: Request, next: Next) -> Response {
  let path = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str).unwrap_or_default();
  if request.extensions().get::<Caller>() == Some(&Caller::Service)
    && console::console_only(request.method().as_str(), path)
  {
    return ApiError::new(ErrorCode::Unauthorized, "The console's token alone reaches the console bundle.")
      .into_response();
  }
  if request.extensions().get::<Caller>() == Some(&Caller::Console) {
    match console::reach(request.method().as_str(), path) {
      Reach::Open => {}
      Reach::Screen => {
        let role = request
          .headers()
          .get(OWNER_ROLE)
          .and_then(|value| value.to_str().ok())
          .and_then(Role::named);
        if !role.is_some_and(|role| role.allows(SCREEN_ROLE)) {
          return ApiError::new(
            ErrorCode::Forbidden,
            format!("The request names no role allowing the screen's, {}.", SCREEN_ROLE.name()),
          )
          .into_response();
        }
      }
      Reach::Closed => {
        return ApiError::new(ErrorCode::Forbidden, "The console's token does not reach this route.")
          .into_response();
      }
    }
  }
  next.run(request).await
}

/// The discipline a chat agent's system prompt carries (docs/design.md, 「Writing」).
pub const DISCIPLINE: &str = include_str!("../../skills/brain-harness/every-turn.md");

async fn discipline() -> Response {
  ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], DISCIPLINE).into_response()
}

async fn no_route() -> ApiError {
  ApiError::new(ErrorCode::NoRoute, "No route answers at this method and path.")
}

async fn health(State(brain): State<Brain>) -> Json<Health> {
  let reason = brain.store.unready().await;
  let searches = if reason.is_none() { brain.store.search_counts().await } else { Vec::new() };
  let embedding = brain.store.embedding_health().await;
  Json(Health {
    ready: reason.is_none(),
    reason,
    version: crate::VERSION.to_owned(),
    rules: brain.store.rules.clone(),
    searches,
    embedding,
  })
}

/// The routes, on what they stand on.
pub fn router(brain: Brain) -> Router {
  Router::new()
    .route("/health", axum::routing::get(health))
    .route("/events", axum::routing::get(events::events))
    .route("/discipline", axum::routing::get(discipline))
    .route("/console-manifest", axum::routing::get(console::manifest))
    .route("/console-bundle/words", axum::routing::get(console::words))
    .route("/console-bundle/{file}", axum::routing::get(console::file))
    .merge(brains::routes())
    .route_layer(middleware::from_fn(guarded))
    .fallback(no_route)
    .method_not_allowed_fallback(no_route)
    .layer(middleware::from_fn_with_state(brain.clone(), authorised))
    .with_state(brain)
}
