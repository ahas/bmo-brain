//! A unit stood in for on a local port: it answers each path with the status and body a test set, or with a stream
//! of server-sent events once another path was asked, and records every request's method, path, query, bearer
//! token and body, so a client test reads what went over the wire.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use bmo_brain::ports::http::Peer;
use futures_util::stream::{self, StreamExt};
use serde_json::{Value, json};

/// The token every stub is called with.
pub const TOKEN: &str = "stub-token";

/// One request a stub took.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
  pub method: String,
  pub path: String,
  /// The query as it went, none where the request carried none.
  pub query: Option<String>,
  pub bearer: Option<String>,
  pub body: Value,
}

// A stream of events a path answers, sent once the path it waits on was asked and then held open.
#[derive(Clone)]
struct Events {
  after: String,
  events: Vec<(String, Value)>,
}

#[derive(Default)]
struct Book {
  answers: BTreeMap<String, (StatusCode, Value)>,
  streams: BTreeMap<String, Events>,
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

  /// Answers `path` with a stream of these events, each its kind and its data, sent once `after` was asked; the
  /// stream is held open after them.
  pub fn answer_events(&self, path: &str, after: &str, events: Vec<(&str, Value)>) -> &Self {
    let events = Events {
      after: after.to_owned(),
      events: events.into_iter().map(|(kind, data)| (kind.to_owned(), data)).collect(),
    };
    self.book.lock().unwrap().streams.insert(path.to_owned(), events);
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

// The events of a stream, once the path they wait on was asked, and then nothing more while the stream stays open.
fn streamed(book: Arc<Mutex<Book>>, events: Events) -> Response {
  let waited = stream::once(async move {
    while !book.lock().unwrap().taken.iter().any(|taken| taken.path == events.after) {
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
    stream::iter(
      events
        .events
        .into_iter()
        .map(|(kind, data)| Ok::<_, Infallible>(Event::default().event(kind).data(data.to_string()))),
    )
  })
  .flatten()
  .chain(stream::pending());

  Sse::new(waited).into_response()
}

async fn answer(
  State(book): State<Arc<Mutex<Book>>>,
  method: Method,
  uri: Uri,
  headers: HeaderMap,
  body: Bytes,
) -> Response {
  let bearer = headers
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .map(str::to_owned);
  let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
  let path = uri.path().to_owned();
  let shared = book.clone();
  let mut book = book.lock().unwrap();
  book.taken.push(Taken {
    method: method.to_string(),
    path: path.clone(),
    query: uri.query().map(str::to_owned),
    bearer,
    body,
  });
  if let Some(events) = book.streams.get(&path).cloned() {
    drop(book);
    return streamed(shared, events);
  }
  let (status, answer) = book
    .answers
    .get(&path)
    .cloned()
    .unwrap_or((StatusCode::NOT_FOUND, json!({ "code": "no-route", "error": "No route at this path." })));
  (status, axum::Json(answer)).into_response()
}
