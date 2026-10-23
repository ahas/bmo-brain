//! The brain's clock (docs/boundary-api.md, 「Events」): every minute it lets go of the events past the window. A
//! pass that fails is said on the error output and the next goes on.

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::store::events::WINDOW_MS;
use crate::store::{Held, Store, now};

/// How often the clock passes.
pub const EVERY: Duration = Duration::from_secs(60);

/// One pass, as the store stands at `at`.
pub async fn pass(store: &Store, at: i64) -> Held<()> {
  store.prune_events(at.saturating_sub(WINDOW_MS)).await?;
  Ok(())
}

/// The clock, running until it is aborted.
pub fn stand(store: Store) -> JoinHandle<()> {
  tokio::spawn(async move {
    let mut ticks = tokio::time::interval(EVERY);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
      ticks.tick().await;
      if let Err(why) = pass(&store, now()).await {
        eprintln!("bmo-brain: the clock's pass failed: {why}");
      }
    }
  })
}
