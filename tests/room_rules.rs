//! The owner's room rules (docs/boundary-api.md, 「Owner room rules」): a room's document under the data root, a
//! rule granted only on a message the owner sent and revoked leaving nothing behind, served on a local port and
//! called as the receiver's authority check calls it.

mod support;

use std::os::unix::fs::PermissionsExt;

use bmo_brain::ports::http::PortFailure;
use bmo_brain::room_rules::{Unchanged, change_rule, id_problem, read_rules, rule_problem, rules_path};
use serde_json::{Value, json};

use support::OWNER;
use support::receiver::message;
use support::served::{Served, served};

const ROOM: i64 = -1;
const SOMEONE: i64 = 43;

impl Served {
  // Messages of the room: the first and third the owner's, the second another person's.
  fn spoken(&self) {
    *self.receiver.messages.lock().unwrap() =
      vec![message(ROOM, 1, OWNER, 1), message(ROOM, 2, SOMEONE, 1), message(ROOM, 3, OWNER, 1)];
  }

  async fn rules(&self, chat_id: i64) -> Value {
    self.post("/room-rules/read", json!({ "chatId": chat_id })).await["rules"].clone()
  }

  async fn status_of(&self, body: Value) -> (u16, String) {
    match self.caller.post::<Value>("/room-rules/set", &body).await {
      Err(PortFailure::Answered {
        status,
        answer: Some(answer),
        ..
      }) => (status, answer.error),
      other => panic!("not refused: {other:?}"),
    }
  }
}

#[tokio::test]
async fn only_an_owners_message_grants_and_a_revoked_rule_leaves_nothing_behind() {
  let api = served().await;

  api.spoken();

  let rule = |message_id: i64, text: &str| json!({ "chatId": ROOM, "messageId": message_id, "id": "links-person", "rule": { "subject": SOMEONE, "text": text } });
  let refused = api.post("/room-rules/set", rule(2, "save links")).await;

  assert_eq!(refused["answered"], json!(false));
  assert_eq!(refused["refusal"]["code"], json!("not-owner"));
  assert!(!rules_path(api.data.path(), ROOM).exists());

  let granted = api.post("/room-rules/set", rule(1, "  analyse and save links in project X ")).await;
  let row =
    json!({ "id": "links-person", "subject": SOMEONE, "text": "analyse and save links in project X" });

  assert_eq!(granted, json!({ "rules": [row.clone()] }));
  assert_eq!(api.rules(ROOM).await, json!([row]));
  assert_eq!(api.rules(-2).await, json!([]));
  // The owner check asked the receiver about the message the change came on.
  assert_eq!(
    api.receiver.calls.of("sender_is_owner").last(),
    Some(&json!({ "chatId": ROOM, "messageId": 1 }))
  );

  let document = rules_path(api.data.path(), ROOM);

  assert_eq!(std::fs::metadata(&document).unwrap().permissions().mode() & 0o777, 0o600);

  // Setting the same id again replaces the rule; another id stands beside it, in the order set.
  api.post("/room-rules/set", rule(3, "save links only")).await;

  let other = json!({ "chatId": ROOM, "messageId": 3, "id": "notes_2", "rule": { "subject": 7.0, "text": "keep notes" } });

  assert_eq!(api.post("/room-rules/set", other).await["rules"].as_array().unwrap().len(), 2);
  assert_eq!(api.rules(ROOM).await[0]["text"], json!("save links only"));
  assert_eq!(api.rules(ROOM).await[1]["subject"], json!(7));

  // A revocation removes the rule outright; with the last one gone the document is an empty list.
  for id in ["links-person", "notes_2"] {
    api
      .post("/room-rules/set", json!({ "chatId": ROOM, "messageId": 3, "id": id, "rule": null }))
      .await;
  }

  assert_eq!(std::fs::read_to_string(&document).unwrap(), "[]\n");
  assert_eq!(api.rules(ROOM).await, json!([]));
}

#[tokio::test]
async fn an_id_subject_or_text_out_of_shape_is_refused_and_a_change_holding_the_lock_is_a_conflict() {
  let api = served().await;

  api.spoken();

  for body in [
    json!({ "chatId": ROOM, "messageId": 1, "id": "has space", "rule": null }),
    json!({ "chatId": ROOM, "messageId": 1, "id": "", "rule": null }),
    json!({ "chatId": ROOM, "messageId": 1, "id": "a", "rule": { "subject": 1.5, "text": "x" } }),
    json!({ "chatId": ROOM, "messageId": 1, "id": "a", "rule": { "subject": 9007199254740992_i64, "text": "x" } }),
    json!({ "chatId": ROOM, "messageId": 1, "id": "a", "rule": { "subject": 1, "text": "   " } }),
    json!({ "chatId": ROOM, "messageId": 1, "id": "a", "rule": { "subject": 1, "text": "x".repeat(8001) } }),
    json!({ "chatId": ROOM, "messageId": 1, "id": "a" , "rule": { "text": "x" } }),
  ] {
    assert_eq!(api.status_of(body.clone()).await.0, 400, "{body}");
  }

  assert!(!rules_path(api.data.path(), ROOM).exists());

  // A message nobody owns is checked first, whatever else the request holds.
  assert_eq!(
    api
      .post(
        "/room-rules/set",
        json!({ "chatId": ROOM, "messageId": 2, "id": "has space", "rule": null })
      )
      .await["refusal"]["code"],
    json!("not-owner")
  );

  let path = rules_path(api.data.path(), ROOM);

  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(format!("{}.lock", path.display()), "").unwrap();

  let busy = api
    .caller
    .post::<Value>("/room-rules/set", &json!({ "chatId": ROOM, "messageId": 1, "id": "a", "rule": null }))
    .await;

  assert!(matches!(busy, Err(PortFailure::Answered { status: 409, .. })), "{busy:?}");
}

#[test]
fn the_store_reads_a_room_without_a_document_as_empty_and_a_crooked_one_as_a_failure() {
  let data = tempfile::tempdir().unwrap();

  assert_eq!(read_rules(data.path(), ROOM), Ok(vec![]));
  assert_eq!(rules_path(data.path(), ROOM), data.path().join("room-rules/-1.json"));

  let rows = change_rule(data.path(), ROOM, "a", Some((SOMEONE, "x"))).unwrap();

  assert_eq!(read_rules(data.path(), ROOM).unwrap(), rows);
  // Nothing is left beside the document: no lock, no staged copy.
  assert_eq!(std::fs::read_dir(data.path().join("room-rules")).unwrap().count(), 1);

  std::fs::write(rules_path(data.path(), ROOM), r#"[{"id": "a", "subject": "43", "text": "x"}]"#).unwrap();
  assert!(read_rules(data.path(), ROOM).is_err());
  assert!(matches!(change_rule(data.path(), ROOM, "a", None), Err(Unchanged::Failed(_))));
  // The lock is let go after a failed change too.
  assert!(!data.path().join("room-rules/-1.json.lock").exists());
}

#[test]
fn an_id_and_a_rule_are_held_to_their_shapes() {
  assert_eq!(id_problem("links-person_2"), None);
  assert!(id_problem(&"a".repeat(81)).is_some());
  assert_eq!(id_problem(&"a".repeat(80)), None);
  assert!(id_problem("no.dots").is_some());
  assert_eq!(rule_problem(-5, "x"), None);
  assert!(rule_problem(1, "\u{3000}").is_some());
  assert_eq!(rule_problem(1, &"\u{1F600}".repeat(4000)), None);
  assert!(rule_problem(1, &"\u{1F600}".repeat(4001)).is_some());
}
