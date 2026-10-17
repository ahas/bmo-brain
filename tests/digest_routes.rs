//! The digests' routes, the brain holds, the chat-session summaries and the health route, served on a local port
//! (docs/boundary-api.md, 「The digests」, 「Brain holds」, 「Chat-session summaries」 and 「Health」): what is read or
//! set in a request answered there, and the work launched apart through the agency in memory, whose launches are
//! read back as the words the binary would be started with.

mod support;

use std::time::Duration;

use bmo_brain::api::digests::{PauseAsked, seoul_day_start};
use bmo_brain::api::holds::locks_free;
use bmo_brain::editors::lock::{brain_held, brain_holders};
use bmo_brain::ports::receiver::{RoadKind, Roads, RoomAnswerers};
use bmo_brain::schedules::apart::{Launch, LaunchOutcome};
use serde_json::{Value, json};

use support::served::{PROGRAM, Served, served};
use support::{AGENT, OTHER_AGENT, OWNER, ROOM};

impl Served {
  // A note waiting for the memo digest in a room.
  async fn pending_note(&self, chat_id: i64) {
    let answer = self
      .post(
        "/notes/add",
        json!({ "turn": { "agent": AGENT, "chatId": chat_id }, "text": "keep this" }),
      )
      .await;

    assert_eq!(answer["answered"], json!(true), "{answer}");
  }

  // The words each launch asked for would start the binary with, between the program's own and the unit the run
  // reads back as its own.
  fn launched_words(&self) -> Vec<Vec<String>> {
    self
      .starter
      .calls
      .of("start")
      .iter()
      .map(|launch| {
        let words: Vec<String> = launch["command"]
          .as_array()
          .unwrap()
          .iter()
          .map(|word| word.as_str().unwrap().to_owned())
          .collect();

        let unit = launch["unit"].as_str().unwrap();

        assert_eq!(words[..PROGRAM.len()], PROGRAM.map(str::to_owned));
        assert_eq!(words[words.len() - 2..], ["--unit".to_owned(), unit.to_owned()]);
        words[PROGRAM.len()..words.len() - 2].to_vec()
      })
      .collect()
  }

  fn answers(&self, chat_id: i64, agents: &[&str]) {
    self.receiver.answerers.lock().unwrap().push(RoomAnswerers {
      chat_id,
      agents: agents.iter().map(|agent| (*agent).to_owned()).collect(),
    });
  }

  // What a 400 said.
  async fn bad(&self, path: &str, body: Value) -> String {
    match self.caller.post::<Value>(path, &body).await {
      Err(bmo_brain::ports::http::PortFailure::Answered {
        status: 400,
        answer: Some(answer),
        ..
      }) => answer.error,
      other => panic!("no 400: {other:?}"),
    }
  }
}

fn words(all: &[&str]) -> Vec<String> {
  all.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn a_pause_is_read_as_a_switch_or_a_length() {
  assert_eq!(PauseAsked::read("status"), Some(PauseAsked::Status));
  assert_eq!(PauseAsked::read("on"), Some(PauseAsked::On));
  assert_eq!(PauseAsked::read("off"), Some(PauseAsked::Off));
  assert_eq!(PauseAsked::read("2h"), Some(PauseAsked::For(7_200)));
  assert_eq!(PauseAsked::read("90M"), Some(PauseAsked::For(5_400)));
  assert_eq!(PauseAsked::read("45"), Some(PauseAsked::For(2_700)));
  assert_eq!(PauseAsked::read("30 s"), Some(PauseAsked::For(30)));
  assert_eq!(PauseAsked::read("0h"), None);
  assert_eq!(PauseAsked::read("+5m"), None);
  assert_eq!(PauseAsked::read("2d"), None);
  assert_eq!(PauseAsked::read("soon"), None);
}

#[test]
fn a_korean_day_begins_at_its_midnight_in_seoul() {
  assert_eq!(seoul_day_start("2026-01-02"), Some(1_767_279_600));
  assert_eq!(seoul_day_start("2026-02-30"), None);
  assert_eq!(seoul_day_start("2026-1-2"), None);
  assert_eq!(seoul_day_start("tomorrow"), None);
}

#[tokio::test]
async fn the_pause_is_shown_set_for_a_while_set_until_lifted_and_lifted() {
  let api = served().await;
  let status = api.post("/digest/pause", json!({ "set": "status" })).await;

  assert_eq!(
    status,
    json!({ "pausedUntil": null, "answered": true, "lines": ["digests are running"] })
  );

  let forever = api.post("/digest/pause", json!({ "set": "on" })).await;

  assert_eq!(forever["pausedUntil"], json!("forever"));
  assert_eq!(forever["lines"], json!(["digests are paused until told otherwise"]));

  let timed = api.post("/digest/pause", json!({ "set": "2h" })).await;
  let until = timed["pausedUntil"].as_i64().unwrap();
  let now = bmo_brain::editors::fragments::store::now_seconds();

  assert!((now + 7_190..=now + 7_210).contains(&until), "{timed}");
  assert!(
    timed["lines"][0].as_str().unwrap().starts_with("digests are paused until 20"),
    "{timed}"
  );
  assert_eq!(api.post("/digest/pause", json!({ "set": "off" })).await["pausedUntil"], json!(null));
  assert!(
    api
      .bad("/digest/pause", json!({ "set": "soon" }))
      .await
      .contains("a bare number being minutes")
  );
}

#[tokio::test]
async fn the_memo_digest_starts_only_with_notes_waiting_and_never_while_paused_or_resting() {
  let api = served().await;

  assert_eq!(api.post("/digest/run", json!({})).await, json!({ "outcome": "nothing" }));

  api.pending_note(ROOM).await;
  api.post("/digest/pause", json!({ "set": "on" })).await;
  assert_eq!(api.post("/digest/run", json!({})).await, json!({ "outcome": "paused" }));
  // What the pause holds back is said to the owner once.
  assert_eq!(api.receiver.notices.lock().unwrap().len(), 1);
  api.post("/digest/run", json!({})).await;
  assert_eq!(api.receiver.notices.lock().unwrap().len(), 1);
  api.post("/digest/pause", json!({ "set": "off" })).await;

  *api.agency.rest.lock().unwrap() = Some(bmo_brain::ports::agency::Rest {
    resting: true,
    until: None,
    kind: Some("outage".to_owned()),
  });
  assert_eq!(api.post("/digest/run", json!({})).await, json!({ "outcome": "resting" }));
  assert_eq!(
    api.post("/digest/now", json!({ "agent": AGENT, "chatId": ROOM })).await,
    json!({ "outcome": "resting" })
  );
  *api.agency.rest.lock().unwrap() = None;
  assert!(api.launched_words().is_empty());

  assert_eq!(
    api.post("/digest/run", json!({})).await,
    json!({ "outcome": "launched", "unit": "bmo-digest" })
  );
  assert_eq!(
    api.post("/digest/run", json!({ "chatId": ROOM })).await,
    json!({ "outcome": "launched", "unit": format!("bmo-digest-{ROOM}") })
  );
  assert_eq!(
    api.post("/digest/now", json!({ "agent": AGENT, "chatId": ROOM })).await,
    json!({ "outcome": "running", "unit": format!("bmo-digest-{ROOM}") })
  );
  assert_eq!(
    api.post("/digest/now", json!({ "agent": AGENT, "chatId": -999 })).await,
    json!({ "outcome": "nothing" })
  );
  assert_eq!(
    api.launched_words(),
    [
      words(&["digest"]),
      words(&["digest", "--chat", &ROOM.to_string()]),
      words(&["digest", "--chat", &ROOM.to_string()]),
    ]
  );

  // A launch the agency refuses says why.
  *api.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Refused,
    why: Some("no room for another unit".to_owned()),
  });
  assert_eq!(
    api.post("/digest/run", json!({})).await,
    json!({ "outcome": "refused", "why": "no room for another unit" })
  );
}

#[tokio::test]
async fn the_person_book_is_refused_where_it_is_not_written_and_launched_in_the_unit_its_form_runs_under() {
  let api = served().await;

  // An agent keeping no long-term memory has no book.
  let none = api.post("/digest/people", json!({ "agent": "agent-z", "chatId": ROOM })).await;

  assert_eq!(none["refusal"]["code"], json!("no-memory"), "{none}");

  let unanswered = api.post("/digest/people", json!({ "chatId": ROOM })).await;

  assert_eq!(unanswered["refusal"]["code"], json!("not-answered"), "{unanswered}");

  api.answers(ROOM, &[AGENT]);
  api.answers(OWNER, &[AGENT]);
  api.receiver.roads.lock().unwrap().insert(
    OWNER,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );
  assert_eq!(
    api.post("/digest/people", json!({ "chatId": OWNER })).await["refusal"]["code"],
    json!("one-to-one")
  );

  // Forms that do not go together are the wire's 400.
  assert!(
    api
      .bad("/digest/people", json!({ "chatId": ROOM, "reform": true }))
      .await
      .contains("batches")
  );
  assert!(
    api
      .bad("/digest/people", json!({ "sort": true }))
      .await
      .contains("a person, all or status")
  );
  assert!(
    api
      .bad("/digest/people", json!({ "sheets": true }))
      .await
      .contains("asked for a chatId")
  );
  assert!(
    api
      .bad("/digest/people", json!({ "forgetRead": true }))
      .await
      .contains("all rooms with all")
  );
  assert!(
    api
      .bad("/digest/people", json!({ "since": "2026-02-30", "chatId": ROOM }))
      .await
      .contains("Not a day")
  );

  // The day a room's book reads from is set and read in the request.
  let since = api.post("/digest/people", json!({ "chatId": ROOM, "since": "2026-01-02" })).await;

  assert_eq!(since["outcome"], json!("answered"));
  assert!(since["lines"][0].as_str().unwrap().contains("reads from 2026-01-02"), "{since}");
  assert!(
    api.post("/digest/people", json!({ "chatId": ROOM, "since": null })).await["lines"][0]
      .as_str()
      .unwrap()
      .contains("reads from 2026-01-02")
  );
  assert!(
    api.post("/digest/people", json!({ "chatId": ROOM, "since": "none" })).await["lines"][0]
      .as_str()
      .unwrap()
      .contains("reads from its first message")
  );

  let status = api.post("/digest/people", json!({ "sort": true, "status": true })).await;

  assert_eq!(status["outcome"], json!("answered"), "{status}");
  assert_eq!(status["lines"].as_array().unwrap().last(), Some(&json!("people left to sort: none")));
  assert!(api.launched_words().is_empty());

  // The work itself is launched apart: a room's in the agent's worker of it, the sort in its own unit.
  let room = api.post("/digest/people", json!({ "chatId": ROOM, "all": true })).await;

  assert_eq!(room["outcome"], json!("launched"), "{room}");

  let sort = api.post("/digest/people", json!({ "sort": true, "person": 7788, "groups": 2 })).await;

  assert_eq!(sort["outcome"], json!("launched"), "{sort}");
  assert_ne!(room["unit"], sort["unit"]);

  let launcher = api.post("/digest/people", json!({})).await;

  assert_eq!(launcher["outcome"], json!("launched"), "{launcher}");
  assert_eq!(
    api.launched_words(),
    [
      words(&["people-digest", "--chat", &ROOM.to_string(), "--agent", AGENT, "--all"]),
      words(&["people-digest", "--sort", "--person", "7788", "--agent", AGENT, "--groups", "2"]),
      words(&["people-digest"]),
    ]
  );
}

#[tokio::test]
async fn a_rooms_worker_is_launched_before_its_session_folds_where_the_book_is_written_from_it() {
  let api = served().await;

  api.answers(ROOM, &[AGENT]);
  api.receiver.roads.lock().unwrap().insert(
    OWNER,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );

  let launched = api.post("/people/before-fold", json!({ "chatId": ROOM, "agent": AGENT })).await;

  assert_eq!(launched["outcome"], json!("launched"), "{launched}");
  assert_eq!(
    api.post("/people/before-fold", json!({ "chatId": ROOM, "agent": AGENT })).await,
    json!({ "outcome": "running", "unit": launched["unit"] })
  );
  assert_eq!(
    api.post("/people/before-fold", json!({ "chatId": OWNER, "agent": AGENT })).await,
    json!({ "outcome": "skipped" })
  );
  assert_eq!(
    api.post("/people/before-fold", json!({ "chatId": ROOM, "agent": OTHER_AGENT })).await,
    json!({ "outcome": "skipped" })
  );
  assert_eq!(
    api.launched_words(),
    [
      words(&["people-digest", "--chat", &ROOM.to_string(), "--agent", AGENT]),
      words(&["people-digest", "--chat", &ROOM.to_string(), "--agent", AGENT]),
    ]
  );
}

#[tokio::test]
async fn a_jobs_hold_stands_while_its_run_does_and_the_wait_answers_once_the_locks_are_free() {
  let api = served().await;

  assert_eq!(
    api.post("/brain/hold", json!({ "run": "job-7" })).await,
    json!({ "held": true, "taken": [] })
  );
  // A second hold for the same run changes nothing.
  api.post("/brain/hold", json!({ "run": "job-7" })).await;
  api.agency.alive.lock().unwrap().insert("job-7".to_owned());
  assert_eq!(api.get("/brain/hold").await, (200, json!({ "holders": ["job-7"], "taken": [] })));

  // A hold whose run is over is cleared as it is read.
  api.agency.alive.lock().unwrap().clear();
  assert_eq!(api.get("/brain/hold").await.1["holders"], json!([]));
  assert!(api.bad("/brain/hold", json!({ "run": " " })).await.contains("a job's run"));

  assert_eq!(
    api.post("/brain/wait", json!({})).await,
    json!({ "free": true, "answered": true, "lines": ["every editing lock is free; the brain may be written"] })
  );

  // An editor's lock taken holds the wait until it is let go, or until the wait gives up.
  let lock = std::fs::File::create(api.data.path().join("people-digest.lock")).unwrap();

  lock.lock().unwrap();
  assert_eq!(api.get("/brain/hold").await.1["taken"], json!(["people-digest.lock"]));
  assert!(!locks_free(api.data.path(), Duration::from_millis(30), Duration::from_millis(10)).await);

  let data = api.data.path().to_path_buf();
  let waiting =
    tokio::spawn(async move { locks_free(&data, Duration::from_secs(5), Duration::from_millis(10)).await });

  tokio::time::sleep(Duration::from_millis(50)).await;
  lock.unlock().unwrap();
  assert!(waiting.await.unwrap());
}

#[tokio::test]
async fn a_jobs_hold_on_the_brain_holds_the_editors_off_while_its_run_is_open_but_never_its_own_run() {
  let api = served().await;

  api.post("/brain/hold", json!({ "run": "job-1" })).await;
  api.post("/brain/hold", json!({ "run": "job-2" })).await;
  api.agency.alive.lock().unwrap().insert("job-1".to_owned());

  assert_eq!(brain_holders(api.scratch.db(), api.agency.as_ref(), None).await.unwrap(), ["job-1"]);
  // The hold of the run that is over went as it was read.
  assert_eq!(
    api
      .scratch
      .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" LIKE 'brain_hold:%'"#, vec![])
      .await,
    1
  );
  assert!(brain_held(api.scratch.db(), api.agency.as_ref(), None).await);
  assert!(!brain_held(api.scratch.db(), api.agency.as_ref(), Some("job-1")).await);

  // Where nobody can say whether a job's run is alive, its hold stands.
  api.post("/brain/hold", json!({ "run": "job-3" })).await;
  *api.agency.unsure.lock().unwrap() = true;
  assert_eq!(
    brain_holders(api.scratch.db(), api.agency.as_ref(), None).await.unwrap(),
    ["job-1", "job-3"]
  );
}

#[tokio::test]
async fn a_session_summary_is_kept_one_per_agent_read_and_cleared() {
  let api = served().await;

  assert_eq!(
    api.post("/session-summary/read", json!({ "agent": AGENT })).await,
    json!({ "note": null })
  );
  api.post("/session-summary/keep", json!({ "agent": AGENT, "note": "first" })).await;
  assert_eq!(
    api.post("/session-summary/keep", json!({ "agent": AGENT, "note": "second" })).await,
    json!({ "ok": true })
  );
  api
    .post("/session-summary/keep", json!({ "agent": OTHER_AGENT, "note": "the other's" }))
    .await;
  assert_eq!(
    api.post("/session-summary/read", json!({ "agent": AGENT })).await,
    json!({ "note": "second" })
  );
  assert_eq!(
    api.post("/session-summary/clear", json!({ "agent": AGENT })).await,
    json!({ "ok": true })
  );
  assert_eq!(
    api.post("/session-summary/read", json!({ "agent": AGENT })).await,
    json!({ "note": null })
  );
  assert_eq!(
    api.post("/session-summary/read", json!({ "agent": OTHER_AGENT })).await,
    json!({ "note": "the other's" })
  );
  assert!(
    api
      .bad("/session-summary/keep", json!({ "agent": "", "note": "x" }))
      .await
      .contains("registration name")
  );
}

#[tokio::test]
async fn the_service_is_ready_while_its_database_answers_and_its_brain_is_a_repository() {
  let api = served().await;

  assert_eq!(api.get("/health").await, (200, json!({ "ready": true, "reason": null })));

  std::fs::remove_dir_all(api.root().join(".git")).unwrap();
  assert_eq!(
    api.get("/health").await,
    (200, json!({ "ready": false, "reason": "The brain folder is no git repository." }))
  );
}
