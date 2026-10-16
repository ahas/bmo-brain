//! One room's person-book work, batch by batch: a failed batch tried again before the room stops, and what stops
//! it without another try.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use bmo_brain::Failure;
use bmo_brain::editors::people::catchup::{BATCH_ATTEMPTS, BatchOutcome, RoomWork, Thrown, work_room};
use bmo_brain::editors::runs::Step;

struct Limit;

#[derive(Default)]
struct Seen {
  runs: Vec<Vec<i64>>,
  steps: Vec<String>,
  settled: Vec<Vec<i64>>,
  reports: Vec<String>,
  fatal: usize,
  locks: usize,
  releases: usize,
}

struct Held(Rc<RefCell<Seen>>);

impl Drop for Held {
  fn drop(&mut self) {
    self.0.borrow_mut().releases += 1;
  }
}

enum Next {
  Outcome(BatchOutcome),
  Limit,
  Error(&'static str),
}

struct Room {
  messages: Vec<i64>,
  outcomes: VecDeque<Next>,
  seen: Rc<RefCell<Seen>>,
  catch_up: bool,
  batches_per_run: i64,
  backlog: Option<i64>,
  resting: bool,
  paused_from: Option<usize>,
  paused_looks: usize,
  settle_fails: bool,
  lockless: bool,
  next_from: Option<i64>,
}

fn room(outcomes: Vec<Next>) -> Room {
  Room {
    messages: (1..=9).collect(),
    outcomes: outcomes.into(),
    seen: Rc::default(),
    catch_up: true,
    batches_per_run: 10,
    backlog: None,
    resting: false,
    paused_from: None,
    paused_looks: 0,
    settle_fails: false,
    lockless: false,
    next_from: None,
  }
}

fn missing_marker() -> Next {
  Next::Outcome(BatchOutcome::Failed(
    "agent did not report success: summary without the marker".to_owned(),
  ))
}

impl RoomWork for Room {
  type Batch = i64;
  type Held = Held;
  type Fatal = Limit;

  fn chat_id(&self) -> i64 {
    7
  }

  fn catch_up(&self) -> bool {
    self.catch_up
  }

  fn batches_per_run(&self) -> i64 {
    self.batches_per_run
  }

  fn batch_size(&self) -> i64 {
    3
  }

  fn record(&self) -> String {
    "the unit's journal".to_owned()
  }

  fn retry_delay(&self) -> Duration {
    Duration::ZERO
  }

  async fn backlog(&mut self) -> Result<i64, Failure> {
    Ok(self.backlog.unwrap_or(self.messages.len() as i64))
  }

  async fn resting(&mut self) -> Result<bool, Failure> {
    Ok(self.resting)
  }

  async fn paused(&mut self) -> Result<bool, Failure> {
    self.paused_looks += 1;

    Ok(self.paused_from.is_some_and(|from| self.paused_looks > from))
  }

  async fn next_batch(&mut self) -> Result<Vec<i64>, Failure> {
    Ok(match self.next_from {
      Some(next) => vec![next, next + 1, next + 2],
      None => self.messages.iter().take(3).copied().collect(),
    })
  }

  async fn lock(&mut self) -> Result<Option<Held>, Failure> {
    if self.lockless {
      return Ok(None);
    }

    self.seen.borrow_mut().locks += 1;

    Ok(Some(Held(Rc::clone(&self.seen))))
  }

  async fn run(&mut self, batch: &[i64], step: Step) -> Result<BatchOutcome, Thrown<Limit>> {
    self.seen.borrow_mut().runs.push(batch.to_vec());
    self.seen.borrow_mut().steps.push(format!("{}/{}", step.step, step.steps));

    match self.outcomes.pop_front().unwrap_or(Next::Outcome(BatchOutcome::Stood)) {
      Next::Outcome(outcome) => Ok(outcome),
      Next::Limit => Err(Thrown::Fatal(Limit)),
      Next::Error(why) => Err(Thrown::Unexpected(why.to_owned())),
    }
  }

  async fn settle(&mut self, batch: &[i64]) -> Result<(), String> {
    if self.settle_fails {
      return Err("commit failed".to_owned());
    }

    self.seen.borrow_mut().settled.push(batch.to_vec());

    match &mut self.next_from {
      Some(next) => *next += 3,
      None => {
        self.messages.drain(..batch.len());
      }
    }

    Ok(())
  }

  async fn report(&mut self, _batch: &[i64], reason: &str) {
    self.seen.borrow_mut().reports.push(reason.to_owned());
  }

  async fn on_fatal(&mut self, _batch: &[i64], _fatal: Limit) {
    self.seen.borrow_mut().fatal += 1;
  }

  fn log(&mut self, _line: &str) {}
}

fn firsts(batches: &[Vec<i64>]) -> Vec<i64> {
  batches.iter().map(|batch| batch[0]).collect()
}

#[tokio::test]
async fn a_failed_batch_is_run_again_and_the_catch_up_goes_on_to_the_end() {
  let mut work = room(vec![missing_marker(), Next::Error("timed out")]);

  assert!(!work_room(&mut work).await.unwrap());

  let seen = work.seen.borrow();

  assert_eq!(firsts(&seen.runs), vec![1, 1, 1, 4, 7]);
  assert_eq!(firsts(&seen.settled), vec![1, 4, 7]);
  assert!(seen.reports.is_empty());
  assert_eq!(seen.releases, seen.locks);
}

#[tokio::test]
async fn the_room_stops_and_the_owner_is_told_once_only_after_the_last_try_fails() {
  let mut work = room((0..BATCH_ATTEMPTS).map(|_| missing_marker()).collect());

  assert!(work_room(&mut work).await.unwrap());

  let seen = work.seen.borrow();

  assert_eq!(seen.runs.len(), BATCH_ATTEMPTS);
  assert!(seen.settled.is_empty());
  assert_eq!(seen.reports.len(), 1);
  assert!(seen.reports[0].contains(&format!("{BATCH_ATTEMPTS} tries")));
  assert!(seen.reports[0].contains("did not report success"));
  assert_eq!(seen.releases, seen.locks);
}

#[tokio::test]
async fn tries_are_counted_per_batch_so_a_failure_after_a_success_starts_from_one_again() {
  let mut work = room(vec![
    missing_marker(),
    missing_marker(),
    Next::Outcome(BatchOutcome::Stood),
    missing_marker(),
    missing_marker(),
  ]);

  assert!(!work_room(&mut work).await.unwrap());
  assert_eq!(firsts(&work.seen.borrow().settled), vec![1, 4, 7]);
  assert!(work.seen.borrow().reports.is_empty());
}

#[tokio::test]
async fn no_try_is_made_again_past_a_usage_limit() {
  let mut work = room(vec![Next::Limit]);

  assert!(work_room(&mut work).await.unwrap());

  let seen = work.seen.borrow();

  assert_eq!(seen.runs.len(), 1);
  assert_eq!(seen.fatal, 1);
  assert_eq!(seen.releases, 1);
}

#[tokio::test]
async fn no_try_is_made_again_once_a_rest_or_the_owners_pause_has_begun() {
  let mut resting = room(vec![missing_marker()]);

  resting.resting = true;

  let mut paused = room(vec![missing_marker()]);

  paused.paused_from = Some(1);

  assert!(!work_room(&mut resting).await.unwrap());
  assert_eq!(resting.seen.borrow().runs.len(), 1);
  assert!(!work_room(&mut paused).await.unwrap());
  assert_eq!(paused.seen.borrow().runs.len(), 1);
  assert!(resting.seen.borrow().reports.is_empty());
}

#[tokio::test]
async fn a_marked_batch_is_not_run_again_when_what_follows_the_marking_fails() {
  let mut work = room(vec![]);

  work.settle_fails = true;

  assert!(work_room(&mut work).await.unwrap());

  let seen = work.seen.borrow();

  assert_eq!(seen.runs.len(), 1);
  assert_eq!(seen.reports.len(), 1);
  assert!(seen.reports[0].contains("putting it in place afterwards failed"));
  assert!(seen.reports[0].contains("the unit's journal"));
  assert!(!seen.reports[0].contains("commit failed"));
}

#[tokio::test]
async fn a_bounded_run_takes_at_most_its_batches_tries_apart() {
  let mut work = room(vec![missing_marker()]);

  work.catch_up = false;
  work.batches_per_run = 2;

  assert!(!work_room(&mut work).await.unwrap());
  assert_eq!(firsts(&work.seen.borrow().settled), vec![1, 4]);
  assert_eq!(work.seen.borrow().runs.len(), 3);
}

#[tokio::test]
async fn the_backlog_it_started_with_is_worked_and_what_arrived_meanwhile_left_for_the_next_run() {
  let mut work = room(vec![]);

  work.catch_up = false;
  work.backlog = Some(7);
  work.next_from = Some(100);

  assert!(!work_room(&mut work).await.unwrap());
  assert_eq!(work.seen.borrow().steps, vec!["1/3", "2/3", "3/3"]);
}

#[tokio::test]
async fn the_messages_are_left_for_the_next_run_when_the_lock_cannot_be_had() {
  let mut work = room(vec![]);

  work.lockless = true;

  assert!(!work_room(&mut work).await.unwrap());
  assert!(work.seen.borrow().runs.is_empty());
}
