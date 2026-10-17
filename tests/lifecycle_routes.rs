//! A room and an agent leaving the brain (docs/boundary-api.md, 「Rooms leaving」 and 「Agents forgetting」), served on
//! a local port: what a room holds counted, then removed with its folders committed out of the brain; an agent's
//! memory counted, then reset with its writer put out of the way; an agent's editing lock forgotten; and the pieces
//! a caller's own reset stands on: an agent's folder fenced, its lock named, its book of a room made to read from a
//! moment, and what its stopped runs left put back.

mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use bmo_brain::editors::archive::reading_since;
use bmo_brain::editors::lock::{brain_lock_path, fence, fenced_out};
use bmo_brain::ports::agency::Registration;
use bmo_brain::ports::http::PortFailure;
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
      brain_folder: Some(FOLDER.into()),
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

// The files standing directly in the data root, by name.
fn data_files(data: &Path) -> Vec<String> {
  let mut names: Vec<String> = std::fs::read_dir(data)
    .unwrap()
    .flatten()
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect();

  names.sort();
  names
}

#[tokio::test]
async fn an_agents_folder_is_fenced_and_its_lock_named_relative_to_the_data_root() {
  let api = served().await;

  api.with_other_agent();
  assert!(!fenced_out(api.data.path(), FOLDER));
  assert_eq!(api.post("/agents/fence", json!({ "agent": OTHER_AGENT })).await, json!({ "ok": true }));
  // No writer begun before the fence, as this process was, takes the folder's lock again; another folder is free.
  assert!(fenced_out(api.data.path(), FOLDER));
  assert!(!fenced_out(api.data.path(), "agents/agent-c"));
  assert!(!fenced_out(api.data.path(), ""));

  // An agent the agency registers no folder for has nothing fenced.
  let before = data_files(api.data.path());

  assert_eq!(api.post("/agents/fence", json!({ "agent": "agent-z" })).await, json!({ "ok": true }));
  assert_eq!(data_files(api.data.path()), before);

  // The lock's path is the one every writer of the folder takes, relative to the data root.
  let path = api.post("/agents/lock", json!({ "agent": OTHER_AGENT })).await;

  assert_eq!(path, json!({ "path": "people-digest.agents%2Fagent-b.lock" }));
  assert_eq!(
    api.data.path().join(path["path"].as_str().unwrap()),
    brain_lock_path(api.data.path(), FOLDER)
  );
  assert_eq!(
    api.post("/agents/lock", json!({ "agent": AGENT })).await,
    json!({ "path": "people-digest.lock" })
  );
  assert_eq!(api.post("/agents/lock", json!({ "agent": "agent-z" })).await, json!({ "path": null }));
}

#[tokio::test]
async fn an_agents_book_of_a_room_is_made_to_read_from_a_moment() {
  let api = served().await;

  assert_eq!(
    api
      .post(
        "/agents/book-since",
        json!({ "agent": OTHER_AGENT, "chatId": ROOM, "at": 1_791_590_400 })
      )
      .await,
    json!({ "ok": true })
  );
  assert_eq!(reading_since(api.scratch.db(), OTHER_AGENT, ROOM).await.unwrap(), Some(1_791_590_400));
  assert_eq!(reading_since(api.scratch.db(), OTHER_AGENT, OTHER_ROOM).await.unwrap(), None);
  assert_eq!(reading_since(api.scratch.db(), AGENT, ROOM).await.unwrap(), None);

  // A later moment replaces the first.
  api
    .post(
      "/agents/book-since",
      json!({ "agent": OTHER_AGENT, "chatId": ROOM, "at": 1_791_600_000 }),
    )
    .await;
  assert_eq!(reading_since(api.scratch.db(), OTHER_AGENT, ROOM).await.unwrap(), Some(1_791_600_000));

  // A body without its moment is not read.
  match api
    .caller
    .post::<Value>("/agents/book-since", &json!({ "agent": OTHER_AGENT, "chatId": ROOM }))
    .await
  {
    Err(PortFailure::Answered {
      status,
      answer: Some(answer),
      ..
    }) => assert_eq!((status, answer.code.as_str()), (400, "bad-request")),
    other => panic!("not refused: {other:?}"),
  }
}

#[tokio::test]
async fn what_an_agents_stopped_runs_left_in_its_folder_is_put_back_and_no_other_folder_touched() {
  let api = served().await;
  let root = api.root();

  api.with_other_agent();
  put(root, &format!("{FOLDER}/people/7/sheet.md"), "seven");
  put(root, "people/8/sheet.md", "eight");
  git(root, &["add", "--all"]);
  git(root, &["commit", "--quiet", "-m", "both books"]);

  // A stopped run's writes, kept nowhere: a changed sheet and a new piece in the agent's folder; the chat agent's
  // own writer is still at work in the brain root.
  put(root, &format!("{FOLDER}/people/7/sheet.md"), "seven, half rewritten");
  put(root, &format!("{FOLDER}/people/7/chunks/new.md"), "a piece");
  put(root, "people/8/sheet.md", "eight, being written");

  assert_eq!(
    api.post("/agents/put-back", json!({ "agent": OTHER_AGENT })).await,
    json!({ "ok": true })
  );
  assert_eq!(std::fs::read_to_string(root.join(FOLDER).join("people/7/sheet.md")).unwrap(), "seven");
  assert!(!root.join(FOLDER).join("people/7/chunks/new.md").exists());
  assert_eq!(std::fs::read_to_string(root.join("people/8/sheet.md")).unwrap(), "eight, being written");

  // The chat agent's folder is the brain root, which holds every other folder, and is never put back.
  let refused = api.post("/agents/put-back", json!({ "agent": AGENT })).await;

  assert_eq!(refused["refusal"]["code"], json!("brain-root"), "{refused}");
  assert_eq!(std::fs::read_to_string(root.join("people/8/sheet.md")).unwrap(), "eight, being written");

  // An agent with no folder, or whose folder does not stand, has nothing undone.
  assert_eq!(api.post("/agents/put-back", json!({ "agent": "agent-z" })).await, json!({ "ok": true }));
  std::fs::remove_dir_all(root.join(FOLDER)).unwrap();
  assert_eq!(
    api.post("/agents/put-back", json!({ "agent": OTHER_AGENT })).await,
    json!({ "ok": true })
  );
}

#[tokio::test]
async fn the_chat_agents_folder_is_the_brain_root_where_the_agency_names_it_none() {
  let api = served().await;

  api.agency.registrations.lock().unwrap()[0].brain_folder = None;
  api.agency.registrations.lock().unwrap().push(Registration {
    brain_folder: None,
    ..registration(OTHER_AGENT)
  });

  assert_eq!(
    api.post("/agents/lock", json!({ "agent": AGENT })).await,
    json!({ "path": "people-digest.lock" })
  );
  assert_eq!(api.post("/agents/lock", json!({ "agent": OTHER_AGENT })).await, json!({ "path": null }));
  assert_eq!(
    api.post("/agents/put-back", json!({ "agent": AGENT })).await["refusal"]["code"],
    json!("brain-root")
  );
}
