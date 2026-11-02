//! `GET /events`: the stream, or the same events as a page to a reader asking for JSON (docs/boundary-api.md,
//! 「Events」).

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::sse::{Event as Sent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use bmo_brain_client::{Event, EventsQuery};
use futures_util::{StreamExt, stream};
use tokio::sync::watch;

use super::{Answer, Brain, Query};
use crate::store::events::Narrowing;
use crate::store::{Store, limit};

/// How often a quiet stream looks again without being told of a change.
const LOOK_AGAIN: Duration = Duration::from_secs(1);

/// How often a comment line keeps the stream open.
const KEEP_OPEN: Duration = Duration::from_secs(15);

/// How many events a stream reads at once.
const BATCH: i64 = 100;

struct Reading {
  store: Store,
  cursor: i64,
  narrowing: Narrowing,
  moved: watch::Receiver<u64>,
  ready: VecDeque<Event>,
}

fn sent(event: &Event) -> Sent {
  Sent::default()
    .event(event.kind.clone())
    .id(event.id.to_string())
    .json_data(event)
    .unwrap_or_else(|_| Sent::default().comment("an event that did not render"))
}

pub(super) async fn events(
  State(brain): State<Brain>,
  headers: HeaderMap,
  Query(query): Query<EventsQuery>,
) -> Answer<Response> {
  let resumed = headers
    .get("last-event-id")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.trim().parse::<i64>().ok());
  let after = resumed.or(query.after);
  let narrowing = Narrowing::of(query.kinds.as_deref(), query.scope.clone());
  let paged = headers
    .get(header::ACCEPT)
    .and_then(|value| value.to_str().ok())
    .is_some_and(|accepted| accepted.contains("application/json"));
  let stopping = brain.stopping;
  let store = brain.store;
  if paged {
    let limit = limit(query.limit)?;
    return Ok(Json(store.events(after.unwrap_or(0), &narrowing, limit).await?).into_response());
  }
  let cursor = match after {
    Some(after) => {
      store.events(after, &narrowing, 1).await?;
      after
    }
    None => store.latest_event().await?,
  };
  let reading = Reading {
    moved: store.moved(),
    store,
    cursor,
    narrowing,
    ready: VecDeque::new(),
  };
  let events = stream::unfold(reading, |mut reading| async move {
    loop {
      if let Some(event) = reading.ready.pop_front() {
        return Some((Ok::<_, Infallible>(sent(&event)), reading));
      }
      let page = reading.store.events(reading.cursor, &reading.narrowing, BATCH).await.ok()?;
      if let Some(last) = page.items.last() {
        reading.cursor = last.id;
        reading.ready.extend(page.items);
        continue;
      }
      let _ = tokio::time::timeout(LOOK_AGAIN, reading.moved.changed()).await;
    }
  });
  // The stream ends with the service's stop, so the shutdown waits for nothing that never ends.
  let events = events.take_until(stopping.heard());
  Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_OPEN)).into_response())
}
