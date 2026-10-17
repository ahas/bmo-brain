//! The brain's clock (docs/boundary-api.md, 「Runs launched apart」; docs/memories.md, 「Watches」): the due digests
//! launched, the reminders whose moment came woken, and the watches swept, each pass on its own period inside the
//! service and never overlapping the one before it.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::api::Api;
use crate::editors::fragments::store::now_seconds;
use crate::editors::people::launcher::wall_clock;
use crate::reminders::fire_due;
use crate::watches::{end_with_closed_memories, fire_timers};

/// When the memo digest is first due after the service starts, and how often after.
pub const MEMO_DIGEST_FIRST: Duration = Duration::from_secs(3 * 60);
pub const MEMO_DIGEST_PASS: Duration = Duration::from_secs(60 * 60);

/// When the person book is first due after the service starts, and how often after.
pub const PEOPLE_DIGEST_FIRST: Duration = Duration::from_secs(5 * 60);
pub const PEOPLE_DIGEST_PASS: Duration = Duration::from_secs(3 * 60 * 60);

/// How often the reminders whose moment has come are woken, the first pass at the start.
pub const REMINDER_PASS: Duration = Duration::from_secs(30);

/// How often the watches are swept. The first sweep waits one pass: which units run is only known once the
/// agency's picture has come in, and a pass before then would launch a second run beside every one standing.
pub const WATCH_SWEEP: Duration = Duration::from_secs(30);

/// One pass once the first wait is over and once a period after, a pass never overlapping the one before it; a
/// pass that fails is said on the process's error output under its name and the next one goes on.
pub async fn every<Pass, Passed>(name: &'static str, first: Duration, period: Duration, pass: Pass)
where
  Pass: Fn() -> Passed,
  Passed: Future<Output = Result<(), String>>,
{
  let mut clock = tokio::time::interval_at(tokio::time::Instant::now() + first, period);

  clock.set_missed_tick_behavior(MissedTickBehavior::Delay);

  loop {
    clock.tick().await;

    if let Err(why) = pass().await {
      eprintln!("bmo-brain: the {name}: {why}");
    }
  }
}

/// One sweep of the watches: the ones whose memory closed ended, the timers that came due fired, then the checks
/// nobody holds launched again.
pub async fn watch_sweep(api: &Api) -> Result<(), String> {
  let now = now_seconds();

  end_with_closed_memories(&api.db, now).await.map_err(|why| why.to_string())?;
  fire_timers(&api.db, api.agency.as_ref(), api.receiver.as_ref(), now)
    .await
    .map_err(|why| why.to_string())?;
  api.launches().relaunch_watches(wall_clock()).await.map_err(|why| why.to_string())
}

/// One pass of the reminders: every one whose moment has come woken in its room.
pub async fn reminder_pass(api: &Api) -> Result<(), String> {
  fire_due(&api.db, api.agency.as_ref(), api.receiver.as_ref(), now_seconds())
    .await
    .map(drop)
    .map_err(|why| why.to_string())
}

/// The clock stood up beside the routes: each pass a task of its own, which goes with the process.
pub fn stand(api: Arc<Api>) -> Vec<JoinHandle<()>> {
  let memo = api.clone();
  let people = api.clone();
  let reminders = api.clone();
  let sweep = api;

  vec![
    tokio::spawn(every("reminder pass", Duration::ZERO, REMINDER_PASS, move || {
      let api = reminders.clone();

      async move { reminder_pass(&api).await }
    })),
    tokio::spawn(every("memo digest", MEMO_DIGEST_FIRST, MEMO_DIGEST_PASS, move || {
      let api = memo.clone();

      async move { api.launches().digest("memo").await }
    })),
    tokio::spawn(every("people digest", PEOPLE_DIGEST_FIRST, PEOPLE_DIGEST_PASS, move || {
      let api = people.clone();

      async move { api.launches().digest("people").await }
    })),
    tokio::spawn(every("watch sweep", WATCH_SWEEP, WATCH_SWEEP, move || {
      let api = sweep.clone();

      async move { watch_sweep(&api).await }
    })),
  ]
}
