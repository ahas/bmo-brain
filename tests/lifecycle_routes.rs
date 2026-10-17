//! A room and an agent leaving the brain (docs/boundary-api.md, 「Rooms leaving」 and 「Agents forgetting」), served on
//! a local port: what a room holds counted, then removed with its folders committed out of the brain; an agent's
//! memory counted, then reset with its writer put out of the way; and an agent's editing lock forgotten.

mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use bmo_brain::editors::lock::{brain_lock_path, fence, fenced_out};
use bmo_brain::ports::agency::Registration;
use bmo_brain::ports::receiver::RoomAnswerers;
use serde_json::{Value, json};

use support::agency::registration;
use support::editing::last_commit;
use support::served::{Served, served};
use support::{AGENT, OTHER_AGENT, ROOM};

const OTHER_ROOM: i64 = -200;
const FOLDER: &str = "agents/agent-b";

fn put(root: &Path, path: &str, text: &str) {
  let at = root.join(path);

  std::fs::create_dir_all(at.parent().unwrap()).unwrap();
  std::fs::write(at, text).unwrap();
}

fn git(root: &Path, args: &[&str]) {
  let done = std::process::Command::new("git").current_dir(root).args(args).output().unwrap();

  assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
}

impl Served {
  // The second agent registered with a folder of its own.
  fn with_other_agent(&self) {
    self.agency.registrations.lock().unwrap().push(Registration {
      brain_folder: FOLDER.into(),
      ..registration(OTHER_AGENT)
    });
  }

  // One row of every table an agent's memory or a room's holding stands in.
  async fn rows_of(&self, agent: &str, chat_id: i64) {
    let scratch = &self.scratch;
    let slug = format!("{agent}-{chat_id}");

    scratch
      .run(
        r#"INSERT INTO "memories" ("slug", "type", "chat_id", "reach", "condition", "action", "set_at", "active", "hits", "event", "agent")
           VALUES ($1, 'promise', $2, 'all', '', 'do it', 1, 1, 0, '', $3)"#,
        vec![slug.clone().into(), chat_id.into(), agent.into()],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "fragments" ("kind", "chat_id", "slug", "title", "path", "position", "active", "hits", "set_at", "agent")
           VALUES ('room-rule', $1, $2, 'a rule', $3, 0, 1, 0, 1, $4)"#,
        vec![chat_id.into(), slug.clone().into(), format!("rooms/{chat_id}/rules/{slug}.md").into(), agent.into()],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "notes" ("chat_id", "author", "text", "created_at", "shared", "agent") VALUES ($1, 7, 'a note', 1, 0, $2)"#,
        vec![chat_id.into(), agent.into()],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "reminders" ("chat_id", "due_at", "reason", "fired", "created_at", "agent") VALUES ($1, 9999999999, 'r', 0, 1, $2)"#,
        vec![chat_id.into(), agent.into()],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "watches" ("agent", "chat_id", "by_chat", "kind", "condition", "note", "dedupe_key", "interval_seconds", "deadline_at", "state", "created_at")
           VALUES ($1, $2, $2, 'condition', 'true', 'n', $3, 60, 9999999999, 'active', 1)"#,
        vec![agent.into(), chat_id.into(), slug.into()],
      )
      .await;
  }

  async fn count(&self, table: &str, column: &str, value: Value) -> i64 {
    let value: sea_orm::Value = match value {
      Value::String(text) => text.into(),
      other => other.as_i64().unwrap().into(),
    };

    self
      .scratch
      .count(&format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "{column}" = $1"#), vec![value])
      .await
  }
}

#[tokio::test]
async fn a_room_is_counted_then_removed_with_its_folders_committed_out_of_the_brain() {
  let api = served().await;

  api.with_other_agent();
  api.rows_of(AGENT, ROOM).await;
  api.rows_of(OTHER_AGENT, ROOM).await;
  api.rows_of(AGENT, OTHER_ROOM).await;
  // A watch another room set on this one goes with it too.
  api
    .scratch
    .run(
      r#"INSERT INTO "watches" ("agent", "chat_id", "by_chat", "kind", "condition", "note", "dedupe_key", "interval_seconds", "deadline_at", "state", "created_at")
         VALUES ($1, $2, $3, 'condition', 'true', 'n', 'set-elsewhere', 60, 9999999999, 'active', 1)"#,
      vec![AGENT.into(), OTHER_ROOM.into(), ROOM.into()],
    )
    .await;
  api
    .scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ($1, '1'), ($2, '1'), ($3, '1')"#,
      vec![
        format!("people_digest_since:{AGENT}:{ROOM}").into(),
        format!("people_digest_since:{OTHER_AGENT}:{ROOM}").into(),
        format!("people_digest_since:{AGENT}:{OTHER_ROOM}").into(),
      ],
    )
    .await;
  put(api.root(), &format!("rooms/{ROOM}/rules/a.md"), "a rule");
  put(api.root(), &format!("{FOLDER}/rooms/{ROOM}/people/7.md"), "seven");
  put(api.root(), &format!("rooms/{OTHER_ROOM}/rules/b.md"), "another room's rule");
  git(api.root(), &["add", "--all"]);
  git(api.root(), &["commit", "--quiet", "-m", "the room's books"]);

  let preview = api.post("/rooms/removed", json!({ "chatId": ROOM, "preview": true })).await;
  let took = json!({
    "rows": { "memories": 2, "fragments": 2, "notes": 2, "reminders": 2, "watches": 3 },
    "settings": [format!("people_digest_since:{AGENT}:{ROOM}"), format!("people_digest_since:{OTHER_AGENT}:{ROOM}")],
    "folders": [format!("rooms/{ROOM}"), format!("{FOLDER}/rooms/{ROOM}")],
  });

  assert_eq!(preview, json!({ "took": took, "couldNot": [] }));
  assert_eq!(api.count("memories", "chat_id", json!(ROOM)).await, 2);
  assert!(api.root().join(format!("rooms/{ROOM}")).exists());

  let removed = api.post("/rooms/removed", json!({ "chatId": ROOM, "preview": false })).await;

  assert_eq!(removed, json!({ "took": took, "couldNot": [] }));

  for table in ["memories", "fragments", "notes", "reminders", "watches"] {
    assert_eq!(api.count(table, "chat_id", json!(ROOM)).await, 0, "{table}");
    assert_eq!(api.count(table, "chat_id", json!(OTHER_ROOM)).await, 1, "{table}");
  }

  assert_eq!(
    api
      .scratch
      .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" LIKE 'people_digest_since:%'"#, vec![])
      .await,
    1
  );
  assert!(!api.root().join(format!("rooms/{ROOM}")).exists());
  assert!(!api.root().join(format!("{FOLDER}/rooms/{ROOM}")).exists());
  assert!(api.root().join(format!("rooms/{OTHER_ROOM}/rules/b.md")).exists());
  assert!(
    last_commit(api.root()).contains(&format!("room {ROOM}'s registration is removed")),
    "{}",
    last_commit(api.root())
  );
}

#[tokio::test]
async fn an_agent_is_counted_then_reset_with_its_writer_put_out_of_the_way() {
  let api = served().await;

  api.with_other_agent();
  api.rows_of(OTHER_AGENT, ROOM).await;
  api.rows_of(AGENT, ROOM).await;
  put(api.root(), &format!("{FOLDER}/people/7/sheet.md"), "seven");
  git(api.root(), &["add", "--all"]);
  git(api.root(), &["commit", "--quiet", "-m", "agent-b's memory"]);
  api.receiver.answerers.lock().unwrap().push(RoomAnswerers {
    chat_id: ROOM,
    agents: vec![OTHER_AGENT.into()],
  });

  let preview = api.post("/agents/reset", json!({ "agent": OTHER_AGENT, "preview": true })).await;
  let took = json!({
    "rows": { "memories": 1, "reminders": 1, "watches": 1, "notes": 1, "fragments": 1 },
    "folder": FOLDER,
  });

  assert_eq!(preview, json!({ "took": took, "couldNot": [] }));
  assert!(api.receiver.calls.of("forget_read_marks").is_empty());

  // A program still working in the folder, as a writer's editor run would be, goes with the reset.
  let folder = api.root().join(FOLDER);
  let mut working = std::process::Command::new("sleep").arg("60").current_dir(&folder).spawn().unwrap();
  let reset = api.post("/agents/reset", json!({ "agent": OTHER_AGENT, "preview": false })).await;

  assert_eq!(reset, json!({ "took": took, "couldNot": [] }));

  let deadline = Instant::now() + Duration::from_secs(5);

  while working.try_wait().unwrap().is_none() && Instant::now() < deadline {
    std::thread::sleep(Duration::from_millis(50));
  }

  assert!(working.try_wait().unwrap().is_some(), "the program working in the folder still runs");

  for table in ["memories", "reminders", "watches", "notes", "fragments"] {
    assert_eq!(api.count(table, "agent", json!(OTHER_AGENT)).await, 0, "{table}");
    assert_eq!(api.count(table, "agent", json!(AGENT)).await, 1, "{table}");
  }

  assert!(!folder.exists());
  assert!(
    last_commit(api.root()).contains("agent-b's memory is reset"),
    "{}",
    last_commit(api.root())
  );
  assert_eq!(
    api.receiver.calls.of("forget_read_marks"),
    [json!({ "agent": OTHER_AGENT, "chatId": null })]
  );
  assert_eq!(
    api
      .scratch
      .count(
        r#"SELECT COUNT(*) FROM "settings" WHERE "key" = $1"#,
        vec![format!("people_digest_since:{OTHER_AGENT}:{ROOM}").into()]
      )
      .await,
    1
  );
  // The folder stands fenced: no writer begun before the reset takes its lock again.
  assert!(fenced_out(api.data.path(), FOLDER));
}

#[tokio::test]
async fn the_brain_roots_agent_is_never_reset_and_an_unregistered_agent_holds_no_folder() {
  let api = served().await;
  let root = api.post("/agents/reset", json!({ "agent": AGENT, "preview": true })).await;

  assert_eq!(root["refusal"]["code"], json!("brain-root"), "{root}");

  api.rows_of("agent-z", ROOM).await;

  let unknown = api.post("/agents/reset", json!({ "agent": "agent-z", "preview": false })).await;

  assert_eq!(unknown["took"]["folder"], json!(null));
  assert_eq!(unknown["took"]["rows"]["memories"], json!(1));
  assert_eq!(api.count("memories", "agent", json!("agent-z")).await, 0);
}

#[tokio::test]
async fn an_agents_editing_lock_its_queue_and_its_fence_are_forgotten() {
  let api = served().await;

  api.with_other_agent();
  fence(api.data.path(), FOLDER).unwrap();
  std::fs::write(brain_lock_path(api.data.path(), FOLDER), "").unwrap();

  let lock = brain_lock_path(api.data.path(), FOLDER);
  let removed = api.post("/agents/removed", json!({ "agent": OTHER_AGENT })).await;

  assert_eq!(removed, json!({ "ok": true, "left": [] }));
  assert!(!lock.exists());

  let fences: Vec<_> = std::fs::read_dir(api.data.path()).unwrap().flatten().collect();

  assert!(
    fences.is_empty(),
    "{:?}",
    fences.iter().map(|entry| entry.file_name()).collect::<Vec<_>>()
  );
  assert_eq!(
    api.post("/agents/removed", json!({ "agent": "agent-z" })).await,
    json!({ "ok": true, "left": [] })
  );
}
