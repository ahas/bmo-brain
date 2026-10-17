//! The console's routes served on a local port and called as the web unit calls them (docs/boundary-api.md,
//! 「Memory rows」 and 「Fragments」): every memory row or one agent's or room's, a row taken down and put back, and
//! a fragment switched by a person's hand.

mod support;

use bmo_brain::api::console::leading_whole;
use bmo_brain::ports::http::PortFailure;
use serde_json::{Value, json};

use support::served::{Served, caller, served};
use support::{AGENT, OTHER_AGENT, ROOM};

impl Served {
  async fn memory_row(&self, agent: &str, slug: &str, chat_id: Option<i64>, event: &str, set_at: i64) {
    self
      .scratch
      .run(
        r#"INSERT INTO "memories" ("agent", "slug", "type", "chat_id", "reach", "condition", "event", "action", "set_at", "active", "hits")
           VALUES ($1, $2, 'check', $3, 'all', '', $4, 'look again', $5, 1, 0)"#,
        vec![agent.into(), slug.into(), chat_id.into(), event.into(), set_at.into()],
      )
      .await;
  }

  async fn rule_row(&self, agent: &str, slug: &str) {
    self
      .scratch
      .run(
        r#"INSERT INTO "fragments" ("kind", "chat_id", "owner", "slug", "title", "path", "position", "active", "hits", "set_at", "items", "agent")
           VALUES ('room-rule', $1, NULL, $2, 'how many lines an answer takes', $3, 0, 1, 0, 10, '[3, 4.0, "x"]', $4)"#,
        vec![ROOM.into(), slug.into(), format!("rooms/{ROOM}/rules/{slug}.md").into(), agent.into()],
      )
      .await;
  }

  // A refused call's status, code and sentence.
  async fn refused(&self, path: &str, body: Value) -> (u16, String, String) {
    match self.caller.post::<Value>(path, &body).await {
      Err(PortFailure::Answered {
        status,
        answer: Some(answer),
        ..
      }) => (status, answer.code, answer.error),
      other => panic!("not refused: {other:?}"),
    }
  }
}

fn slugs(answer: &Value) -> Vec<&str> {
  answer["memories"]
    .as_array()
    .unwrap()
    .iter()
    .map(|row| row["slug"].as_str().unwrap())
    .collect()
}

#[test]
fn a_rooms_id_is_read_off_a_query_as_parse_int_reads_it() {
  assert_eq!(leading_whole("-100"), Some(-100));
  assert_eq!(leading_whole("  +42abc"), Some(42));
  assert_eq!(leading_whole("12.5"), Some(12));
  assert_eq!(leading_whole("room"), None);
  assert_eq!(leading_whole("-"), None);
  assert_eq!(leading_whole("9007199254740993"), None);
}

#[tokio::test]
async fn the_memories_are_listed_newest_first_whole_or_by_agent_or_room() {
  let api = served().await;

  api.memory_row(AGENT, "keep-short", None, "", 10).await;
  api.memory_row(AGENT, "check-deploy", Some(ROOM), "deployed:773", 20).await;
  api.memory_row(OTHER_AGENT, "other-check", Some(ROOM), "queue-changed", 30).await;

  let unauthorised = reqwest::Client::new()
    .get(format!("{}/memories", api.base))
    .bearer_auth("wrong")
    .send()
    .await
    .unwrap();

  assert_eq!(unauthorised.status().as_u16(), 401);
  assert!(matches!(
    caller(&api.base, "wrong").post::<Value>("/memories/active", &json!({})).await,
    Err(PortFailure::Answered { status: 401, .. })
  ));

  let (status, every) = api.get("/memories").await;

  assert_eq!(status, 200);
  assert_eq!(slugs(&every), ["other-check", "check-deploy", "keep-short"]);
  assert_eq!(every["memories"][1]["event"], json!({ "kind": "deployed", "item": 773 }));
  assert_eq!(every["memories"][1]["type"], json!("check"));
  assert_eq!(every["memories"][2]["event"], json!(null));
  assert_eq!(
    slugs(&api.get(&format!("/memories?chat={ROOM}")).await.1),
    ["other-check", "check-deploy"]
  );
  assert_eq!(
    slugs(&api.get(&format!("/memories?agent={AGENT}")).await.1),
    ["check-deploy", "keep-short"]
  );
  assert_eq!(
    slugs(&api.get(&format!("/memories?agent={AGENT}&chat={ROOM}")).await.1),
    ["check-deploy"]
  );

  let (status, bad) = api.get("/memories?chat=room").await;

  assert_eq!((status, bad["code"].as_str()), (400, Some("bad-request")));
  assert_eq!(bad["error"], json!("chat takes a whole number, not room"));
}

#[tokio::test]
async fn a_memory_is_taken_down_with_its_reason_and_put_back_and_a_switch_against_its_state_is_a_conflict()
{
  let api = served().await;

  api.memory_row(OTHER_AGENT, "keep-short", None, "", 10).await;

  let down = api
    .post(
      "/memories/active",
      json!({ "slug": "keep-short", "active": false, "why": " work done " }),
    )
    .await;

  assert_eq!(down["ok"], json!(true));
  assert_eq!(down["memory"]["active"], json!(false));
  assert_eq!(down["memory"]["downReason"], json!("work done"));
  assert!(down["memory"]["downAt"].as_i64().is_some());

  let again = api
    .refused("/memories/active", json!({ "slug": "keep-short", "active": false, "why": "x" }))
    .await;

  assert_eq!(again.0, 409);
  assert_eq!(again.1, "conflict");
  assert_eq!(again.2, "memory keep-short: already-down");

  let up = api.post("/memories/active", json!({ "slug": "keep-short", "active": true })).await;

  assert_eq!(up["memory"]["active"], json!(true));
  assert_eq!(up["memory"]["downReason"], json!(null));
  assert_eq!(up["memory"]["downAt"], json!(null));
  assert_eq!(
    api.refused("/memories/active", json!({ "slug": "keep-short", "active": true })).await.2,
    "memory keep-short: already-up"
  );

  // A row taken down carries a reason; a body without a slug or a switch is no request; a slug naming no row is
  // not found.
  let reasonless = api.refused("/memories/active", json!({ "slug": "keep-short", "active": false })).await;

  assert_eq!((reasonless.0, reasonless.1.as_str()), (400, "bad-request"));
  assert!(reasonless.2.contains("why (string) is required"), "{}", reasonless.2);
  assert_eq!(api.refused("/memories/active", json!({ "slug": " ", "active": true })).await.0, 400);
  assert_eq!(api.refused("/memories/active", json!({ "slug": "keep-short" })).await.0, 400);

  let missing = api.refused("/memories/active", json!({ "slug": "nobody", "active": true })).await;

  assert_eq!((missing.0, missing.1.as_str()), (404, "not-found"));
  assert_eq!(api.scratch.count(r#"SELECT COUNT(*) FROM "memories""#, vec![]).await, 1);
}

#[tokio::test]
async fn the_conflict_carries_its_refusal_code() {
  let api = served().await;

  api.memory_row(AGENT, "keep-short", None, "", 10).await;
  api
    .post("/memories/active", json!({ "slug": "keep-short", "active": false, "why": "done" }))
    .await;

  let answer = reqwest::Client::new()
    .post(format!("{}/memories/active", api.base))
    .bearer_auth(support::served::TOKEN)
    .header("content-type", "application/json")
    .body(json!({ "slug": "keep-short", "active": false, "why": "x" }).to_string())
    .send()
    .await
    .unwrap();

  assert_eq!(answer.status().as_u16(), 409);
  assert_eq!(
    serde_json::from_slice::<Value>(&answer.bytes().await.unwrap()).unwrap(),
    json!({ "code": "conflict", "error": "memory keep-short: already-down", "refusal": "already-down" })
  );
}

#[tokio::test]
async fn a_fragment_is_switched_by_hand_on_the_chat_agents_row_unless_another_agent_is_named() {
  let api = served().await;

  api.rule_row(AGENT, "answer-lines").await;
  api.rule_row(OTHER_AGENT, "answer-lines").await;

  let off = api
    .post(
      "/fragments/active",
      json!({ "kind": "room-rule", "chatId": ROOM, "owner": null, "slug": "answer-lines", "active": false }),
    )
    .await;

  assert_eq!(off["ok"], json!(true));
  assert_eq!(off["fragment"]["agent"], json!(AGENT));
  assert_eq!(off["fragment"]["active"], json!(false));
  assert_eq!(off["fragment"]["hand"], json!(false));
  assert!(off["fragment"]["handAt"].as_i64().is_some());
  assert_eq!(off["fragment"]["items"], json!([3, 4]));
  assert_eq!(
    api
      .scratch
      .count(
        r#"SELECT COUNT(*) FROM "fragments" WHERE "active" = 1 AND "agent" = $1"#,
        vec![OTHER_AGENT.into()]
      )
      .await,
    1
  );

  let other = api
    .post(
      "/fragments/active",
      json!({ "kind": "room-rule", "chatId": ROOM, "slug": "answer-lines", "active": false, "agent": OTHER_AGENT }),
    )
    .await;

  assert_eq!(other["fragment"]["agent"], json!(OTHER_AGENT));

  let on = api
    .post(
      "/fragments/active",
      json!({ "kind": "room-rule", "chatId": ROOM, "slug": "answer-lines", "active": true }),
    )
    .await;

  assert_eq!(
    (on["fragment"]["active"].clone(), on["fragment"]["hand"].clone()),
    (json!(true), json!(true))
  );
}

#[tokio::test]
async fn a_fragment_switch_naming_no_row_or_a_kind_there_is_none_of_is_refused() {
  let api = served().await;

  api.rule_row(AGENT, "answer-lines").await;

  let elsewhere = api
    .refused(
      "/fragments/active",
      json!({ "kind": "room-rule", "chatId": -999, "slug": "answer-lines", "active": false }),
    )
    .await;

  assert_eq!((elsewhere.0, elsewhere.1.as_str()), (404, "not-found"));
  assert_eq!(elsewhere.2, "No fragment is named room-rule/answer-lines.");

  for body in [
    json!({ "kind": "rulebook", "chatId": ROOM, "slug": "answer-lines", "active": false }),
    json!({ "kind": "room-rule", "chatId": ROOM, "slug": "answer-lines" }),
    json!({ "kind": "room-rule", "chatId": ROOM, "slug": "", "active": false }),
    json!({ "kind": "room-rule", "chatId": "room", "slug": "answer-lines", "active": false }),
    json!({ "kind": "room-rule", "chatId": ROOM, "owner": 1.5, "slug": "answer-lines", "active": false }),
    json!({ "kind": "room-rule", "chatId": ROOM, "slug": "answer-lines", "active": false, "agent": 7 }),
  ] {
    assert_eq!(api.refused("/fragments/active", body.clone()).await.0, 400, "{body}");
  }

  // A kind an owner handed in is switched like the brain's own.
  api
    .post(
      "/libraries/reconcile",
      json!({ "kind": "lore", "label": "World lore", "entries": [{
        "slug": "ooo", "title": "the land of Ooo", "path": "docs/lore/ooo.md", "position": 0, "scope": null,
        "items": [], "writtenOn": null, "confirmedOn": null, "bodyHash": null
      }] }),
    )
    .await;

  let lore = api
    .post("/fragments/active", json!({ "kind": "lore", "slug": "ooo", "active": false }))
    .await;

  assert_eq!(lore["fragment"]["hand"], json!(false));
  assert_eq!(
    api
      .scratch
      .count(r#"SELECT COUNT(*) FROM "fragments" WHERE "active" = 1"#, vec![])
      .await,
    1
  );
}
