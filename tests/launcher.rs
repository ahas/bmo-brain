//! The person-book launcher on the fake agency and receiver: every agent's rooms with enough waiting given a worker
//! in turn, the smallest backlog first and the brain root's agent first, a backlog caught up, a worker already
//! standing left to itself, a refusal counted and gone past — and the wait on a worker, on time a test moves,
//! following the batch it is at and the keep the agency gave it.

mod support;

use std::time::Duration;

use bmo_brain::editors::people::launcher::{LAUNCHER_SLACK_MS, Launcher, wall_clock};
use bmo_brain::ports::agency::{Registration, RunClock};
use bmo_brain::ports::receiver::{Message, RoomAnswerers};
use bmo_brain::schedules::apart::{Launch, LaunchOutcome};
use bmo_brain::schedules::launches::Launches;
use serde_json::Value;

use support::agency::registration;
use support::editing::Stage;
use support::receiver::message;
use support::{AGENT, OTHER_AGENT};

const SMALL: i64 = -100;
const MIDDLE: i64 = -200;
const QUIET: i64 = -300;
const BACKLOG: i64 = -400;

fn said(chat_id: i64, count: i64) -> Vec<Message> {
  (1..=count).map(|id| message(chat_id, chat_id * 1_000 - id, 7, id)).collect()
}

// A stage whose rooms hold 12, 30, 5 and 450 unread messages, every room answered by the agents given.
async fn rooms(agents: &[&str]) -> Stage {
  let stage = Stage::new().await;
  let mut messages = said(SMALL, 12);

  messages.extend(said(MIDDLE, 30));
  messages.extend(said(QUIET, 5));
  messages.extend(said(BACKLOG, 450));
  *stage.receiver.messages.lock().unwrap() = messages;
  *stage.receiver.answerers.lock().unwrap() = [SMALL, MIDDLE, QUIET, BACKLOG]
    .iter()
    .map(|chat_id| RoomAnswerers {
      chat_id: *chat_id,
      agents: agents.iter().map(|agent| (*agent).to_owned()).collect(),
    })
    .collect();

  stage
}

// Every launch answered as started, the worker gone by the time the launcher looks.
fn workers_end_at_once(stage: &Stage) {
  *stage.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Launched,
    why: None,
  });
}

fn launched(stage: &Stage) -> Vec<Value> {
  stage.starter.calls.of("start")
}

fn units(stage: &Stage) -> Vec<String> {
  launched(stage).iter().map(|call| call["unit"].as_str().unwrap().to_owned()).collect()
}

async fn launch(stage: &Stage) -> bool {
  let program = vec!["brain".to_owned()];
  let launches = Launches {
    ground: stage.ground(),
    program: &program,
  };

  Launcher {
    launches: &launches,
    clock: &wall_clock,
  }
  .launch_workers()
  .await
  .unwrap()
}

#[tokio::test]
async fn the_rooms_with_enough_waiting_are_worked_smallest_backlog_first_and_a_backlog_caught_up() {
  let stage = rooms(&[AGENT]).await;

  workers_end_at_once(&stage);

  assert!(!launch(&stage).await);
  assert_eq!(
    units(&stage),
    [
      "bmo-people-digest-agent-a--100",
      "bmo-people-digest-agent-a--200",
      "bmo-people-digest-agent-a--400"
    ]
  );

  let calls = launched(&stage);
  let words = |at: usize| -> Vec<String> {
    calls[at]["command"]
      .as_array()
      .unwrap()
      .iter()
      .map(|word| word.as_str().unwrap().to_owned())
      .collect()
  };

  assert!(!words(0).contains(&"--all".to_owned()));
  assert!(words(2).contains(&"--all".to_owned()));
}

#[tokio::test]
async fn a_worker_already_standing_is_left_to_itself_and_the_next_room_goes_on() {
  let stage = rooms(&[AGENT]).await;

  workers_end_at_once(&stage);
  stage.agency.alive.lock().unwrap().insert("bmo-people-digest-agent-a--200".to_owned());

  assert!(!launch(&stage).await);
  assert_eq!(units(&stage), ["bmo-people-digest-agent-a--100", "bmo-people-digest-agent-a--400"]);
}

#[tokio::test]
async fn a_refused_worker_counts_as_a_failure_and_every_room_is_still_tried() {
  let stage = rooms(&[AGENT]).await;

  *stage.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Refused,
    why: Some("No host takes it.".to_owned()),
  });

  assert!(launch(&stage).await);
  assert_eq!(units(&stage).len(), 3);
}

#[tokio::test]
async fn every_agents_rooms_are_gone_through_in_turn_the_brain_roots_agent_first() {
  let stage = rooms(&[OTHER_AGENT, AGENT]).await;

  *stage.agency.registrations.lock().unwrap() = vec![
    Registration {
      brain_folder: Some("agents/agent-b".to_owned()),
      ..registration(OTHER_AGENT)
    },
    Registration {
      brain_folder: Some(String::new()),
      ..registration(AGENT)
    },
  ];
  workers_end_at_once(&stage);

  assert!(!launch(&stage).await);

  let order: Vec<String> = units(&stage);

  assert_eq!(order.len(), 6);
  assert!(order[..3].iter().all(|unit| unit.starts_with("bmo-people-digest-agent-a-")));
  assert!(order[3..].iter().all(|unit| unit.starts_with("bmo-people-digest-agent-b-")));
}

const WORKER: &str = "bmo-people-digest-agent-a--100";
const BASE_MS: i64 = 1_700_000_000_000;

// The wait on a worker that stands from the start, on time the test moves once its stages are laid: whether it
// ended in time, and how long the launcher waited, in seconds.
async fn waited(stage: &Stage, ends_after: Option<Duration>) -> (bool, u64) {
  let program = vec!["brain".to_owned()];
  let launches = Launches {
    ground: stage.ground(),
    program: &program,
  };
  let start = tokio::time::Instant::now();
  let clock = move || BASE_MS + i64::try_from(start.elapsed().as_millis()).unwrap();
  let launcher = Launcher {
    launches: &launches,
    clock: &clock,
  };

  stage.agency.alive.lock().unwrap().insert(WORKER.to_owned());

  let ending = async {
    if let Some(after) = ends_after {
      tokio::time::sleep(after).await;
      stage.agency.alive.lock().unwrap().remove(WORKER);
    }
  };
  let (ended, ()) = tokio::join!(launcher.wait_for_kept(WORKER), ending);

  (ended.unwrap(), start.elapsed().as_secs())
}

#[tokio::test]
async fn a_worker_that_ends_within_its_cap_is_waited_for_until_it_ends() {
  let stage = Stage::new().await;

  tokio::time::pause();

  let (ended, seconds) = waited(&stage, Some(Duration::from_secs(100))).await;

  assert!(ended);
  assert!((100..=102).contains(&seconds), "{seconds}");
}

#[tokio::test]
async fn a_worker_nobody_kept_is_left_be_one_cap_and_a_minute_after_it_started() {
  let stage = Stage::new().await;

  tokio::time::pause();

  let (ended, seconds) = waited(&stage, None).await;

  assert!(!ended);
  assert_eq!(seconds, 1_800 + 60);
}

#[tokio::test]
async fn the_wait_follows_the_batch_a_worker_is_at_and_the_keep_the_agency_gave_it() {
  let kept = Stage::new().await;
  let later = Stage::new().await;
  let base = BASE_MS / 1000;

  kept.agency.clocks.lock().unwrap().insert(
    WORKER.to_owned(),
    RunClock {
      cap_ends: Some(base + 2_000),
      kept_until: Some(base + 2_500),
    },
  );
  later.agency.clocks.lock().unwrap().insert(
    WORKER.to_owned(),
    RunClock {
      cap_ends: Some(base + 3_000),
      kept_until: None,
    },
  );
  tokio::time::pause();

  let (ended, seconds) = waited(&kept, None).await;

  assert!(!ended);
  assert_eq!(seconds, 2_500 + u64::try_from(LAUNCHER_SLACK_MS / 1000).unwrap());

  // A later batch the agency reports carries the wait on with it, and a worker that then ends is waited for.
  let (ended, seconds) = waited(&later, Some(Duration::from_secs(2_900))).await;

  assert!(ended);
  assert!((2_900..=2_902).contains(&seconds), "{seconds}");
}
