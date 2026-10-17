//! The person-book digest: which rooms a launch takes on and the units it runs in, whom a batch writes for, where
//! a rewrite's paths will stand, which documents a room's book holds — and, on a stage of its own, one room's
//! messages folded in a batch, marked, sorted, committed and announced, or put back after three tries and told.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::time::Duration;

use bmo_brain::editors::archive::Message;
use bmo_brain::editors::fragments::store::{self, of_kind};
use bmo_brain::editors::people::batches::{PEOPLE_DIGEST_FAILED, beside_path, failure_notice, senders_of};
use bmo_brain::editors::people::prompts::SUCCESS_MARKER;
use bmo_brain::editors::people::rooms::{recorded_day, sheet_names};
use bmo_brain::editors::people::{PeopleDigest, chats_to_digest, sort_unit, worker_unit};
use bmo_brain::editors::recollection::recollection_for;
use bmo_brain::ports::agency::Outcome;
use fragment_desk::desk::Beside;
use serde_json::json;

use support::editing::{Stage, last_commit, row_write};
use support::receiver::message;
use support::{AGENT, ROOM};

#[test]
fn the_smallest_backlog_with_enough_waiting_goes_first() {
  let counts = BTreeMap::from([(1, 50), (2, 9), (3, 12), (4, 400)]);

  assert_eq!(chats_to_digest(&counts), vec![3, 1, 4]);
}

#[test]
fn a_rooms_worker_and_an_agents_sort_are_named_for_them() {
  assert_eq!(worker_unit(-5, "agent-b"), "bmo-people-digest-agent-b--5");
  assert_eq!(sort_unit("agent-a"), "bmo-people-sort-agent-a");
}

fn said(id: i64, sender: Option<i64>) -> Message {
  Message {
    chat_id: -5,
    message_id: id,
    sender_id: sender,
    sender_name: String::new(),
    sent_at: 0,
    text: String::new(),
    reply_to_message_id: None,
  }
}

#[test]
fn the_agents_own_words_are_no_sender_of_the_book() {
  let batch = [said(1, Some(9)), said(2, Some(42)), said(3, None), said(4, Some(9)), said(5, Some(7))];

  assert_eq!(senders_of(&batch, 42), vec![9, 7]);
}

#[test]
fn a_rewrite_names_its_paths_where_they_will_stand() {
  let beside = Beside {
    from: "rooms/5/people-reform".into(),
    to: "rooms/5/people".into(),
  };

  assert_eq!(
    beside_path(&beside, "rooms/5/people-reform/chunks/p1-a.md"),
    "rooms/5/people/chunks/p1-a.md"
  );
  assert_eq!(beside_path(&beside, "people/1/chunks/c1-a.md"), "people/1/chunks/c1-a.md");
}

#[test]
fn only_a_persons_sheet_is_read_as_their_document() {
  let dir = tempfile::tempdir().unwrap();

  for name in ["12.md", "index.md", "3.md", "x1.md", "sorted.txt"] {
    fs::write(dir.path().join(name), "").unwrap();
  }

  assert_eq!(sheet_names(dir.path()), Some(vec!["12.md".to_owned(), "3.md".to_owned()]));
  assert_eq!(sheet_names(&dir.path().join("none")), None);
}

#[test]
fn a_document_is_known_to_hold_what_it_says_from_the_day_it_was_last_committed() {
  assert_eq!(recorded_day(Some("2026-10-16\n")), "2026-10-16");
  assert_eq!(recorded_day(Some("")), "unknown");
  assert_eq!(recorded_day(Some("fatal: no commits")), "unknown");
  assert_eq!(recorded_day(None), "unknown");
}

#[test]
fn a_failed_batch_is_told_as_a_code_with_its_room_and_cause() {
  assert_eq!(
    serde_json::to_value(failure_notice(ROOM, Some("Family"), "Helper", 400, "It failed.")).unwrap(),
    json!({
      "code": PEOPLE_DIGEST_FAILED,
      "chatId": ROOM,
      "title": "Family",
      "agent": "Helper",
      "messages": 400,
      "cause": "It failed.",
    })
  );
}

const SPEAKER: i64 = 7;
const PIECE: &str = "rooms/-100/people/chunks/p7-address.md";
const BODY: &str = "# Seven (7) — Address\n\n## Facts\n- [date 2026-01-02 · confirmed 2026-01-02] Called Seven by everyone here.\n";
const TITLE: &str = "How the members call Seven, needed when a turn addresses that person by name";
const SHEET: &str = "## Address\n- [fact · confirmed 2026-01-02] Called Seven by everyone here.";

// A stage whose room holds a dozen messages of one speaker the agent has not read yet.
async fn talked() -> Stage {
  let stage = Stage::new().await;

  *stage.receiver.messages.lock().unwrap() =
    (1..=12).map(|id| message(ROOM, id, SPEAKER, 1_767_323_045 + id)).collect();

  stage
}

async fn digest(stage: &Stage) -> PeopleDigest<'_> {
  let ground = stage.ground();
  let recollection = recollection_for(ground.agency, ground.brain_root, ground.chat_key, AGENT)
    .await
    .unwrap()
    .unwrap();

  PeopleDigest {
    ground,
    recollection,
    catch_up: false,
    unkept_rooms: BTreeSet::new(),
  }
}

fn marked(stage: &Stage) -> BTreeSet<i64> {
  stage
    .receiver
    .marks
    .lock()
    .unwrap()
    .get(&(AGENT.to_owned(), ROOM))
    .cloned()
    .unwrap_or_default()
}

#[tokio::test]
async fn a_rooms_messages_are_folded_in_marked_sorted_committed_and_announced() {
  let stage = talked().await;

  stage.sessions_write(vec![(PIECE.to_owned(), BODY.to_owned())], vec![row_write(PIECE, TITLE, &[])]);
  stage.sessions_end(&[(Outcome::Answered, Some(&format!("Wrote the book.\n{SUCCESS_MARKER}")))]);
  stage.gate.sheets.lock().unwrap().push_back(Some(SHEET.to_owned()));

  let failed = digest(&stage).await.digest_chat(ROOM, Duration::ZERO).await.unwrap();

  assert!(!failed);

  // One planned batch of the twelve, on the person-book key, told which batch it is.
  let starts = stage.agency.calls.of("start_session");

  assert_eq!(starts.len(), 1);
  assert_eq!(
    (starts[0]["workKey"].clone(), starts[0]["step"].clone(), starts[0]["steps"].clone()),
    (json!("people-digest"), json!(1), json!(1))
  );
  assert!(
    starts[0]["prompt"]
      .as_str()
      .unwrap()
      .contains("Write for 7 into rooms/-100/people/chunks/")
  );

  // Every message is marked, the piece stands with its row and on the sorted list, and its sheet is written.
  assert_eq!(marked(&stage), (1..=12).collect());

  let rows = store::rows(stage.scratch.db(), &of_kind("person-book")).await.unwrap();

  assert_eq!(
    rows.iter().map(|row| (row.path.as_str(), row.chat_id, row.owner)).collect::<Vec<_>>(),
    vec![(PIECE, Some(ROOM), Some(SPEAKER))]
  );
  assert_eq!(
    fs::read_to_string(stage.brain.path().join("rooms/-100/people/sorted.txt")).unwrap(),
    "p7-address\n"
  );
  assert_eq!(
    fs::read_to_string(stage.brain.path().join("rooms/-100/people/7.md")).unwrap(),
    format!("{SHEET}\n")
  );
  assert_eq!(
    last_commit(stage.brain.path()),
    format!("Room {ROOM}'s person book covers 12 more messages")
  );

  let reloads = stage.agency.calls.of("reload_documents");

  assert_eq!(reloads.len(), 1);
  assert!(reloads[0]["paths"].as_array().unwrap().contains(&json!(PIECE)));
  assert_eq!(reloads[0]["rooms"], json!([ROOM]));
  assert!(stage.receiver.notices.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_batch_that_fails_three_times_is_put_back_left_unmarked_and_told() {
  let stage = talked().await;

  stage.sessions_write(vec![(PIECE.to_owned(), BODY.to_owned())], vec![row_write(PIECE, TITLE, &[])]);
  stage.sessions_end(&[
    (Outcome::Answered, Some("Wrote it.")),
    (Outcome::Failed, None),
    (Outcome::Answered, Some("Done.")),
  ]);

  let failed = digest(&stage).await.digest_chat(ROOM, Duration::ZERO).await.unwrap();

  assert!(failed);
  assert_eq!(stage.agency.calls.of("start_session").len(), 3);
  assert!(marked(&stage).is_empty());
  assert!(!stage.brain.path().join(PIECE).exists());
  assert!(store::rows(stage.scratch.db(), &of_kind("person-book")).await.unwrap().is_empty());

  let notices = stage.receiver.notices.lock().unwrap().clone();

  assert_eq!(notices.len(), 1);
  assert_eq!(notices[0].code, PEOPLE_DIGEST_FAILED);
  assert_eq!(
    (notices[0].params["chatId"].clone(), notices[0].params["messages"].clone()),
    (json!(ROOM), json!(12))
  );
  assert!(notices[0].params["cause"].as_str().unwrap().starts_with("All 3 tries failed."));
}

#[tokio::test]
async fn a_usage_limit_stops_the_room_with_its_messages_unmarked_and_nobody_told() {
  let stage = talked().await;

  stage.sessions_write(vec![(PIECE.to_owned(), BODY.to_owned())], vec![]);
  stage.sessions_end(&[(Outcome::Limit, None)]);

  let stopped = digest(&stage).await.digest_chat(ROOM, Duration::ZERO).await;

  assert!(matches!(stopped, Err(bmo_brain::editors::people::Stopped::Limit(_))));
  assert_eq!(stage.agency.calls.of("start_session").len(), 1);
  assert!(marked(&stage).is_empty());
  assert!(!stage.brain.path().join(PIECE).exists());
  assert!(stage.receiver.notices.lock().unwrap().is_empty());
}
