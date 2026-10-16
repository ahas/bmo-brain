//! A unit stood in for on a local port: it answers each path with the status and body a test set, and
//! records every request's path, bearer token and body, so a client test reads what went over the wire.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::IntoResponse;
use bmo_brain::ports::http::Peer;
use serde_json::{Value, json};

/// The token every stub is called with.
pub const TOKEN: &str = "stub-token";

/// One request a stub took.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
  pub path: String,
  pub bearer: Option<String>,
  pub body: Value,
}

#[derive(Default)]
struct Book {
  answers: BTreeMap<String, (StatusCode, Value)>,
  taken: Vec<Taken>,
}

/// A stub server, listening until the test ends.
#[derive(Clone)]
pub struct Stub {
  book: Arc<Mutex<Book>>,
  pub peer: Peer,
}

impl Stub {
  pub async fn start() -> Self {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let book = Arc::new(Mutex::new(Book::default()));
    let app = Router::new().fallback(answer).with_state(book.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Self {
      book,
      peer: Peer {
        base: format!("http://{address}/"),
        token: TOKEN.to_owned(),
      },
    }
  }

  /// Answers `path` with 200 and `body`.
  pub fn answer(&self, path: &str, body: Value) -> &Self {
    self.answer_with(path, StatusCode::OK, body)
  }

  pub fn answer_with(&self, path: &str, status: StatusCode, body: Value) -> &Self {
    self.book.lock().unwrap().answers.insert(path.to_owned(), (status, body));
    self
  }

  /// Every request so far.
  pub fn taken(&self) -> Vec<Taken> {
    self.book.lock().unwrap().taken.clone()
  }

  /// The one request the stub took at `path`, which a test expects there to be.
  pub fn body_at(&self, path: &str) -> Value {
    let taken: Vec<Taken> = self.taken().into_iter().filter(|taken| taken.path == path).collect();
    assert_eq!(taken.len(), 1, "{path} was asked {} times", taken.len());
    assert_eq!(taken[0].bearer.as_deref(), Some(TOKEN), "{path} was asked without the token");
    taken[0].body.clone()
  }
}

async fn answer(
  State(book): State<Arc<Mutex<Book>>>,
  uri: Uri,
  headers: HeaderMap,
  body: Bytes,
) -> impl IntoResponse {
  let bearer = headers
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .map(str::to_owned);
  let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
  let path = uri.path().to_owned();
  let mut book = book.lock().unwrap();
  book.taken.push(Taken {
    path: path.clone(),
    bearer,
    body,
  });
  let (status, answer) = book
    .answers
    .get(&path)
    .cloned()
    .unwrap_or((StatusCode::NOT_FOUND, json!({ "code": "no-route", "error": "No route at this path." })));
  (status, axum::Json(answer))
}
