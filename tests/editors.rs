//! One editor run at its desk on the agency: the turn of its key's editor session it asks for, the cap the agency
//! gives it, the brain's watch on it past that cap, and what it came to, the files it wrote read off the brain.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bmo_brain::editors::editor::{Answer, EditorAsk, Limit, RunDesk, answer_at_desk, editor_cap_seconds};
use bmo_brain::editors::runs::Step;
use bmo_brain::ports::agency::{EditorTurn, HttpAgency, Registration, RunClock, TurnOutcome};
use fragment_desk::desk::FragmentWrite;
use serde_json::json;

use support::agency::{CAP_SECONDS, FakeAgency, Session, registration};
use support::editing::repository;
use support::stub::{ASKED_RUN, Stub};
use support::{AGENT, OTHER_AGENT, ROOM};

const KEY: &str = "people-digest";
const SERVICE: &str = "brain-service";

// A desk in a folder of the test's own beside a brain that is a git repository: the run stands in the agent's folder
// in that brain, and closing the desk answers the writes the test laid on it.
struct TestDesk {
  root: tempfile::TempDir,
  brain_root: PathBuf,
  folder: &'static str,
  opens: bool,
  writes: Vec<FragmentWrite>,
  closed: Mutex<Vec<PathBuf>>,
}

impl TestDesk {
  fn new() -> Self {
    Self::in_folder(AGENT)
  }

  // A desk whose run stands in this folder of the brain, empty for the brain root.
  fn in_folder(folder: &'static str) -> Self {
    let root = tempfile::tempdir().unwrap();

    std::fs::create_dir_all(root.path().join("brain")).unwrap();
    repository(&root.path().join("brain"));

    Self {
      brain_root: root.path().join("brain"),
      root,
      folder,
      opens: true,
      writes: vec![],
      closed: Mutex::new(vec![]),
    }
  }

  fn path(&self) -> PathBuf {
    self.root.path().join("desk.json")
  }

  fn brain(&self) -> PathBuf {
    self.brain_root.clone()
  }

  fn folder(&self) -> PathBuf {
    if self.folder.is_empty() { self.brain() } else { self.brain().join(self.folder) }
  }
}

impl RunDesk for TestDesk {
  fn open(&self) -> std::io::Result<PathBuf> {
    if !self.opens {
      return Err(std::io::Error::other("no room for desks"));
    }
    std::fs::write(self.path(), "{}")?;
    Ok(self.path())
  }

  fn brain(&self, desk: &Path) -> Option<PathBuf> {
    desk.exists().then(|| self.folder())
  }

  fn close(&self, desk: &Path) -> Vec<FragmentWrite> {
    let _ = std::fs::remove_file(desk);
    self.closed.lock().unwrap().push(desk.to_path_buf());
    self.writes.clone()
  }
}

fn write(path: &str) -> FragmentWrite {
  FragmentWrite {
    path: path.to_owned(),
    removed: false,
    title: "A piece".to_owned(),
    keywords: vec![],
    items: vec![],
    shared: None,
    all_agents: None,
  }
}

fn ask<'a>(desk: &'a TestDesk, unit: Option<&'a str>, step: Option<Step>) -> EditorAsk<'a> {
  EditorAsk {
    key: KEY,
    agent: AGENT,
    chat_id: ROOM,
    prompt: "Fold the batch into the book.",
    unit,
    service_unit: SERVICE,
    cap_seconds: 1800,
    step,
    desk,
    registration: None,
    brain_root: &desk.brain_root,
  }
}

// Every turn from now on writes these files, relative to the brain root, as the model's file tools would.
fn turns_write(agency: &FakeAgency, brain: PathBuf, files: &[&'static str]) {
  let files = files.to_vec();
  let session = move |turn: &EditorTurn| {
    for path in &files {
      let at = brain.join(&turn.workdir).join(path);

      std::fs::create_dir_all(at.parent().unwrap()).unwrap();
      std::fs::write(at, "written").unwrap();
    }
  };

  *agency.session.lock().unwrap() = Some(Session(Box::new(session)));
}

fn unix_now() -> i64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

#[tokio::test]
async fn an_editors_turn_names_its_batch_and_its_end_says_what_it_wrote() {
  let agency = FakeAgency::default();
  let mut desk = TestDesk::new();
  desk.writes = vec![write("rooms/-100/people/chunks/p1-a.md")];
  turns_write(&agency, desk.brain(), &["rooms/-100/people/chunks/p1-a.md"]);
  agency.turns_end(&[(TurnOutcome::Answered, Some("All folded in. DONE"))]);

  let step = Step { step: 2, steps: 5 };
  let answered = answer_at_desk(&agency, ask(&desk, Some("people-sort-agent-a"), Some(step)))
    .await
    .unwrap();

  let brain = desk.folder();
  assert_eq!(
    agency.calls.of("editor_session"),
    [json!({ "agent": KEY, "scope": { "kind": "editor" } })]
  );
  let turns = agency.calls.of("editor_turn");
  assert_eq!(turns.len(), 1);
  let run = turns[0]["run"].as_str().unwrap();
  assert!(run.starts_with("people-sort-agent-a:people-digest:agent-a:-100:"), "{run}");
  let mut sent = turns[0].clone();
  sent["run"] = json!("the run's name");
  assert_eq!(
    sent,
    json!({
      "session": "people-digest@editor", "prompt": "Fold the batch into the book.", "run": "the run's name",
      "parent": "people-sort-agent-a", "agent": AGENT, "chatId": ROOM, "step": 2, "steps": 5, "workdir": AGENT,
      "desk": desk.path().to_string_lossy(), "registration": null,
    })
  );
  assert!(agency.calls.of("run_clock").is_empty());
  assert!(brain.is_dir(), "the folder the run stands in is made before it starts");
  assert_eq!(
    answered,
    Answer::Answered {
      answer: "All folded in. DONE".to_owned(),
      run: "run-1".to_owned(),
      writes: vec![write("rooms/-100/people/chunks/p1-a.md")],
      wrote: BTreeSet::from([brain.join("rooms/-100/people/chunks/p1-a.md")]),
    }
  );
  assert_eq!(*desk.closed.lock().unwrap(), [desk.path()]);
}

#[tokio::test]
async fn a_batch_running_in_no_unit_is_named_by_the_services_unit_and_has_no_parent() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  agency.turns_end(&[(TurnOutcome::Answered, Some("DONE"))]);

  answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  let turn = &agency.calls.of("editor_turn")[0];
  assert_eq!(
    (&turn["parent"], &turn["step"], &turn["steps"]),
    (&json!(null), &json!(null), &json!(null))
  );
  assert!(turn["run"].as_str().unwrap().starts_with("brain-service:people-digest:agent-a:-100:"));
}

#[tokio::test]
async fn what_another_agents_folder_under_the_run_holds_is_not_the_runs() {
  let agency = FakeAgency::default();
  *agency.registrations.lock().unwrap() = vec![
    Registration {
      brain_folder: Some(String::new()),
      ..registration(AGENT)
    },
    Registration {
      brain_folder: Some("agents/agent-b".to_owned()),
      ..registration(OTHER_AGENT)
    },
  ];
  let desk = TestDesk::in_folder("");
  turns_write(&agency, desk.brain(), &["knowledge/a.md", "agents/agent-b/knowledge/b.md"]);
  agency.turns_end(&[(TurnOutcome::Answered, Some("DONE"))]);

  let answered = answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  assert_eq!(agency.calls.of("editor_turn")[0]["workdir"], json!(""));
  let Answer::Answered { wrote, .. } = answered else {
    panic!("the run answered: {answered:?}")
  };
  assert_eq!(wrote, BTreeSet::from([desk.brain().join("knowledge/a.md")]));
}

#[tokio::test]
async fn a_runs_cap_is_the_agencys_for_the_key_and_the_agent_it_writes_for() {
  let agency = FakeAgency::default();
  agency.caps.lock().unwrap().insert(KEY.to_owned(), 7200);

  assert_eq!(editor_cap_seconds(&agency, KEY, OTHER_AGENT).await.unwrap(), 7200);
  assert_eq!(editor_cap_seconds(&agency, "digest", AGENT).await.unwrap(), CAP_SECONDS);
  assert_eq!(
    agency.calls.of("editor_cap"),
    [
      json!({ "workKey": KEY, "agent": OTHER_AGENT }),
      json!({ "workKey": "digest", "agent": AGENT })
    ]
  );
}

#[tokio::test]
async fn a_named_registration_goes_with_the_turn() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  let mut asked = ask(&desk, None, None);
  asked.registration = Some("registration-a");

  answer_at_desk(&agency, asked).await.unwrap();

  assert_eq!(agency.calls.of("editor_turn")[0]["registration"], json!("registration-a"));
}

#[tokio::test(start_paused = true)]
async fn a_run_past_its_cap_is_watched_while_kept_then_stopped_and_fails_as_cut() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  *agency.runs_for.lock().unwrap() = Some(Duration::from_secs(5000));
  agency.clock_reads.lock().unwrap().push_back(RunClock {
    cap_ends: None,
    kept_until: Some(unix_now() + 600),
  });
  agency.turns_end(&[(TurnOutcome::Stopped, None)]);

  let answered = answer_at_desk(&agency, ask(&desk, Some("people-sort-agent-a"), None)).await.unwrap();

  let read = json!({ "name": "people-sort-agent-a" });
  assert_eq!(agency.calls.of("run_clock"), [read.clone(), read]);
  assert_eq!(
    agency.calls.of("stop_run"),
    [json!({ "run": "run-1", "reason": "The model run went past its 30-minute limit and was stopped." })]
  );
  assert_eq!(
    answered,
    Answer::Failed("The model run went past its 30-minute limit and was stopped.".to_owned())
  );
  assert_eq!(desk.closed.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_batch_in_no_unit_reads_its_runs_clock_past_its_cap() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  *agency.runs_for.lock().unwrap() = Some(Duration::from_secs(2000));

  answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  assert_eq!(agency.calls.of("run_clock"), [json!({ "name": "run-1" })]);
  assert_eq!(agency.calls.of("stop_run").len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_session_carrying_another_turn_is_asked_again_until_it_takes_this_one() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  *agency.busy.lock().unwrap() = 2;
  agency.turns_end(&[(TurnOutcome::Answered, Some("DONE"))]);
  let began = tokio::time::Instant::now();

  let answered = answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  assert!(matches!(answered, Answer::Answered { .. }), "{answered:?}");
  assert_eq!(agency.calls.of("editor_turn").len(), 3);
  assert_eq!(began.elapsed(), Duration::from_secs(10));
}

#[tokio::test]
async fn a_failed_turn_while_the_agency_rests_for_a_limit_stops_the_run_and_the_desk_is_still_closed() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  agency.limited();

  let limited = answer_at_desk(&agency, ask(&desk, None, None)).await;

  assert_eq!(
    limited,
    Err(Limit {
      run: "run-1".to_owned()
    })
  );
  assert_eq!(desk.closed.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_turn_that_left_no_answer_fails_naming_where_its_record_is() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  agency.turns_end(&[
    (TurnOutcome::Answered, None),
    (TurnOutcome::Failed, Some("half a thought")),
    (TurnOutcome::Stopped, None),
    (TurnOutcome::Blocked, None),
  ]);

  let mut failures = vec![];
  for _ in 0..4 {
    let Ok(Answer::Failed(why)) = answer_at_desk(&agency, ask(&desk, None, None)).await else {
      panic!("every one of these runs fails");
    };
    failures.push(why);
  }

  let unreadable = |run: &str| {
    format!(
      "The model run ended without a readable answer. The cause is not known from what the run left; \
       the whole of it is in the agency's record of run {run}."
    )
  };
  assert_eq!(
    failures,
    [
      unreadable("run-1"),
      unreadable("run-2"),
      "the run was stopped from outside while it was working".to_owned(),
      unreadable("run-4"),
    ]
  );
}

#[tokio::test]
async fn a_desk_that_cannot_open_fails_the_run_before_any_turn() {
  let agency = FakeAgency::default();
  let mut desk = TestDesk::new();
  desk.opens = false;

  let answered = answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  assert_eq!(
    answered,
    Answer::Failed("The run's fragment desk could not be opened: no room for desks.".to_owned())
  );
  assert!(agency.calls.of("editor_turn").is_empty());
  assert!(desk.closed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_editor_run_goes_over_the_agencys_routes_as_it_documents_them() {
  let stub = Stub::start().await;
  let desk = TestDesk::new();
  let session = json!({ "id": "people-digest@editor", "agent": KEY, "scope": { "kind": "editor" } });
  stub
    .answer("/session/start", session)
    .answer("/session/turn", json!({ "delivery": "taken" }))
    .answer("/agents", json!({ "agents": [{ "name": AGENT, "label": null }] }))
    .answer_events(
      "/events",
      "/session/turn",
      vec![
        ("turn-started", json!({ "session": "people-digest@editor", "run": ASKED_RUN, "at": 5 })),
        (
          "turn-ended",
          json!({ "session": "people-digest@editor", "run": ASKED_RUN, "outcome": "answered",
            "answer": "DONE" }),
        ),
      ],
    );
  std::fs::create_dir_all(desk.folder().join("knowledge")).unwrap();
  std::fs::write(desk.folder().join("knowledge/a.md"), "written").unwrap();

  let answered = answer_at_desk(&HttpAgency::new(&stub.peer), ask(&desk, None, None)).await.unwrap();
  let run = stub.body_at("/session/turn")["run"].as_str().unwrap().to_owned();

  assert_eq!(
    answered,
    Answer::Answered {
      answer: "DONE".to_owned(),
      run,
      writes: vec![],
      wrote: BTreeSet::from([desk.folder().join("knowledge/a.md")]),
    }
  );
  assert_eq!(stub.body_at("/session/start"), json!({ "agent": KEY, "scope": { "kind": "editor" } }));
  assert_eq!(stub.body_at("/session/turn")["workdir"], json!(AGENT));
  let paths: Vec<String> = stub.taken().into_iter().map(|taken| taken.path).collect();
  assert!(
    !paths.iter().any(|path| path.starts_with("/runs")),
    "a run that ended within its cap is not watched"
  );
}
