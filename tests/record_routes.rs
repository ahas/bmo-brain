//! The memory tools' routes served on a local port and called as a tool calls them (docs/memories.md, 「The memory
//! routes」; docs/boundary-api.md, 「Memory rows」, 「Notes」 and 「Watches and reminders」): a memory row filed,
//! moved on, checked off, taken down, put back, raised and listed, a note kept, and a watch set, whose run the
//! agency in memory stands in for, so nothing is left running on this machine.

mod support;

use bmo_brain::ports::receiver::{RoadKind, Roads, RoomTitle};
use bmo_brain::schedules::apart::{Launch, LaunchOutcome};
use chrono::{Duration, FixedOffset, Utc};
use serde_json::{Map, Value, json};

use support::served::{Served, served};
use support::{AGENT, OWNER, ROOM};

const OTHER_ROOM: i64 = -100_456;
const SOMEONE_ELSE: i64 = 7788;

// What one call came to: whether it was carried out, the lines it wrote, the refusal's code and what it said.
struct Said {
  ok: bool,
  out: String,
  code: String,
  err: String,
}

// The turn a tool runs in: this agent's, in the room named.
fn turn(chat_id: i64) -> Value {
  json!({ "agent": AGENT, "chatId": chat_id, "messageId": 1 })
}

// A body carrying its fields and the turn in `ROOM`, unless it names a turn of its own.
fn body(fields: Value) -> Value {
  let mut body: Map<String, Value> = fields.as_object().cloned().unwrap_or_default();

  body.entry("turn").or_insert_with(|| turn(ROOM));

  Value::Object(body)
}

impl Served {
  async fn call(&self, path: &str, fields: Value) -> Said {
    let answer = self.post(path, body(fields)).await;
    let lines = answer["lines"].as_array().cloned().unwrap_or_default();

    Said {
      ok: answer["answered"] == json!(true),
      out: lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"),
      code: answer["refusal"]["code"].as_str().unwrap_or_default().to_owned(),
      err: answer["refusal"]["message"].as_str().unwrap_or_default().to_owned(),
    }
  }

  async fn errand(&self, chat: i64, action: &str, more: Value) -> i64 {
    let mut fields = json!({ "chat": chat, "type": "promise", "action": action });

    fields.as_object_mut().unwrap().extend(more.as_object().cloned().unwrap_or_default());

    let said = self.call("/memory/add", fields).await;

    assert!(said.ok, "{}", said.err);

    written_id(&said.out)
  }

  async fn rule(&self, more: Value) -> Said {
    let mut fields = json!({
      "chat": ROOM,
      "type": "situation",
      "slug": "deploy-tells-room",
      "condition": "when a deployment ends",
      "action": "say in one line what went up",
    });

    fields.as_object_mut().unwrap().extend(more.as_object().cloned().unwrap_or_default());

    self.call("/memory/add", fields).await
  }

  async fn count(&self, sql: &str) -> i64 {
    self.scratch.count(sql, vec![]).await
  }

  async fn text(&self, sql: &str) -> Option<String> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    self
      .scratch
      .db()
      .query_one(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
      .await
      .unwrap()
      .and_then(|row| row.try_get_by_index::<Option<String>>(0).unwrap())
  }

  // A receiver that knows a room by its title and a person by their name, which a global row is checked against.
  fn peopled(&self) {
    *self.receiver.titles.lock().unwrap() = vec![RoomTitle {
      chat_id: OTHER_ROOM,
      title: Some("Lab Room".to_owned()),
      senders: vec![SOMEONE_ELSE],
    }];
    self.receiver.names.lock().unwrap().insert(SOMEONE_ELSE, "Monkey".to_owned());
  }

  // The rows the matcher is offered in `ROOM`, read as the gate reads them.
  async fn candidates(&self) -> Vec<String> {
    self.post("/memory/conditions", json!({ "agent": AGENT, "chatId": ROOM })).await["rows"]
      .as_array()
      .unwrap()
      .iter()
      .map(|row| row["slug"].as_str().unwrap().to_owned())
      .collect()
  }

  fn room_kind(&self, kind: RoadKind) {
    self.receiver.roads.lock().unwrap().insert(
      ROOM,
      Roads {
        kind,
        one_to_one: kind == RoadKind::Owner,
      },
    );
  }

  // The watches whose runs were launched, in order.
  fn launched(&self) -> Vec<i64> {
    self
      .starter
      .calls
      .of("start")
      .iter()
      .filter_map(|launch| {
        let words: Vec<&str> = launch["command"].as_array()?.iter().filter_map(Value::as_str).collect();
        let at = words.iter().position(|word| *word == "watch-runner")?;

        words.get(at + 1)?.parse().ok()
      })
      .collect()
  }
}

fn written_id(out: &str) -> i64 {
  let at = out.find("memory ").expect("a row written") + "memory ".len();

  out[at..].split(' ').next().unwrap().parse().unwrap()
}

#[tokio::test]
async fn an_errand_is_written_listed_checked_off_and_kept_in_the_full_list() {
  let api = served().await;
  let id = api
    .errand(ROOM, "tell the owner how the build went", json!({ "for": 42, "message": 7 }))
    .await;

  assert_eq!(
    api.count(&format!(r#"SELECT COUNT(*) FROM "memories" WHERE "id" = {id} AND "chat_id" = {ROOM} AND "type" = 'promise' AND "condition" = '' AND "for_user" = 42 AND "message_id" = 7 AND "done_at" IS NULL AND "agent" = '{AGENT}'"#)).await,
    1
  );

  let open = api.call("/memory/list", json!({})).await;

  assert!(open.ok);
  assert!(open.out.contains(&format!("[on ] #{id} promise")), "{}", open.out);
  assert!(open.out.contains("for user 42"));
  assert!(open.out.contains("then: tell the owner how the build went"));
  // A row with no condition shows none, because it waits for nothing.
  assert!(!open.out.contains("when:"));

  let done = api.call("/memory/done", json!({ "memory": id.to_string(), "text": "told" })).await;

  assert!(done.ok, "{}", done.err);
  assert!(done.out.contains(&format!("memory {id} checked off")));
  assert!(api.call("/memory/list", json!({})).await.out.contains("has no open memories"));

  let every = api.call("/memory/list", json!({ "all": true })).await;

  assert!(every.out.contains(&format!("#{id} promise")));
  assert!(every.out.contains("KST: told"));
  assert_eq!(
    api
      .text(&format!(r#"SELECT "done_text" FROM "memories" WHERE "id" = {id}"#))
      .await
      .as_deref(),
    Some("told")
  );
}

#[tokio::test]
async fn a_row_written_with_no_slug_is_given_one_and_either_handle_names_it() {
  let api = served().await;
  let id = api.errand(ROOM, "written with no name", json!({})).await;
  let slug = api.text(&format!(r#"SELECT "slug" FROM "memories" WHERE "id" = {id}"#)).await.unwrap();

  assert_eq!(slug, format!("memory-{id}"));
  assert!(api.call("/memory/done", json!({ "memory": slug })).await.ok);
  assert_eq!(
    api
      .count(&format!(
        r#"SELECT COUNT(*) FROM "memories" WHERE "id" = {id} AND "done_at" IS NOT NULL"#
      ))
      .await,
    1
  );
}

#[tokio::test]
async fn an_errand_may_wait_on_a_condition_and_then_it_is_the_matchers() {
  let api = served().await;

  api.room_kind(RoadKind::Owner);

  let written = api
    .call(
      "/memory/add",
      json!({
        "type": "promise",
        "slug": "when-back",
        "condition": "when the owner is back",
        "action": "tell him the result",
      }),
    )
    .await;

  assert!(written.ok, "{}", written.err);
  assert_eq!(
    api.count(r#"SELECT COUNT(*) FROM "memories" WHERE "slug" = 'when-back' AND "type" = 'promise' AND "condition" = 'when the owner is back'"#).await,
    1
  );
  assert_eq!(api.candidates().await, ["when-back"]);
}

#[tokio::test]
async fn refuses_to_check_off_a_missing_row_another_rooms_row_or_one_already_checked() {
  let api = served().await;
  let id = api.errand(ROOM, "this room's promise", json!({})).await;
  let missing = api.call("/memory/done", json!({ "memory": (id + 1).to_string() })).await;

  assert!(!missing.ok);
  assert!(missing.err.contains(&format!("there is no memory '{}'", id + 1)), "{}", missing.err);

  let elsewhere = api.call("/memory/done", json!({ "chat": OTHER_ROOM, "memory": id.to_string() })).await;

  assert!(!elsewhere.ok);
  assert!(elsewhere.err.contains("is not this room's"));
  assert!(!elsewhere.err.contains("this room's promise"));
  assert!(api.call("/memory/done", json!({ "memory": id.to_string() })).await.ok);

  let twice = api.call("/memory/done", json!({ "memory": id.to_string(), "text": "again" })).await;

  assert!(!twice.ok);
  assert!(twice.err.contains("was already checked off"));
  assert_eq!(
    api.text(&format!(r#"SELECT "done_text" FROM "memories" WHERE "id" = {id}"#)).await,
    None
  );
}

#[tokio::test]
async fn a_situation_row_cannot_be_checked_off_and_is_pointed_at_retire_instead() {
  let api = served().await;

  assert!(api.rule(json!({})).await.ok);

  let checked = api
    .call("/memory/done", json!({ "memory": "deploy-tells-room", "text": "all done" }))
    .await;

  assert!(!checked.ok);
  assert!(checked.err.contains("is a situation row, which is never finished"));
  assert!(checked.err.contains("--retire deploy-tells-room"));
  assert_eq!(
    api
      .count(r#"SELECT COUNT(*) FROM "memories" WHERE "done_at" IS NULL AND "active" = 1"#)
      .await,
    1
  );
}

#[tokio::test]
async fn lists_only_the_room_it_is_asked_about_and_narrows_by_type() {
  let api = served().await;

  api.errand(ROOM, "this one here", json!({})).await;
  api.errand(OTHER_ROOM, "that one there", json!({})).await;
  assert!(api.rule(json!({})).await.ok);

  let here = api.call("/memory/list", json!({ "all": true })).await;

  assert!(here.out.contains("this one here"));
  assert!(!here.out.contains("that one there"));

  let situations = api.call("/memory/list", json!({ "type": "situation" })).await;

  assert!(situations.out.contains("deploy-tells-room"));
  assert!(!situations.out.contains("this one here"));

  let promises = api.call("/memory/list", json!({ "type": "promise" })).await;

  assert!(promises.out.contains("this one here"));
  assert!(!promises.out.contains("deploy-tells-room"));

  // Another agent's rows are its own, even in the same room.
  let other = api
    .call("/memory/list", json!({ "all": true, "turn": { "agent": "agent-b", "chatId": ROOM } }))
    .await;

  assert!(other.out.contains("has no memories"), "{}", other.out);
}

#[tokio::test]
async fn a_rule_is_written_says_where_it_landed_and_is_listed() {
  let api = served().await;
  let written = api.rule(json!({ "by": 42, "message": 9 })).await;

  assert!(written.ok, "{}", written.err);

  let id = written_id(&written.out);

  assert!(written.out.contains(&format!("for chat {ROOM}")));
  assert_eq!(
    api.count(&format!(r#"SELECT COUNT(*) FROM "memories" WHERE "id" = {id} AND "slug" = 'deploy-tells-room' AND "type" = 'situation' AND "chat_id" = {ROOM} AND "active" = 1 AND "set_by" = 42 AND "message_id" = 9 AND "hits" = 0"#)).await,
    1
  );

  let listed = api.call("/memory/list", json!({})).await;

  assert!(listed.out.contains("[on ] #"));
  assert!(listed.out.contains("deploy-tells-room"));
  assert!(listed.out.contains("when: when a deployment ends"));
  assert!(listed.out.contains("then: say in one line what went up"));
}

#[tokio::test]
async fn a_situation_row_with_no_condition_is_refused() {
  let api = served().await;
  let written = api
    .call(
      "/memory/add",
      json!({ "type": "situation", "slug": "no-when", "action": "do something" }),
    )
    .await;

  assert!(!written.ok);
  assert!(written.err.contains("a situation row is a pair"));
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 0);
}

#[tokio::test]
async fn a_row_filed_in_place_of_another_takes_that_one_down_in_the_same_call() {
  let api = served().await;

  api.rule(json!({})).await;

  let written = api
    .call(
      "/memory/add",
      json!({
        "type": "situation",
        "slug": "deploy-tells-room-2",
        "condition": "when a deployment ends",
        "action": "say what went up and how long it took",
        "replaces": "deploy-tells-room",
      }),
    )
    .await;

  assert!(written.ok, "{}", written.err);
  assert!(written.out.contains("replacing 'deploy-tells-room'"));
  assert_eq!(
    api
      .text(r#"SELECT "down_reason" FROM "memories" WHERE "slug" = 'deploy-tells-room' AND "active" = 0"#)
      .await
      .as_deref(),
    Some("replaced by deploy-tells-room-2")
  );
  assert_eq!(
    api.text(r#"SELECT "slug" FROM "memories" WHERE "active" = 1"#).await.as_deref(),
    Some("deploy-tells-room-2")
  );
}

#[tokio::test]
async fn a_global_row_that_names_a_room_a_person_or_an_id_is_refused_and_the_phrase_is_named() {
  let api = served().await;

  api.peopled();

  let refused = |slug: &'static str, condition: &'static str, action: String| {
    let api = &api;

    async move {
      api
        .call(
          "/memory/add",
          json!({ "global": true, "type": "situation", "condition": condition, "slug": slug, "action": action }),
        )
        .await
    }
  };
  let named = refused("names-room", "when a deployment ends", "tell the lab room too".into()).await;

  assert!(!named.ok);
  assert!(named.err.contains("room title Lab Room"), "{}", named.err);

  let person = refused("names-person", "when a deployment ends", "ask Monkey first".into()).await;

  assert!(person.err.contains("person Monkey"), "{}", person.err);

  let number = refused("names-id", "when a deployment ends", format!("ask {SOMEONE_ELSE} first")).await;

  assert!(number.err.contains(&format!("user id {SOMEONE_ELSE}")), "{}", number.err);

  let form = refused("names-id-form", "when a deployment ends", "ask id 999111 first".into()).await;

  assert!(form.err.contains("'id 999111'"), "{}", form.err);

  // The condition half is read the same way as the action.
  let in_condition =
    refused("condition-names", "when asked in the lab room", "answer in one line".into()).await;

  assert!(in_condition.err.contains("room title Lab Room"));
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 0);
}

#[tokio::test]
async fn a_global_row_may_not_be_for_one_person() {
  let api = served().await;
  let written = api
    .call(
      "/memory/add",
      json!({ "global": true, "type": "promise", "slug": "for-somebody", "action": "answer in one line", "for": 7788 }),
    )
    .await;

  assert!(!written.ok);
  assert!(written.err.contains("give it a --chat instead"));
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 0);
}

#[tokio::test]
async fn a_global_row_that_names_nobody_stands_reaching_the_owners_room_until_told_otherwise() {
  let api = served().await;

  api.peopled();

  let written = api
    .call(
      "/memory/add",
      json!({
        "global": true,
        "type": "situation",
        "slug": "short-answers",
        "condition": "when asked to be brief",
        "action": "answer within three lines",
      }),
    )
    .await;

  assert!(written.ok, "{}", written.err);
  assert!(written.out.contains("globally, reaching owner rooms"));
  assert_eq!(
    api
      .text(r#"SELECT "reach" FROM "memories" WHERE "slug" = 'short-answers' AND "chat_id" IS NULL"#)
      .await
      .as_deref(),
    Some("owner")
  );

  api.room_kind(RoadKind::Owner);
  assert_eq!(api.candidates().await, ["short-answers"]);
  api.room_kind(RoadKind::Group);
  assert!(api.candidates().await.is_empty());

  let wider = api
    .call(
      "/memory/add",
      json!({
        "global": true,
        "reach": "all",
        "type": "situation",
        "slug": "everywhere",
        "condition": "whenever",
        "action": "answer politely",
      }),
    )
    .await;

  assert!(wider.ok, "{}", wider.err);
  assert_eq!(
    api
      .text(r#"SELECT "reach" FROM "memories" WHERE "slug" = 'everywhere'"#)
      .await
      .as_deref(),
    Some("all")
  );
}

#[tokio::test]
async fn retiring_keeps_the_row_and_the_full_list_still_shows_it_with_its_reason() {
  let api = served().await;

  api.rule(json!({})).await;

  let retired = api
    .call(
      "/memory/retire",
      json!({ "memory": "deploy-tells-room", "why": "the owner said it is no longer needed" }),
    )
    .await;

  assert!(retired.ok, "{}", retired.err);
  assert!(retired.out.contains("retired"));
  assert!(api.call("/memory/list", json!({})).await.out.contains("has no open memories"));

  let every = api.call("/memory/list", json!({ "all": true })).await;

  assert!(every.out.contains("[off]"));
  assert!(every.out.contains("the owner said it is no longer needed"));
  assert!(every.out.contains("when: when a deployment ends"));
}

#[tokio::test]
async fn a_row_put_back_loses_what_it_was_taken_down_for_and_is_refused_twice_over() {
  let api = served().await;

  api.rule(json!({})).await;
  api
    .call("/memory/retire", json!({ "memory": "deploy-tells-room", "why": "out for a while" }))
    .await;

  let back = api.call("/memory/revive", json!({ "memory": "deploy-tells-room" })).await;

  assert!(back.ok, "{}", back.err);
  assert!(back.out.contains("put back"));
  assert_eq!(
    api.count(r#"SELECT COUNT(*) FROM "memories" WHERE "active" = 1 AND "down_reason" IS NULL AND "down_at" IS NULL"#).await,
    1
  );

  let again = api.call("/memory/revive", json!({ "memory": "deploy-tells-room" })).await;

  assert!(!again.ok);
  assert!(again.err.contains("is already active"));
}

#[tokio::test]
async fn a_row_is_named_by_its_number_as_readily_as_by_its_slug() {
  let api = served().await;
  let id = written_id(&api.rule(json!({})).await.out);
  let retired = api
    .call("/memory/retire", json!({ "memory": id.to_string(), "why": "by number" }))
    .await;

  assert!(retired.ok, "{}", retired.err);
  assert!(retired.out.contains("'deploy-tells-room' retired"));
  assert_eq!(
    api
      .text(&format!(r#"SELECT "down_reason" FROM "memories" WHERE "id" = {id} AND "active" = 0"#))
      .await
      .as_deref(),
    Some("by number")
  );
}

#[tokio::test]
async fn a_candidate_is_raised_without_the_row_leaving_the_list() {
  let api = served().await;

  api.rule(json!({})).await;

  let raised = api
    .call(
      "/memory/candidate",
      json!({ "memory": "deploy-tells-room", "why": "the deployment already says so" }),
    )
    .await;

  assert!(raised.ok, "{}", raised.err);
  assert!(raised.out.contains("stays active until a person takes it down"));

  let listed = api.call("/memory/list", json!({})).await;

  assert!(listed.out.contains("[on ]"));
  assert!(listed.out.contains("candidate · the deployment already says so"));
}

#[tokio::test]
async fn no_route_takes_a_row_out_of_the_table() {
  let api = served().await;

  api.rule(json!({})).await;
  api
    .call(
      "/memory/add",
      json!({ "type": "situation", "slug": "second", "condition": "whenever", "action": "answer briefly" }),
    )
    .await;

  let id = api.errand(ROOM, "a thing to finish", json!({})).await;

  api
    .call("/memory/retire", json!({ "memory": "deploy-tells-room", "why": "over" }))
    .await;
  api
    .call("/memory/candidate", json!({ "memory": "second", "why": "already a rule" }))
    .await;
  api.call("/memory/done", json!({ "memory": id.to_string(), "text": "done" })).await;

  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 3);
}

#[tokio::test]
async fn another_scopes_row_is_not_this_callers_to_touch() {
  let api = served().await;

  api.rule(json!({})).await;

  let from_elsewhere = api
    .call(
      "/memory/retire",
      json!({ "chat": OTHER_ROOM, "memory": "deploy-tells-room", "why": "from another room" }),
    )
    .await;

  assert!(!from_elsewhere.ok);
  assert!(from_elsewhere.err.contains("belongs to another scope"));

  let as_global = api
    .call(
      "/memory/candidate",
      json!({ "global": true, "memory": "deploy-tells-room", "why": "posing as global" }),
    )
    .await;

  assert!(!as_global.ok);

  // Another agent does not reach this agent's row either.
  let other = api
    .call(
      "/memory/retire",
      json!({ "memory": "deploy-tells-room", "why": "not mine", "turn": { "agent": "agent-b", "chatId": ROOM } }),
    )
    .await;

  assert!(!other.ok);
  assert_eq!(
    api
      .count(r#"SELECT COUNT(*) FROM "memories" WHERE "active" = 1 AND "candidate_why" IS NULL"#)
      .await,
    1
  );
}

#[tokio::test]
async fn on_writes_the_event_the_row_waits_on_and_the_list_shows_it() {
  let api = served().await;
  let written = api
    .call(
      "/memory/add",
      json!({ "chat": OWNER, "type": "promise", "action": "take up #767 again after #773", "on": "deployed:#773" }),
    )
    .await;

  assert!(written.ok, "{}", written.err);
  assert_eq!(
    api.count(r#"SELECT COUNT(*) FROM "memories" WHERE "event" = 'deployed:773' AND "event_met_at" IS NULL AND "condition" = ''"#).await,
    1
  );
  assert!(
    api
      .call("/memory/list", json!({ "chat": OWNER }))
      .await
      .out
      .contains("on: deployed:773")
  );
}

#[tokio::test]
async fn an_errand_waiting_on_nothing_that_wakes_it_and_an_event_out_of_the_vocabulary_are_refused() {
  let api = served().await;
  let waiting = "once the results are in, run the training again";

  // The receiver says the text waits on something happening first.
  api.receiver.waiting.lock().unwrap().insert(waiting.to_owned());

  let waits = api
    .call("/memory/add", json!({ "chat": OWNER, "type": "promise", "action": waiting }))
    .await;
  let unknown = api
    .call(
      "/memory/add",
      json!({ "chat": OWNER, "type": "promise", "action": "report", "on": "restarted" }),
    )
    .await;
  let both = api
    .call(
      "/memory/add",
      json!({
        "chat": OWNER,
        "type": "situation",
        "condition": "when a deployment ends",
        "on": "deployed",
        "action": "say so",
      }),
    )
    .await;

  assert!(!waits.ok);
  assert!(waits.err.contains("nothing would wake you"), "{}", waits.err);
  assert!(!unknown.ok);
  assert!(unknown.err.contains("restarted"));
  assert!(!both.ok);
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 0);
}

#[tokio::test]
async fn until_sets_the_row_and_a_watch_serving_it_in_one_call() {
  let api = served().await;
  let written = api
    .call(
      "/memory/add",
      json!({
        "chat": OWNER,
        "type": "promise",
        "action": "upload once C is done",
        "until": "! systemctl --user is-active --quiet voice-c",
        "name": "C training done",
      }),
    )
    .await;

  assert!(written.ok, "{}", written.err);
  assert!(written.out.contains("serving memory #"));

  let id = written_id(&written.out);
  let watch = api
    .count(&format!(r#"SELECT "id" FROM "watches" WHERE "by_chat" = {OWNER} AND "memory_id" = {id} AND "name" = 'C training done' AND "note" = 'upload once C is done' AND "state" = 'active' AND "agent" = '{AGENT}'"#))
    .await;

  assert_eq!(api.launched(), [watch]);
}

#[tokio::test]
async fn a_watch_that_cannot_be_set_takes_the_row_down_again() {
  let api = served().await;

  for n in 0..10 {
    api
      .scratch
      .run(
        r#"INSERT INTO "watches" ("agent", "chat_id", "by_chat", "kind", "condition", "note", "dedupe_key", "interval_seconds", "deadline_at", "state", "created_at")
           VALUES ($1, $2, $2, 'condition', $3, 'waiting', $4, 60, 4000000000, 'active', 1)"#,
        vec![AGENT.into(), OWNER.into(), format!("test -e /tmp/none-{n}").into(), format!("k{n}").into()],
      )
      .await;
  }

  let written = api
    .call(
      "/memory/add",
      json!({
        "chat": OWNER,
        "type": "promise",
        "action": "say so once the build ends",
        "until": "test -e /tmp/build-done",
        "turn": turn(OWNER),
      }),
    )
    .await;

  assert!(!written.ok);
  assert!(written.err.contains("taken down again"), "{}", written.err);
  assert!(written.out.contains("written for chat 42"), "{}", written.out);
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories" WHERE "active" = 0"#).await, 1);
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 1);
}

#[tokio::test]
async fn a_state_is_written_moved_on_listed_when_it_moved_and_released() {
  let api = served().await;
  let written = api
    .call(
      "/memory/add",
      json!({ "type": "state", "slug": "improv", "action": "improv game, Monkey's turn" }),
    )
    .await;

  assert!(written.ok, "{}", written.err);

  let moved = api
    .call(
      "/memory/update",
      json!({ "memory": "improv", "action": "improv game, the owner's turn" }),
    )
    .await;

  assert!(moved.ok, "{}", moved.err);
  assert!(moved.out.contains(&format!("moved on for chat {ROOM}")));

  let open = api.call("/memory/list", json!({ "type": "state" })).await;

  assert!(open.out.contains(&format!("state · improv · chat {ROOM} · set ")), "{}", open.out);
  assert!(open.out.contains(" · moved "));
  assert!(open.out.contains("then: improv game, the owner's turn"));

  let released = api.call("/memory/done", json!({ "memory": "improv", "text": "asked to stop" })).await;

  assert!(released.out.contains(&format!("released for chat {ROOM}")));

  // A move acts on a room's own state; outside a room's turn there is none to name.
  let roomless = api
    .call(
      "/memory/update",
      json!({ "global": true, "memory": "improv", "action": "x", "turn": { "agent": AGENT } }),
    )
    .await;

  assert_eq!(roomless.code, "no-room");
}

#[tokio::test]
async fn every_bad_call_is_refused_and_the_table_is_left_empty() {
  let api = served().await;
  let long_action = "a".repeat(2001);
  let long_condition = "a".repeat(501);
  let roomless = json!({ "agent": AGENT });

  for (path, fields, says) in [
    (
      "/memory/add",
      json!({ "type": "promise", "action": "x", "turn": roomless }),
      "has no room to act in",
    ),
    ("/memory/list", json!({ "chat": ROOM, "global": true }), "two different scopes"),
    ("/memory/add", json!({ "type": "todo", "action": "x" }), "--type must be one of"),
    ("/memory/add", json!({ "type": "promise", "action": "   " }), "is what attaches"),
    ("/memory/add", json!({ "type": "promise", "action": long_action }), "at most 2000"),
    (
      "/memory/add",
      json!({ "type": "situation", "condition": long_condition, "action": "x" }),
      "at most 500",
    ),
    (
      "/memory/add",
      json!({ "type": "promise", "action": "x", "slug": "Big Name" }),
      "lower-case letters, digits and hyphens",
    ),
    (
      "/memory/add",
      json!({ "reach": "all", "type": "situation", "condition": "a", "action": "b" }),
      "--reach says which kinds of room",
    ),
    (
      "/memory/add",
      json!({ "type": "situation", "condition": "a", "action": "b", "replaces": "nobody" }),
      "there is no memory 'nobody'",
    ),
    ("/memory/retire", json!({ "memory": "deploy-tells-room" }), "--why"),
    ("/memory/retire", json!({ "memory": "" }), "takes the row's id or its slug"),
    ("/memory/list", json!({ "type": "todo" }), "--type must be one of"),
    (
      "/memory/add",
      json!({ "type": "promise", "action": "x", "turn": { "chatId": ROOM } }),
      "no agent whose memory it would be",
    ),
  ] {
    let said = api.call(path, fields.clone()).await;

    assert!(!said.ok, "{path} {fields}");
    assert!(said.err.contains(says), "{path} {fields}: {}", said.err);
  }

  // A body of another shape is no refusal but the wire's 400.
  let shape = api
    .caller
    .post::<Value>("/memory/add", &body(json!({ "type": "promise", "action": "x", "for": "someone" })))
    .await;

  assert!(
    matches!(shape, Err(bmo_brain::ports::http::PortFailure::Answered { status: 400, .. })),
    "{shape:?}"
  );
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 0);
}

#[tokio::test]
async fn a_slug_already_standing_names_one_row_everywhere() {
  let api = served().await;

  assert!(api.rule(json!({})).await.ok);

  let taken = api
    .call(
      "/memory/add",
      json!({ "chat": OTHER_ROOM, "type": "promise", "slug": "deploy-tells-room", "action": "b" }),
    )
    .await;

  assert!(!taken.ok);
  assert!(taken.err.contains("a slug names one row everywhere"));
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "memories""#).await, 1);
}

#[tokio::test]
async fn a_note_is_kept_for_its_room_or_every_room_and_one_with_nothing_to_keep_is_refused() {
  let api = served().await;
  let room = api
    .call("/notes/add", json!({ "text": "  minutes are tidied every Friday ", "author": OWNER }))
    .await;
  let every = api
    .call(
      "/notes/add",
      json!({ "text": "answer in the room's language", "author": OWNER, "global": true }),
    )
    .await;

  assert!(room.ok, "{}", room.err);
  assert!(
    room.out.starts_with("note ") && room.out.ends_with(&format!(" kept for chat {ROOM}")),
    "{}",
    room.out
  );
  assert!(every.out.ends_with(&format!(" kept for every room, from chat {ROOM}")), "{}", every.out);
  assert_eq!(
    api.count(&format!(r#"SELECT COUNT(*) FROM "notes" WHERE "text" = 'minutes are tidied every Friday' AND "shared" = 0 AND "author" = 42 AND "agent" = '{AGENT}'"#)).await,
    1
  );

  let empty = api.call("/notes/add", json!({ "text": "   " })).await;

  assert!(!empty.ok);
  assert_eq!(empty.code, "usage");
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "notes""#).await, 2);

  // The digest's own notes are listed for the agent, and narrowed to a room.
  let listed = api.post("/notes/list", json!({ "agent": AGENT, "chatId": ROOM })).await;
  let texts: Vec<&str> = listed["notes"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|note| note["text"].as_str())
    .collect();

  assert_eq!(texts, ["minutes are tidied every Friday", "answer in the room's language"]);
  assert_eq!(listed["notes"][1]["global"], json!(true));
  assert!(
    api.post("/notes/list", json!({ "agent": "agent-b" })).await["notes"]
      .as_array()
      .unwrap()
      .is_empty()
  );
}

// A watch's arguments over the turn in `ROOM`, the check given and the note every watch here carries.
fn watch(check: Option<&str>, more: Value) -> Value {
  let mut fields = json!({ "until": check, "note": "tell the room once the job ends" });

  fields.as_object_mut().unwrap().extend(more.as_object().cloned().unwrap_or_default());

  fields
}

// A moment a day from now, as a person writes it in Seoul.
fn tomorrow() -> String {
  let seoul = FixedOffset::east_opt(9 * 3600).unwrap();

  (Utc::now() + Duration::days(1))
    .with_timezone(&seoul)
    .format("%Y-%m-%d %H:%M")
    .to_string()
}

#[tokio::test]
async fn a_watch_is_refused_before_anything_exists() {
  let api = served().await;

  for fields in [
    watch(Some("if then ("), json!({})),
    watch(Some("true"), json!({ "every": 5 })),
    watch(Some("true"), json!({ "within": 5000 })),
    watch(Some("true"), json!({ "at": tomorrow() })),
    watch(None, json!({ "at": "2001-01-01 09:00" })),
    watch(None, json!({ "at": "2099-01-01 09:00" })),
  ] {
    let said = api.call("/watches/set", fields.clone()).await;

    assert!(!said.ok, "{fields}");
    assert!(!said.err.is_empty(), "{fields}");
  }

  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "watches""#).await, 0);
  assert!(api.launched().is_empty());
}

#[tokio::test]
async fn a_watch_is_set_in_a_run_of_its_own_and_not_twice() {
  let api = served().await;
  let set = api
    .call(
      "/watches/set",
      watch(Some("test -e /tmp/never-there"), json!({ "every": 30, "within": 10 })),
    )
    .await;

  assert!(set.ok, "{}", set.err);

  let id = api
    .count(&format!(r#"SELECT "id" FROM "watches" WHERE "kind" = 'condition' AND "interval_seconds" = 30 AND "by_chat" = {ROOM} AND "state" = 'active' AND "agent" = '{AGENT}'"#))
    .await;

  assert!(
    set.out.starts_with(&format!(
      "watch #{id} (tell the room once the job ends): checking every 30 seconds until "
    )) && set.out.ends_with(" KST"),
    "{}",
    set.out
  );
  assert_eq!(api.launched(), [id]);
  // The watch is armed under the unit its run was launched as.
  assert_eq!(
    api.text(&format!(r#"SELECT "unit" FROM "watches" WHERE "id" = {id}"#)).await,
    api.starter.calls.of("start")[0]["unit"].as_str().map(str::to_owned)
  );

  let again = api.call("/watches/set", watch(Some("test  -e /tmp/never-there"), json!({}))).await;

  assert!(!again.ok);
  assert_eq!(again.err, format!("already watching this as #{id}"));

  // A check whose run never came up ends its row, so nothing stands that nobody watches.
  *api.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Refused,
    why: Some("the unit could not start".to_owned()),
  });

  let refused = api.call("/watches/set", watch(Some("test -e /tmp/other"), json!({}))).await;

  assert!(
    refused.err.ends_with("could not be launched, so nothing is watching it"),
    "{}",
    refused.err
  );
  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "watches" WHERE "state" = 'active'"#).await, 1);
}

#[tokio::test]
async fn a_watch_serves_a_memory_of_its_own_room_and_refuses_any_other() {
  let api = served().await;
  let item = api.errand(ROOM, "tell how the job went", json!({})).await;
  let elsewhere = api.errand(-555, "another room's promise", json!({})).await;
  let checked = api.errand(ROOM, "a finished check", json!({})).await;

  api.call("/memory/done", json!({ "memory": checked.to_string() })).await;

  for (id, says) in [
    (elsewhere, "not this room's".to_owned()),
    (checked, "already checked off".to_owned()),
    (999_999, "there is no memory 999999".to_owned()),
  ] {
    let refused = api
      .call("/watches/set", watch(Some("test -e /tmp/never-there"), json!({ "item": id })))
      .await;

    assert!(refused.err.contains(&says), "{}", refused.err);
    assert!(!refused.err.contains("another room's promise"));
  }

  assert_eq!(api.count(r#"SELECT COUNT(*) FROM "watches""#).await, 0);

  let served = api
    .call("/watches/set", watch(Some("test -e /tmp/never-there"), json!({ "item": item })))
    .await;

  assert!(served.out.ends_with(&format!("serving memory #{item}")), "{}", served.out);
  assert_eq!(api.count(r#"SELECT "memory_id" FROM "watches""#).await, item);
}

#[tokio::test]
async fn a_watch_takes_a_short_name_and_is_set_without_one_all_the_same() {
  let api = served().await;
  let named = api
    .call(
      "/watches/set",
      watch(Some("test -e /tmp/never-there"), json!({ "name": "  build   ready " })),
    )
    .await;

  assert!(named.out.contains("(build ready)"), "{}", named.out);
  assert_eq!(
    api.text(r#"SELECT "name" FROM "watches" ORDER BY "id" LIMIT 1"#).await.as_deref(),
    Some("build ready")
  );

  let plain = api.call("/watches/set", watch(Some("test -e /tmp/never-either"), json!({}))).await;

  assert!(plain.out.contains("(tell the room once the job ends)"), "{}", plain.out);
  assert_eq!(api.text(r#"SELECT "name" FROM "watches" ORDER BY "id" DESC LIMIT 1"#).await, None);
}

#[tokio::test]
async fn a_timer_is_a_row_alone_with_no_run() {
  let api = served().await;
  let at = tomorrow();
  let set = api.call("/watches/set", watch(None, json!({ "at": at }))).await;

  assert!(set.ok, "{}", set.err);
  assert!(set.out.ends_with(&format!("a timer for {at} KST")), "{}", set.out);
  assert_eq!(
    api
      .count(r#"SELECT COUNT(*) FROM "watches" WHERE "kind" = 'timer' AND "unit" IS NULL"#)
      .await,
    1
  );
  assert!(api.launched().is_empty());
}
