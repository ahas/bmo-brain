//! The notes digest: a room's notes split by whether they can be taken as the owner's, held to what only the owner's
//! notes may do, and — on a stage of its own — one room's notes filed into the agent's library, committed and
//! announced, or put back and told to the owner where the run does not stand.

mod support;

use std::collections::HashSet;
use std::fs;

use bmo_brain::editors::archive::Note;
use bmo_brain::editors::fragments::store::{self, of_kind};
use bmo_brain::editors::memo::{
  DIGEST_FAILED, FAILURE_LOG, MemoDigest, RULE_CANDIDATE, SUCCESS_MARKER, added_candidates,
  batches_by_owner, candidate_notice, failure_notice, owner_only_problems, put_back_reason,
};
use bmo_brain::ports::agency::TurnOutcome;
use sea_orm::Value;
use serde_json::json;

use support::editing::{Stage, last_commit, row_write};
use support::receiver::message;
use support::{AGENT, OWNER, ROOM};

fn note(id: i64, author: Option<i64>) -> Note {
  Note {
    id,
    agent: AGENT.into(),
    chat_id: -5,
    author,
    text: format!("note {id}"),
    shared: false,
    created_at: 1_767_323_045,
  }
}

#[test]
fn the_owners_notes_are_a_batch_of_their_own_and_come_first() {
  let notes = vec![note(1, Some(9)), note(2, Some(7)), note(3, Some(7))];
  let batches = batches_by_owner(&notes, |one| one.author == Some(7));

  assert_eq!(batches.len(), 2);
  assert!(batches[0].owner_batch);
  assert_eq!(batches[0].notes.iter().map(|one| one.id).collect::<Vec<_>>(), vec![2, 3]);
  assert_eq!(batches[1].notes.iter().map(|one| one.id).collect::<Vec<_>>(), vec![1]);
  assert_eq!(batches_by_owner(&notes[..1], |_| false).len(), 1);
}

#[test]
fn a_candidate_is_an_item_line_that_was_not_there_before() {
  assert_eq!(added_candidates("- a\n", "- a\n- b\nprose\n"), vec!["- b"]);
}

#[test]
fn only_the_owners_batch_may_touch_the_rules_every_room_reads() {
  assert!(owner_only_problems("- a\n", "", &["knowledge/k.md".into()], true).is_empty());

  let problems = owner_only_problems("- a\n", "", &["knowledge/k.md".into()], false);

  assert_eq!(
    problems,
    vec![
      "made, changed or removed 1 rules for every room on notes that are not the owner's: knowledge/k.md"
        .to_owned(),
      "took 1 candidates off on notes that are not the owner's: - a".to_owned(),
    ]
  );
}

#[test]
fn a_put_back_says_each_way_the_run_fell_short() {
  assert_eq!(
    put_back_reason(&[], &["a".into(), "b".into()], &["x".into()]),
    "The rows the run wrote through the fragment tool did not match what it wrote on disk (a and 1 more), so everything it wrote was put back. The run changed the rules every room reads on notes that are not the owner's, which only the owner's own notes may do, so everything it wrote was put back."
  );
}

#[test]
fn a_failure_and_a_candidate_are_told_as_codes_with_their_parameters() {
  assert_eq!(
    serde_json::to_value(failure_notice(ROOM, None, "Helper", 2, "It failed.")).unwrap(),
    json!({ "code": DIGEST_FAILED, "chatId": ROOM, "title": null, "agent": "Helper", "notes": 2, "cause": "It failed." })
  );
  assert_eq!(
    serde_json::to_value(candidate_notice("Helper", "- [date 2026-01-02] 7 · room -5 · Speak slowly."))
      .unwrap(),
    json!({ "code": RULE_CANDIDATE, "agent": "Helper", "candidate": "[date 2026-01-02] 7 · room -5 · Speak slowly." })
  );
}

const CHUNK_PATH: &str = "knowledge/afternoon-tea.md";
const CHUNK: &str = "# Afternoon tea\n\nSource room: -100\n\n## Facts\n- [date 2026-01-02 · confirmed 2026-01-02] Tea is served at four.\n";
const TITLE: &str =
  "When the members break for tea in the afternoon, needed when a turn plans a meeting at four";
const SENT_AT: i64 = 1_767_323_045;

// A stage whose room holds one note of the owner's, written ten minutes after the owner spoke there.
async fn noted() -> Stage {
  let stage = Stage::new().await;

  stage
    .scratch
    .run(
      r#"INSERT INTO "notes" ("chat_id", "author", "text", "created_at", "agent") VALUES ($1, $2, $3, $4, $5)"#,
      vec![
        Value::from(ROOM),
        Value::from(OWNER),
        Value::from("Tea is served at four in this room"),
        Value::from(SENT_AT),
        Value::from(AGENT),
      ],
    )
    .await;
  *stage.receiver.messages.lock().unwrap() = vec![message(ROOM, 1, OWNER, SENT_AT - 600)];

  stage
}

async fn notes_left(stage: &Stage) -> i64 {
  stage.scratch.count(r#"SELECT COUNT(*) FROM "notes""#, vec![]).await
}

#[tokio::test]
async fn a_rooms_notes_are_filed_into_the_agents_library_committed_and_announced() {
  let stage = noted().await;

  stage.sessions_write(
    vec![(CHUNK_PATH.to_owned(), CHUNK.to_owned())],
    vec![row_write(CHUNK_PATH, TITLE, &["tea"])],
  );
  stage.sessions_end(&[(TurnOutcome::Answered, Some(&format!("Filed the note.\n{SUCCESS_MARKER}")))]);

  assert_eq!(
    MemoDigest {
      ground: stage.ground()
    }
    .run(None)
    .await
    .unwrap(),
    0
  );

  // The run was asked for on the digest key, in the agent's folder, told the note is the owner's.
  let starts = stage.agency.calls.of("editor_turn");

  assert_eq!(starts.len(), 1);
  assert_eq!(
    (stage.agency.calls.of("editor_session")[0]["agent"].clone(), starts[0]["agent"].clone()),
    (json!("digest"), json!(AGENT))
  );
  assert_eq!(starts[0]["chatId"], json!(ROOM));
  assert_eq!(starts[0]["workdir"], json!(""));
  assert!(starts[0]["prompt"].as_str().unwrap().contains("| owner | this chat |"));
  assert!(starts[0]["prompt"].as_str().unwrap().contains("Tea is served at four in this room"));

  // The note is gone, its chunk stands with its row, and the batch is committed.
  assert_eq!(notes_left(&stage).await, 0);
  assert_eq!(fs::read_to_string(stage.brain.path().join(CHUNK_PATH)).unwrap(), CHUNK);

  let rows = store::rows(stage.scratch.db(), &of_kind("library-knowledge")).await.unwrap();

  assert_eq!(
    rows
      .iter()
      .map(|row| (row.path.as_str(), row.chat_id, row.agent.as_str()))
      .collect::<Vec<_>>(),
    vec![(CHUNK_PATH, Some(ROOM), AGENT)]
  );
  assert!(
    last_commit(stage.brain.path()).starts_with(&format!("Room {ROOM}'s notes are filed into Helper's"))
  );

  // What it wrote passed the sanitiser, and the room's sessions were told to read it again.
  let sanitised: HashSet<String> = stage
    .gate
    .calls
    .of("sanitise")
    .iter()
    .map(|call| call["path"].as_str().unwrap().to_owned())
    .collect();

  assert!(sanitised.contains(CHUNK_PATH));

  let reloads = stage.agency.calls.of("reload_documents");

  assert_eq!(reloads.len(), 1);
  assert!(reloads[0]["paths"].as_array().unwrap().contains(&json!(CHUNK_PATH)));
  assert_eq!(reloads[0]["rooms"], json!([ROOM]));
  assert!(stage.receiver.notices.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_run_without_its_marker_is_put_back_and_told_to_the_owner_with_its_notes_left() {
  let stage = noted().await;

  stage.sessions_write(
    vec![(CHUNK_PATH.to_owned(), CHUNK.to_owned())],
    vec![row_write(CHUNK_PATH, TITLE, &["tea"])],
  );
  stage.sessions_end(&[(TurnOutcome::Answered, Some("Filed the note, I think."))]);

  assert_eq!(
    MemoDigest {
      ground: stage.ground()
    }
    .run(None)
    .await
    .unwrap(),
    1
  );
  assert_eq!(notes_left(&stage).await, 1);
  assert!(!stage.brain.path().join(CHUNK_PATH).exists());
  assert!(store::rows(stage.scratch.db(), &of_kind("library-knowledge")).await.unwrap().is_empty());

  let notices = stage.receiver.notices.lock().unwrap().clone();

  assert_eq!(notices.len(), 1);
  assert_eq!(notices[0].code, DIGEST_FAILED);
  assert_eq!(
    (notices[0].params["chatId"].clone(), notices[0].params["notes"].clone()),
    (json!(ROOM), json!(1))
  );
  assert!(
    notices[0].params["cause"]
      .as_str()
      .unwrap()
      .contains("without saying it had finished its work")
  );
  assert!(!stage.data.path().join(FAILURE_LOG).exists());
}

#[tokio::test]
async fn a_usage_limit_stops_the_digest_with_its_notes_left_and_nobody_told() {
  let stage = noted().await;

  stage.sessions_write(vec![(CHUNK_PATH.to_owned(), CHUNK.to_owned())], vec![]);
  stage.agency.limited();

  assert_eq!(
    MemoDigest {
      ground: stage.ground()
    }
    .run(None)
    .await
    .unwrap(),
    1
  );
  assert_eq!(notes_left(&stage).await, 1);
  assert!(!stage.brain.path().join(CHUNK_PATH).exists());
  assert!(stage.receiver.notices.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_note_by_anyone_but_the_owner_is_no_owners_and_may_not_touch_a_rule_for_every_room() {
  let stage = noted().await;

  // The note's author is a member, and the run writes a chunk every room reads.
  stage.scratch.run(r#"UPDATE "notes" SET "author" = 7"#, vec![]).await;
  stage.sessions_write(
    vec![(CHUNK_PATH.to_owned(), CHUNK.to_owned())],
    vec![fragment_desk::desk::FragmentWrite {
      shared: Some(true),
      ..row_write(CHUNK_PATH, TITLE, &["tea"])
    }],
  );
  stage.sessions_end(&[(TurnOutcome::Answered, Some(SUCCESS_MARKER))]);

  assert_eq!(
    MemoDigest {
      ground: stage.ground()
    }
    .run(None)
    .await
    .unwrap(),
    1
  );

  let starts = stage.agency.calls.of("editor_turn");

  assert!(starts[0]["prompt"].as_str().unwrap().contains("| member | this chat |"));
  assert_eq!(notes_left(&stage).await, 1);
  assert!(!stage.brain.path().join(CHUNK_PATH).exists());
  assert!(
    stage.receiver.notices.lock().unwrap()[0].params["cause"]
      .as_str()
      .unwrap()
      .contains("which only the owner's own notes may do")
  );
}
