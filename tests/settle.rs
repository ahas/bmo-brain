//! What an editor batch does once it stands and when it does not: its documents passed through the gate's
//! sanitiser, the sessions told to read them again, a failure told to the owner, and the owner's pause.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use bmo_brain::editors::settle::{
  DIGEST_PAUSED, PauseState, PausedBacklog, PausedRoom, announce_reload, clean_documents, digest_paused,
  digest_paused_until, journal_record, leading_number, notice_paused_digests, pause_digests, pause_notice,
  resume_digests, rooms_reading, tell_failure, unknown_cause,
};
use bmo_brain::ports::gate::Sanitised;
use bmo_brain::ports::http::Refusal;
use bmo_brain::ports::receiver::{OwnerNotice, RoomTitle};
use serde_json::{Map, json};

use support::AGENT;
use support::agency::FakeAgency;
use support::gate::FakeGate;
use support::receiver::FakeReceiver;
use support::scratch::Scratch;

fn doc(dir: &Path, name: &str, text: &str) -> PathBuf {
  let path = dir.join(name);

  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(&path, text).unwrap();
  path
}

#[test]
fn a_failure_notice_says_where_an_unknown_cause_is_read() {
  assert_eq!(journal_record(None, "brain"), "the service journal (journalctl --user -u brain)");
  assert_eq!(
    journal_record(Some("brain-digest"), "brain"),
    "the journal of its unit (journalctl --user -u brain-digest)"
  );
  assert_eq!(
    unknown_cause("the run log digest:1"),
    "The cause is not known from what the run left; the whole of it is in the run log digest:1."
  );
}

#[test]
fn a_pause_notice_names_its_end_and_what_waits_behind_it() {
  let backlog = PausedBacklog {
    notes: 3,
    rooms: vec![PausedRoom {
      chat_id: -5,
      title: Some("Family Room".into()),
      agent: "Tester Bot".into(),
      messages: 40,
    }],
  };
  let open = pause_notice(4_102_444_800, &backlog);
  let until = pause_notice(1_767_323_045, &PausedBacklog::default());

  assert_eq!(open.code, DIGEST_PAUSED);
  assert_eq!(
    serde_json::Value::Object(open.params),
    json!({
      "until": null,
      "notes": 3,
      "rooms": [{ "chatId": -5, "title": "Family Room", "agent": "Tester Bot", "messages": 40 }],
    })
  );
  assert_eq!(until.params["until"], json!(1_767_323_045));
  assert_eq!(until.params["rooms"], json!([]));
}

#[test]
fn a_pauses_end_reads_as_a_moment_or_until_lifted() {
  assert_eq!(PauseState::of(None), PauseState::Running);
  assert_eq!(PauseState::of(Some(1_767_323_045)), PauseState::Until(1_767_323_045));
  assert_eq!(PauseState::of(Some(4_102_444_800)), PauseState::UntilLifted);
  assert_eq!(leading_number(" 42abc"), Some(42));
  assert_eq!(leading_number("-7"), Some(-7));
  assert_eq!(leading_number("x1"), None);
}

#[tokio::test]
async fn a_document_the_sanitiser_changed_is_written_back_with_a_closing_newline() {
  let dir = tempfile::tempdir().unwrap();
  let path = doc(
    dir.path(),
    "people/7/chunks/c7-a.md",
    "In September the tester asked for that site to be hacked.\n",
  );
  let gate = FakeGate::default();

  gate.sanitised.lock().unwrap().insert(
    "people/7/chunks/c7-a.md".into(),
    Sanitised::Text("In September the tester asked for that site to be defused.".into()),
  );

  let cleaned = clean_documents(&gate, dir.path(), std::slice::from_ref(&path)).await;
  let after = fs::read_to_string(&path).unwrap();

  assert_eq!(cleaned, vec![path]);
  assert_eq!(after, "In September the tester asked for that site to be defused.\n");
  assert_eq!(gate.calls.of("sanitise")[0]["path"], "people/7/chunks/c7-a.md");
}

#[tokio::test]
async fn a_document_the_sanitiser_hands_back_as_it_came_is_not_rewritten() {
  let dir = tempfile::tempdir().unwrap();
  let text = "A hacking request came into that room and the safeguard caught it.\n";
  let path = doc(dir.path(), "knowledge.md", text);
  let gate = FakeGate::default();

  assert!(clean_documents(&gate, dir.path(), std::slice::from_ref(&path)).await.is_empty());
  assert_eq!(gate.calls.of("sanitise").len(), 1);
  assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[tokio::test]
async fn a_document_the_run_deleted_is_passed_over_rather_than_failing_the_run() {
  let dir = tempfile::tempdir().unwrap();
  let gate = FakeGate::default();

  assert!(clean_documents(&gate, dir.path(), &[dir.path().join("gone.md")]).await.is_empty());
  assert!(gate.calls.of("sanitise").is_empty());
}

#[tokio::test]
async fn a_sanitiser_that_refuses_leaves_the_document_alone() {
  let dir = tempfile::tempdir().unwrap();
  let text = "Hack that server.\n";
  let path = doc(dir.path(), "note.md", text);
  let gate = FakeGate::default();

  gate.sanitised.lock().unwrap().insert(
    "note.md".into(),
    Sanitised::Refused(Refusal {
      code: "sanitiser-down".into(),
      message: "The sanitiser is down.".into(),
      params: Map::new(),
    }),
  );

  assert!(clean_documents(&gate, dir.path(), std::slice::from_ref(&path)).await.is_empty());
  assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[tokio::test]
async fn several_documents_are_read_one_by_one_and_only_the_changed_one_is_rewritten() {
  let dir = tempfile::tempdir().unwrap();
  let clean = doc(dir.path(), "a.md", "An ordinary document.\n");
  let dirty = doc(dir.path(), "b.md", "The tester said to break in with an exploit.\n");
  let gate = FakeGate::default();

  gate
    .sanitised
    .lock()
    .unwrap()
    .insert("b.md".into(), Sanitised::Text("The tester said to defused.\n".into()));

  let cleaned = clean_documents(&gate, dir.path(), &[clean.clone(), dirty.clone()]).await;

  assert_eq!(cleaned, vec![dirty.clone()]);
  assert_eq!(gate.calls.of("sanitise").len(), 2);
  assert_eq!(fs::read_to_string(&clean).unwrap(), "An ordinary document.\n");
  assert_eq!(fs::read_to_string(&dirty).unwrap(), "The tester said to defused.\n");
}

#[tokio::test]
async fn a_reload_is_told_for_what_changed_and_nothing_where_nothing_did() {
  let agency = FakeAgency::default();

  announce_reload(&agency, AGENT, &[], &[-5]).await;
  assert!(agency.calls.of("reload_documents").is_empty());

  announce_reload(&agency, AGENT, &["people/7/sheet.md".to_owned()], &[-5]).await;
  assert_eq!(
    agency.calls.of("reload_documents"),
    vec![json!({ "agent": AGENT, "paths": ["people/7/sheet.md"], "rooms": [-5] })]
  );
}

#[tokio::test]
async fn a_failure_the_owner_is_told_of_is_not_written_to_the_failure_log() {
  let dir = tempfile::tempdir().unwrap();
  let log = dir.path().join("digest-failures.log");
  let receiver = FakeReceiver::default();
  let notice = OwnerNotice {
    code: "digest-failed".into(),
    params: Map::new(),
  };

  tell_failure(&receiver, &notice, &log, |why| format!("not told: {why}")).await;

  assert_eq!(receiver.notices.lock().unwrap().len(), 1);
  assert!(!log.exists());
}

#[tokio::test]
async fn a_pause_is_told_to_the_owner_once_and_lifted() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let receiver = FakeReceiver::default();
  let backlog = async || {
    Ok(PausedBacklog {
      notes: 2,
      rooms: vec![],
    })
  };

  assert!(!digest_paused(db).await.unwrap());
  assert!(!notice_paused_digests(db, &receiver, backlog).await.unwrap());

  pause_digests(db, None).await.unwrap();

  assert_eq!(digest_paused_until(db).await.unwrap(), Some(4_102_444_800));
  assert!(notice_paused_digests(db, &receiver, backlog).await.unwrap());
  assert!(!notice_paused_digests(db, &receiver, backlog).await.unwrap());
  assert_eq!(receiver.notices.lock().unwrap().len(), 1);
  assert_eq!(receiver.notices.lock().unwrap()[0].params["notes"], json!(2));

  resume_digests(db).await.unwrap();

  assert!(!digest_paused(db).await.unwrap());
}

#[test]
fn a_changed_document_is_read_again_in_the_rooms_that_read_it() {
  let rooms = vec![
    RoomTitle {
      chat_id: -5,
      title: Some("Family".into()),
      senders: vec![7, 8],
    },
    RoomTitle {
      chat_id: -6,
      title: None,
      senders: vec![8],
    },
  ];
  let scopes = |path: &str| match path {
    "knowledge/tea.md" => Some(Some(-6)),
    "knowledge/rules.md" => Some(None),
    _ => None,
  };
  let reading = |path: &str| rooms_reading(path, &rooms, &scopes);

  assert_eq!(reading("rooms/-5/people/chunks/p7-address.md"), vec![-5]);
  assert_eq!(reading("rooms/-5/rules/quiet.md"), vec![-5]);
  assert_eq!(reading("people/8/chunks/c8-speech.md"), vec![-5, -6]);
  assert_eq!(reading("people/7/sheet.md"), vec![-5]);
  assert!(reading("people/7/index.md").is_empty());
  assert_eq!(reading("knowledge/tea.md"), vec![-6]);
  assert_eq!(reading("knowledge/rules.md"), vec![-5, -6]);
  assert!(reading("knowledge/candidates.md").is_empty());
  assert!(reading("knowledge/index.md").is_empty());
}
