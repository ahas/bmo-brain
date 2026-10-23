//! The events stream and page, the token, the routes no one serves, and health (docs/boundary-api.md,
//! 「Common rules」, 「Health」 and 「Events」).

mod support;

use std::time::Duration;

use bmo_brain::clock::pass;
use bmo_brain::store::events::WINDOW_MS;
use bmo_brain::store::now;
use bmo_brain_client::{EventsQuery, NewBrain};
use futures_util::StreamExt;
use support::{Served, TOKEN, refused};

fn brain(agent: &str) -> NewBrain {
  NewBrain {
    account_key: "account-a".into(),
    agent_key: agent.into(),
    name: None,
  }
}

#[tokio::test]
async fn the_page_reads_events_past_a_cursor_narrowed_by_kind_and_scope() {
  let served = Served::new().await;
  let client = &served.client;
  let first = client.brain_add(&brain("agent-a")).await.unwrap();
  let second = client.brain_add(&brain("agent-b")).await.unwrap();

  let all = client.events(&EventsQuery::default()).await.unwrap();
  assert_eq!(all.items.len(), 2);
  assert_eq!(all.items[0].data, serde_json::to_value(&first).unwrap());
  let past = client
    .events(&EventsQuery {
      after: Some(all.items[0].id),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(past.items, all.items[1..]);
  let of_scope = client
    .events(&EventsQuery {
      scope: Some(format!("brain:{}", second.id)),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(of_scope.items, all.items[1..]);
  let of_kind = client
    .events(&EventsQuery {
      kinds: Some("memory.written".into()),
      ..Default::default()
    })
    .await
    .unwrap();
  assert!(of_kind.items.is_empty());
}

#[tokio::test]
async fn a_resume_from_before_the_window_is_gone() {
  let served = Served::new().await;
  served.client.brain_add(&brain("agent-a")).await.unwrap();
  pass(&served.store, now() + WINDOW_MS + 1_000).await.unwrap();
  assert_eq!(
    refused(
      served
        .client
        .events(&EventsQuery {
          after: Some(0),
          ..Default::default()
        })
        .await
    ),
    (410, "gone".into())
  );
}

#[tokio::test]
async fn the_stream_hands_over_each_event_with_its_kind_and_number() {
  let served = Served::new().await;
  let first = served.client.brain_add(&brain("agent-a")).await.unwrap();
  let answer = reqwest::Client::new()
    .get(format!("{}/events?after=0", served.base))
    .bearer_auth(TOKEN)
    .send()
    .await
    .unwrap();
  assert_eq!(answer.headers()["content-type"], "text/event-stream");
  let mut bytes = answer.bytes_stream();
  let mut read = String::new();
  while !read.contains("\n\n") {
    let chunk = tokio::time::timeout(Duration::from_secs(20), bytes.next())
      .await
      .unwrap()
      .unwrap()
      .unwrap();
    read.push_str(&String::from_utf8_lossy(&chunk));
  }
  assert!(read.contains("event: brain.created\n"), "{read}");
  let id = read.lines().find_map(|line| line.strip_prefix("id: ")).unwrap();
  let data: serde_json::Value =
    serde_json::from_str(read.lines().find_map(|line| line.strip_prefix("data: ")).unwrap()).unwrap();
  assert_eq!(data["id"].to_string(), id);
  assert_eq!(data["data"], serde_json::to_value(&first).unwrap());
}

#[tokio::test]
async fn every_route_takes_the_token_and_a_route_no_one_serves_is_no_route() {
  let served = Served::new().await;
  assert_eq!(refused(served.with_token("wrong").health().await), (401, "unauthorized".into()));
  let health = served.client.health().await.unwrap();
  assert!(health.ready);
  assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
  let http = reqwest::Client::new();
  for (method, path) in [
    (reqwest::Method::GET, "/nowhere"),
    (reqwest::Method::GET, "/memories"),
    (reqwest::Method::POST, "/memories"),
    (reqwest::Method::GET, "/memories/1"),
    (reqwest::Method::PATCH, "/memories/1"),
    (reqwest::Method::GET, "/fragments"),
    (reqwest::Method::PATCH, "/fragments/tea"),
    (reqwest::Method::GET, "/fragment-kinds"),
    (reqwest::Method::GET, "/notes"),
    (reqwest::Method::GET, "/watches"),
    (reqwest::Method::GET, "/reminders"),
    (reqwest::Method::GET, "/libraries"),
    (reqwest::Method::GET, "/knowledge"),
    (reqwest::Method::GET, "/locks"),
    (reqwest::Method::GET, "/scopes/room:a"),
    (reqwest::Method::POST, "/room-rules/read"),
    (reqwest::Method::POST, "/sessions/loads"),
    (reqwest::Method::POST, "/session-summary/read"),
  ] {
    let answer = http
      .request(method, format!("{}{path}", served.base))
      .bearer_auth(TOKEN)
      .send()
      .await
      .unwrap();
    assert_eq!(answer.status(), 404);
    let body: serde_json::Value = answer.json().await.unwrap();
    assert_eq!(body["code"], "no-route");
  }
  let unread = http
    .patch(format!("{}/brains/1/memories/1", served.base))
    .bearer_auth(TOKEN)
    .header("content-type", "application/json")
    .body("{\"state\": 1}")
    .send()
    .await
    .unwrap();
  assert_eq!(unread.status(), 400);
}
