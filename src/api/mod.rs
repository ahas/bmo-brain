//! The brain's routes (docs/boundary-api.md): each a thin call of the store, behind the bearer token, every
//! failure answered in the common error shape.

mod events;
mod routes;

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use bmo_brain_client::{ErrorCode, Health, WireError};
use serde::de::DeserializeOwned;

use crate::store::{Fault, Store};

/// What the routes stand on: the store and the token every caller presents.
#[derive(Debug, Clone)]
pub struct Brain {
  pub store: Store,
  pub token: String,
}

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

// Every route needs the brain's token as a bearer.
async fn authorised(State(brain): State<Brain>, request: Request, next: Next) -> Response {
  let presented = request
    .headers()
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "));
  if presented != Some(brain.token.as_str()) {
    return ApiError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong.").into_response();
  }
  next.run(request).await
}

async fn no_route() -> ApiError {
  ApiError::new(ErrorCode::NoRoute, "No route answers at this method and path.")
}

async fn health(State(brain): State<Brain>) -> Json<Health> {
  let reason = brain.store.unready().await;
  Json(Health {
    ready: reason.is_none(),
    reason,
    version: env!("CARGO_PKG_VERSION").to_owned(),
  })
}

/// The routes, on what they stand on.
pub fn router(brain: Brain) -> Router {
  Router::new()
    .route("/health", axum::routing::get(health))
    .route("/events", axum::routing::get(events::events))
    .merge(routes::routes())
    .fallback(no_route)
    .method_not_allowed_fallback(no_route)
    .layer(middleware::from_fn_with_state(brain.clone(), authorised))
    .with_state(brain)
}
