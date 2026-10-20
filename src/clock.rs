//! The brain's clock (docs/boundary-api.md, 「Reminders」, 「Watches」, 「Locks」, 「Scopes」 and 「Events」): every
//! second it fires the reminders and the `at` watches whose moment has come and lets the run-out locks go;
//! every minute it withers the forgotten scopes' rows and lets go of the events past the window. A pass that
//! fails is said on the error output and the next goes on.

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::store::events::WINDOW_MS;
use crate::store::scopes::WITHER_AFTER_MS;
use crate::store::{Held, Store, now};

/// How often the moments are looked at.
pub const TICK: Duration = Duration::from_secs(1);

/// How many ticks pass between two slow passes.
const SLOW_EVERY: u32 = 60;

/// One pass over the moments as they stand at `at`.
pub async fn tick(store: &Store, at: i64) -> Held<()> {
  store.fire_reminders(at).await?;
  store.fire_moments(at).await?;
  store.expire_locks(at).await?;
  Ok(())
}

/// One pass over what changes slowly, as it stands at `at`.
pub async fn slow(store: &Store, at: i64) -> Held<()> {
  store.wither(at.saturating_sub(WITHER_AFTER_MS)).await?;
  store.prune_events(at.saturating_sub(WINDOW_MS)).await?;
  Ok(())
}

/// The clock, running until it is aborted.
pub fn stand(store: Store) -> JoinHandle<()> {
  tokio::spawn(async move {
    let mut ticks = tokio::time::interval(TICK);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut count: u32 = 0;
    loop {
      ticks.tick().await;
      let at = now();
      if let Err(why) = tick(&store, at).await {
        eprintln!("bmo-brain: the clock's pass failed: {why}");
      }
      if count.is_multiple_of(SLOW_EVERY)
        && let Err(why) = slow(&store, at).await
      {
        eprintln!("bmo-brain: the clock's slow pass failed: {why}");
      }
      count = count.wrapping_add(1);
    }
  })
}
