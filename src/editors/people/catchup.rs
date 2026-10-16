//! One room's person-book work, batch by batch (docs/people-book.md, 「A run plans its batches as it starts」 and 「A
//! failed batch is tried again before the room stops」). The batches are counted once, before the first, so a room
//! whose conversation keeps going is let go after the backlog it started with. A batch that fails is put back by
//! the one who ran it and handed to a fresh run, up to `BATCH_ATTEMPTS` runs, before the room stops and the owner is
//! told; a catch-up therefore survives one bad run. What stops the room without another try: a rest or the owner's
//! pause seen before the next run, a fatal error — a usage limit — and a failure after the batch was marked.

use std::future::Future;
use std::time::Duration;

use super::super::settle::unknown_cause;
use crate::Failure;
use crate::editors::runs::Step;

/// How many runs a failing batch is given before the room stops.
pub const BATCH_ATTEMPTS: usize = 3;

/// What one run over a batch came to.
pub enum BatchOutcome {
  Stood,
  Failed(String),
}

/// What stopped a run short of an outcome: an error that ends the whole process rather than one try, or one
/// nobody named a cause for, said in the log's words.
pub enum Thrown<F> {
  Fatal(F),
  Unexpected(String),
}

/// One room's work as the loop asks it.
pub trait RoomWork {
  type Batch;
  /// The editing lock, let go when dropped.
  type Held;
  type Fatal;

  fn chat_id(&self) -> i64;
  /// Catch-up works the whole backlog it started with; otherwise at most `batches_per_run` batches of it.
  fn catch_up(&self) -> bool;
  fn batches_per_run(&self) -> i64;
  /// The most messages one batch takes.
  fn batch_size(&self) -> i64;
  /// Where the whole account of an error nobody expected is.
  fn record(&self) -> String;
  fn retry_delay(&self) -> Duration;
  fn backlog(&mut self) -> impl Future<Output = Result<i64, Failure>>;
  fn resting(&mut self) -> impl Future<Output = Result<bool, Failure>>;
  fn paused(&mut self) -> impl Future<Output = Result<bool, Failure>>;
  /// The oldest unmarked messages, empty when none are left.
  fn next_batch(&mut self) -> impl Future<Output = Result<Vec<Self::Batch>, Failure>>;
  /// The editing lock, taken only while no job holds the brain; none when it could not be had.
  fn lock(&mut self) -> impl Future<Output = Result<Option<Self::Held>, Failure>>;
  /// One run over the batch, told which of the planned batches it is; what a failed run wrote is already put back.
  fn run(
    &mut self,
    batch: &[Self::Batch],
    step: Step,
  ) -> impl Future<Output = Result<BatchOutcome, Thrown<Self::Fatal>>>;
  /// Marking, cleaning, reloading and committing a batch that stood.
  fn settle(&mut self, batch: &[Self::Batch]) -> impl Future<Output = Result<(), String>>;
  /// The reason is the failure's cause as a person reads it.
  fn report(&mut self, batch: &[Self::Batch], reason: &str) -> impl Future<Output = ()>;
  fn on_fatal(&mut self, batch: &[Self::Batch], fatal: Self::Fatal) -> impl Future<Output = ()>;
  fn log(&mut self, line: &str);
}

// An error thrown out of a run is one nobody named a cause for: it goes to the log, and the report says the cause is
// unknown and where the log is.
fn thrown_cause<W: RoomWork>(work: &mut W, room: &str, why: &str) -> String {
  work.log(&format!("{room} stopped on an error: {why}"));

  format!("The run stopped on an error nobody anticipated. {}", unknown_cause(&work.record()))
}

/// The room worked. Answers whether it stopped on a failure.
pub async fn work_room<W: RoomWork>(work: &mut W) -> Result<bool, Failure> {
  let room = format!("people digest: chat {}", work.chat_id());
  let mut rounds = 0;
  let mut failures = 0;
  let size = work.batch_size();
  let planned = (work.backlog().await? + size - 1) / size;
  let steps = if work.catch_up() { planned } else { planned.min(work.batches_per_run()) };

  while rounds < steps {
    if (rounds > 0 || failures > 0) && work.resting().await? {
      work.log(&format!("{room} pausing for the rest"));
      return Ok(false);
    }

    if work.paused().await? {
      work.log(&format!("{room} stopping here, digests are paused"));
      return Ok(false);
    }

    let batch = work.next_batch().await?;

    if batch.is_empty() {
      return Ok(false);
    }

    // The lock is held for this batch alone and let go before the next, so the notes digest, which writes common
    // person books under the same lock, is not held off for a whole catch-up.
    let Some(held) = work.lock().await? else {
      work.log(&format!(
        "{room} could not have the person book's lock; its messages wait for the next run"
      ));
      return Ok(false);
    };
    let outcome = match work
      .run(
        &batch,
        Step {
          step: rounds + 1,
          steps,
        },
      )
      .await
    {
      Ok(outcome) => outcome,
      Err(Thrown::Fatal(fatal)) => {
        work.on_fatal(&batch, fatal).await;
        drop(held);
        return Ok(true);
      }
      Err(Thrown::Unexpected(why)) => BatchOutcome::Failed(thrown_cause(work, &room, &why)),
    };

    if let BatchOutcome::Stood = outcome
      && let Err(why) = work.settle(&batch).await
    {
      // The batch is marked already, so trying it again would read the next messages instead.
      work.log(&format!("{room} failed after its batch was marked"));

      let cause = thrown_cause(work, &room, &why);

      work
        .report(
          &batch,
          &format!("The batch was written and marked, but putting it in place afterwards failed. {cause}"),
        )
        .await;
      drop(held);

      return Ok(true);
    }

    drop(held);

    match outcome {
      BatchOutcome::Stood => {
        failures = 0;
        rounds += 1;
      }
      BatchOutcome::Failed(reason) => {
        failures += 1;

        if failures < BATCH_ATTEMPTS {
          work.log(&format!(
            "{room} failed on try {failures} of {BATCH_ATTEMPTS}, its writes put back; trying the same batch again: {reason}"
          ));
          tokio::time::sleep(work.retry_delay()).await;
          continue;
        }

        work.log(&format!("{room} failed {BATCH_ATTEMPTS} times, its messages stay unmarked: {reason}"));
        work
          .report(&batch, &format!("All {BATCH_ATTEMPTS} tries failed. The last one: {reason}"))
          .await;

        return Ok(true);
      }
    }
  }

  Ok(false)
}
