//! The brain's tools answered through the served routes (docs/boundary-api.md, 「Its tools」): a memory, a note and
//! a watch filed through the memory tool as the turn's agent's in the turn's room, a call outside a room's or an
//! agent's turn refused before anything is written, and the fragment tool writing on a desk an editor's batch
//! opened, the table left to the batch.

mod support;

use std::collections::BTreeMap;
use std::path::PathBuf;

use bmo_brain::editors::DocumentBackup;
use bmo_brain::editors::desk::{DeskScope, desk_dir, open_desk};
use bmo_brain::editors::fragments::libraries::{AgentFolder, Brain as BrainFolders};
use bmo_brain::editors::fragments::names;
use bmo_brain::editors::fragments::store::of_kind;
use chrono::{Duration, FixedOffset, Utc};
use fragment_desk::desk::Desk;
use serde_json::json;
use tool_call::{Brain, Session};

use support::served::{Served, TOKEN, served};
use support::{AGENT, ROOM};

const TITLE: &str = "A note on the order of a deploy, read when a deploy has to be run again.";

fn turn_of(agent: Option<&str>, chat: Option<i64>) -> Session {
  let mut variables = BTreeMap::new();

  if let Some(agent) = agent {
    variables.insert("BMO_TURN_AGENT".to_owned(), agent.to_owned());
  }

  if let Some(chat) = chat {
    variables.insert("BMO_TURN_CHAT".to_owned(), chat.to_string());
  }

  Session {
    run_variables: variables,
    ..Session::default()
  }
}

impl Served {
  fn brain(&self) -> Brain {
    Brain::at(&self.base, TOKEN)
  }

  async fn rows(&self, table: &str) -> i64 {
    self
      .scratch
      .count(&format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "agent" = $1"#), vec![AGENT.into()])
      .await
  }
}

#[tokio::test]
async fn a_memory_a_note_and_a_watch_filed_through_the_tool_are_the_turns_agents_in_the_turns_room() {
  let api = served().await;
  let brain = api.brain();
  let turn = turn_of(Some(AGENT), Some(ROOM));
  let filed = memory_tool::call_on(
    &brain,
    &turn,
    "memory_add",
    json!({ "type": "promise", "action": "tell the result tomorrow" }),
  )
  .await;

  assert!(!filed.is_error, "{}", filed.plain());

  let listed = memory_tool::call_on(&brain, &turn, "memory_list", json!({})).await;
  let lines = listed.structured.as_ref().unwrap()["lines"].as_array().unwrap().clone();

  assert!(
    lines.iter().any(|line| line.as_str().unwrap().contains("tell the result tomorrow")),
    "{lines:?}"
  );
  assert_eq!(
    api
      .scratch
      .count(
        r#"SELECT COUNT(*) FROM "memories" WHERE "agent" = $1 AND "chat_id" = $2"#,
        vec![AGENT.into(), ROOM.into()]
      )
      .await,
    1
  );

  let noted =
    memory_tool::call_on(&brain, &turn, "note_add", json!({ "text": "minutes on Fridays" })).await;

  assert!(noted.plain().ends_with(&format!("kept for chat {ROOM}")), "{}", noted.plain());

  let seoul = FixedOffset::east_opt(9 * 3600).unwrap();
  let tomorrow = (Utc::now() + Duration::days(1))
    .with_timezone(&seoul)
    .format("%Y-%m-%d %H:%M")
    .to_string();
  let timer =
    memory_tool::call_on(&brain, &turn, "watch_set", json!({ "at": tomorrow, "note": "look again" })).await;

  assert!(!timer.is_error, "{}", timer.plain());
  assert_eq!((api.rows("notes").await, api.rows("watches").await), (1, 1));

  // A refusal comes back marked as one, after the route's own sentence.
  let refused = memory_tool::call_on(&brain, &turn, "memory_done", json!({ "memory": "nobody" })).await;

  assert!(
    refused.is_error && refused.plain().contains("there is no memory 'nobody'"),
    "{}",
    refused.plain()
  );
}

#[tokio::test]
async fn a_call_outside_a_room_or_an_agents_turn_is_refused_before_anything_is_written() {
  let api = served().await;
  let brain = api.brain();
  let timer = |chat: Option<i64>| json!({ "chat": chat, "at": "2026-09-23 09:00", "note": "x" });
  let roomless = memory_tool::call_on(&brain, &turn_of(None, None), "watch_set", timer(None)).await;
  let agentless = memory_tool::call_on(&brain, &turn_of(None, None), "watch_set", timer(Some(ROOM))).await;
  let promise = memory_tool::call_on(
    &brain,
    &turn_of(None, Some(ROOM)),
    "memory_add",
    json!({ "type": "promise", "action": "tell the result tomorrow" }),
  )
  .await;

  assert!(
    roomless.is_error && roomless.plain().contains("outside any room's turn"),
    "{}",
    roomless.plain()
  );
  assert!(
    agentless.is_error && agentless.plain().contains("outside any agent's turn"),
    "{}",
    agentless.plain()
  );
  assert!(
    promise.is_error && promise.plain().contains("outside any agent's turn"),
    "{}",
    promise.plain()
  );
  assert_eq!(api.scratch.count(r#"SELECT COUNT(*) FROM "watches""#, vec![]).await, 0);
  assert_eq!(api.scratch.count(r#"SELECT COUNT(*) FROM "memories""#, vec![]).await, 0);
}

#[tokio::test]
async fn the_fragment_tool_writes_on_its_runs_desk_through_the_brain() {
  let api = served().await;
  let piece = api.root().join("knowledge/deploy.md");

  std::fs::create_dir_all(piece.parent().unwrap()).unwrap();
  std::fs::write(&piece, "- [date 2026-10-03 · confirmed 2026-10-05] #758 the order\n").unwrap();

  let folders = BrainFolders::new(
    api.root().to_path_buf(),
    AGENT,
    vec![AgentFolder {
      agent: AGENT.into(),
      folder: String::new(),
      label: Some("Helper".into()),
    }],
  );
  let dir = desk_dir(api.data.path());

  std::fs::create_dir_all(&dir).unwrap();

  let desk: PathBuf = open_desk(
    &DeskScope {
      room: Some(ROOM),
      agent: Some(AGENT.into()),
      places: vec!["knowledge".into()],
      reach: vec![of_kind("library-knowledge")],
      ..DeskScope::default()
    },
    &dir,
    &api.scratch.address(),
    api.root(),
    &DocumentBackup::new(),
    &names(&folders, AGENT, &[]),
  )
  .unwrap();
  let editor = Session {
    fragment_desk: Some(desk.clone()),
    ..Session::default()
  };
  let brain = api.brain();
  let owed = fragment_tool::call_on(&brain, &editor, "fragment_owed", json!({})).await;

  assert_eq!(owed.plain(), "These pieces have no row written in this run yet:\nknowledge/deploy.md");

  let short = fragment_tool::call_on(
    &brain,
    &editor,
    "fragment_write",
    json!({ "path": "knowledge/deploy.md", "title": "Too short.", "keywords": "deploy-order", "items": [] }),
  )
  .await;

  assert!(
    short.is_error && short.plain().starts_with("REFUSED: The title of knowledge/deploy.md"),
    "{}",
    short.plain()
  );

  let written = fragment_tool::call_on(
    &brain,
    &editor,
    "fragment_write",
    json!({ "path": "knowledge/deploy.md", "title": TITLE, "keywords": "deploy-order", "items": [758] }),
  )
  .await;

  assert!(!written.is_error, "{}", written.plain());
  assert!(written.plain().ends_with("[written in this run]"), "{}", written.plain());
  assert_eq!(Desk::read(&desk).unwrap().writes.len(), 1);
  assert_eq!(api.scratch.count(r#"SELECT COUNT(*) FROM "fragments""#, vec![]).await, 0);

  // A desk the record does not name, or none, is no desk.
  let stranger = Session {
    fragment_desk: Some(api.data.path().join("elsewhere.json")),
    ..Session::default()
  };

  assert!(
    fragment_tool::call_on(&brain, &stranger, "fragment_owed", json!({}))
      .await
      .plain()
      .starts_with("REFUSED: This run has no fragment desk")
  );
}
