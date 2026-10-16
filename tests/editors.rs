//! One editor run at its desk on the agency: the session it asks for, the cap its work key gives it, the clock
//! kept on it, and what it came to.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bmo_brain::editors::editor::{
  Answer, EditorAsk, Limit, RunDesk, TOOLS, answer_at_desk, editor_cap_seconds,
};
use bmo_brain::editors::runs::Step;
use bmo_brain::ports::agency::{Outcome, Overrun, SessionEnd, WorkKey};
use fragment_desk::desk::FragmentWrite;
use serde_json::json;

use support::agency::{CAP_SECONDS, FakeAgency};
use support::{AGENT, OTHER_AGENT, ROOM};

const KEY: &str = "people-digest";

// A desk in a folder of the test's own: the run stands in the agent's folder under it, and closing it answers the
// writes the test laid on it.
struct TestDesk {
  root: tempfile::TempDir,
  opens: bool,
  writes: Vec<FragmentWrite>,
  closed: Mutex<Vec<PathBuf>>,
}

impl TestDesk {
  fn new() -> Self {
    Self {
      root: tempfile::tempdir().unwrap(),
      opens: true,
      writes: vec![],
      closed: Mutex::new(vec![]),
    }
  }

  fn path(&self) -> PathBuf {
    self.root.path().join("desk.json")
  }

  fn folder(&self) -> PathBuf {
    self.root.path().join("brain").join(AGENT)
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

fn ended(outcome: Outcome, answer: Option<&str>) -> SessionEnd {
  SessionEnd {
    outcome,
    answer: answer.map(str::to_owned),
    files_written: vec![],
    cost: None,
    size: None,
  }
}

fn ask<'a>(desk: &'a TestDesk, unit: Option<&'a str>, step: Option<Step>) -> EditorAsk<'a> {
  EditorAsk {
    key: KEY,
    agent: AGENT,
    chat_id: ROOM,
    prompt: "Fold the batch into the book.",
    unit,
    cap_seconds: 1800,
    step,
    desk,
    registration: None,
  }
}

fn unix_now() -> i64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

#[tokio::test]
async fn an_editors_session_names_its_batch_and_its_end_says_what_it_carried() {
  let agency = FakeAgency::default();
  let mut desk = TestDesk::new();
  desk.writes = vec![write("rooms/-100/people/chunks/p1-a.md")];
  agency.ends.lock().unwrap().push_back(SessionEnd {
    outcome: Outcome::Answered,
    answer: Some("All folded in. DONE".to_owned()),
    files_written: vec!["rooms/-100/people/chunks/p1-a.md".to_owned(), "/elsewhere/notes.md".to_owned()],
    cost: Some(0.25),
    size: Some(128_400),
  });

  let step = Step { step: 2, steps: 5 };
  let answered = answer_at_desk(&agency, ask(&desk, Some("people-sort-agent-a"), Some(step)))
    .await
    .unwrap();

  let brain = desk.folder();
  assert_eq!(
    agency.calls.of("start_session"),
    [json!({
      "workKey": KEY, "agent": AGENT, "chatId": ROOM, "unit": "people-sort-agent-a", "capSeconds": 1800,
      "step": 2, "steps": 5, "prompt": "Fold the batch into the book.", "tools": TOOLS,
      "workdir": brain.to_string_lossy(), "desk": desk.path().to_string_lossy(), "registration": null,
    })]
  );
  assert_eq!(agency.calls.of("wait_session"), [json!({ "run": "run-1" })]);
  assert!(agency.calls.of("ask_overrun").is_empty());
  assert!(brain.is_dir(), "the folder the run stands in is made before it starts");
  assert_eq!(
    answered,
    Answer::Answered {
      answer: "All folded in. DONE".to_owned(),
      run: "run-1".to_owned(),
      cost: Some(0.25),
      size: Some(128_400),
      writes: vec![write("rooms/-100/people/chunks/p1-a.md")],
      wrote: BTreeSet::from([
        brain.join("rooms/-100/people/chunks/p1-a.md"),
        PathBuf::from("/elsewhere/notes.md")
      ]),
    }
  );
  assert_eq!(*desk.closed.lock().unwrap(), [desk.path()]);
}

#[tokio::test]
async fn a_batch_running_in_no_unit_names_none_and_still_says_what_it_carried() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  agency.ends.lock().unwrap().push_back(SessionEnd {
    size: Some(9_000),
    ..ended(Outcome::Answered, Some("DONE"))
  });

  let answered = answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  let started = &agency.calls.of("start_session")[0];
  assert_eq!(
    (&started["unit"], &started["step"], &started["steps"]),
    (&json!(null), &json!(null), &json!(null))
  );
  let Answer::Answered { size, .. } = answered else {
    panic!("the run answered: {answered:?}")
  };
  assert_eq!(size, Some(9_000));
}

#[tokio::test]
async fn a_runs_cap_is_its_work_keys_for_the_agent_it_writes_for() {
  let agency = FakeAgency::default();
  agency.work_keys.lock().unwrap().insert(
    KEY.to_owned(),
    WorkKey {
      cap_seconds: 7200,
      reasoning: Some("high".to_owned()),
    },
  );

  assert_eq!(editor_cap_seconds(&agency, KEY, OTHER_AGENT).await.unwrap(), 7200);
  assert_eq!(editor_cap_seconds(&agency, "digest", AGENT).await.unwrap(), CAP_SECONDS);
  assert_eq!(
    agency.calls.of("work_key"),
    [
      json!({ "workKey": KEY, "agent": OTHER_AGENT }),
      json!({ "workKey": "digest", "agent": AGENT })
    ]
  );
}

#[tokio::test]
async fn a_named_registration_goes_with_the_session() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  let mut asked = ask(&desk, None, None);
  asked.registration = Some("registration-a");

  answer_at_desk(&agency, asked).await.unwrap();

  assert_eq!(agency.calls.of("start_session")[0]["registration"], json!("registration-a"));
}

#[tokio::test(start_paused = true)]
async fn a_session_past_its_cap_is_asked_about_then_stopped_and_fails_as_cut() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  *agency.runs_for.lock().unwrap() = Some(Duration::from_secs(5000));
  agency.overruns.lock().unwrap().push_back(Overrun {
    keep: true,
    until: Some(unix_now() + 600),
  });
  agency.overruns.lock().unwrap().push_back(Overrun {
    keep: true,
    until: None,
  });
  agency.ends.lock().unwrap().push_back(ended(Outcome::Stopped, None));

  let answered = answer_at_desk(&agency, ask(&desk, Some("people-sort-agent-a"), None)).await.unwrap();

  let asked = json!({ "run": "run-1", "unit": "people-sort-agent-a", "capSeconds": 1800 });
  assert_eq!(agency.calls.of("ask_overrun"), [asked.clone(), asked.clone(), asked]);
  assert_eq!(agency.calls.of("stop_session"), [json!({ "run": "run-1" })]);
  assert_eq!(
    answered,
    Answer::Failed("The model run went past its 30-minute limit and was stopped.".to_owned())
  );
  assert_eq!(desk.closed.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_batch_in_no_unit_names_its_run_when_it_asks_about_its_cap() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  *agency.runs_for.lock().unwrap() = Some(Duration::from_secs(2000));

  answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  assert_eq!(
    agency.calls.of("ask_overrun"),
    [json!({ "run": "run-1", "unit": "run-1", "capSeconds": 1800 })]
  );
  assert_eq!(agency.calls.of("stop_session").len(), 1);
}

#[tokio::test]
async fn a_usage_limit_stops_the_run_and_the_desk_is_still_closed() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  agency.ends.lock().unwrap().push_back(ended(Outcome::Limit, None));

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
async fn a_session_that_left_no_answer_fails_naming_where_its_record_is() {
  let agency = FakeAgency::default();
  let desk = TestDesk::new();
  let ends = [
    ended(Outcome::Answered, None),
    ended(Outcome::Failed, Some("half a thought")),
    ended(Outcome::Stopped, None),
    ended(Outcome::Overran, None),
  ];
  agency.ends.lock().unwrap().extend(ends);

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
      "The model run went past its 30-minute limit and was stopped.".to_owned(),
    ]
  );
}

#[tokio::test]
async fn a_desk_that_cannot_open_fails_the_run_before_any_session() {
  let agency = FakeAgency::default();
  let mut desk = TestDesk::new();
  desk.opens = false;

  let answered = answer_at_desk(&agency, ask(&desk, None, None)).await.unwrap();

  assert_eq!(
    answered,
    Answer::Failed("The run's fragment desk could not be opened: no room for desks.".to_owned())
  );
  assert!(agency.calls.of("start_session").is_empty());
  assert!(desk.closed.lock().unwrap().is_empty());
}
