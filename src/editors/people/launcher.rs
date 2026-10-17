//! The person-book launcher (docs/people-book.md, 「The launcher and the worker」): every agent's book gone through in
//! turn, the brain root's agent first, each room it answers with enough waiting given a worker of its own — one
//! after another and waited for, so two rooms never hold a writer at once. A worker already standing is left to
//! the next round, a room's worker is not started while a job holds the brain, and the wait on a worker follows the
//! batch it is at and whatever the agency kept that batch for, bounded by the cap the worker runs under.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::super::archive::people_counts;
use super::super::lock::{HOLD_POLL, wait_for_brain};
use super::super::recollection::recollections;
use super::{BATCH_SIZE, CAP_SECONDS, chats_to_digest, worker_unit};
use crate::Failure;
use crate::schedules::launches::Launches;

/// How long past a room worker's cap, or past the keep the agency gave it, the launcher still waits for it.
pub const LAUNCHER_SLACK_MS: i64 = 60_000;

// How often the launcher looks whether the worker has ended.
const LOOK: Duration = Duration::from_secs(2);

/// The moment it is, in milliseconds since the epoch, by the machine's clock.
pub fn wall_clock() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

/// The person-book launcher of one process: the launches it starts workers through, and the clock it waits by, in
/// milliseconds since the epoch.
pub struct Launcher<'a> {
  pub launches: &'a Launches<'a>,
  pub clock: &'a (dyn Fn() -> i64 + Sync),
}

impl Launcher<'_> {
  /// Whether the worker ended within the cap of the batch it is at, or within whatever the agency kept that batch
  /// for; at first, within one editor's cap.
  pub async fn wait_for_kept(&self, unit: &str) -> Result<bool, Failure> {
    let agency = self.launches.ground.agency;
    let cap_ms = i64::try_from(CAP_SECONDS * 1000).unwrap_or(i64::MAX);
    let mut deadline = (self.clock)() + cap_ms + LAUNCHER_SLACK_MS;

    loop {
      while self.launches.standing(unit).await {
        let now = (self.clock)();

        if now >= deadline {
          break;
        }

        tokio::time::sleep(LOOK.min(Duration::from_millis(u64::try_from(deadline - now).unwrap_or(0))))
          .await;
      }

      if !self.launches.standing(unit).await {
        return Ok(true);
      }

      let clock = agency.run_clock(unit).await?;
      let until = clock.kept_until.unwrap_or(0).max(clock.cap_ends.unwrap_or(0)).saturating_mul(1000);

      if until + LAUNCHER_SLACK_MS <= (self.clock)() {
        return Ok(false);
      }

      deadline = until + LAUNCHER_SLACK_MS;
    }
  }

  // One agent's rooms, the smallest backlog first. Answers whether any failed.
  async fn launch_agent_workers(&self, agent: &str) -> Result<bool, Failure> {
    let ground = &self.launches.ground;
    let counts = people_counts(ground.db, ground.receiver, agent).await?;
    let mut failed = false;

    for chat_id in chats_to_digest(&counts) {
      let unit = worker_unit(chat_id, agent);

      // A room's worker would only stand waiting behind a job holding the brain, so it is not started until then.
      wait_for_brain(ground.db, ground.agency, "people digest launcher", ground.own_run, HOLD_POLL).await;

      if self.launches.standing(&unit).await {
        println!("people digest: a worker for chat {chat_id} is already running ({unit})");
        continue;
      }

      // A room with a whole batch or more waiting is a backlog, and its worker keeps going until it is gone.
      let catch_up = counts.get(&chat_id).copied().unwrap_or(0) >= BATCH_SIZE;

      match self.launches.people_worker(&unit, chat_id, agent, catch_up).await {
        Ok(()) => {
          if !self.wait_for_kept(&unit).await? {
            eprintln!("people digest: {unit} was still running after its own cap; leaving it be");
            failed = true;
          }
        }
        Err(why) => {
          eprintln!("people digest: {unit} launch failed: {why}");
          failed = true;
        }
      }
    }

    Ok(failed)
  }

  /// Every agent's rooms, the brain root's agent first. Answers whether any failed.
  pub async fn launch_workers(&self) -> Result<bool, Failure> {
    let ground = &self.launches.ground;
    let mut failed = false;

    for recollection in recollections(ground.agency, ground.brain_root, ground.chat_key).await? {
      if self.launch_agent_workers(&recollection.agent).await? {
        failed = true;
      }
    }

    Ok(failed)
  }
}
