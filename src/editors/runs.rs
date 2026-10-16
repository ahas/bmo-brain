//! An editor's run as the agency keeps it (`docs/boundary-api.md`, 「The agency」): which batch it is of how
//! many, and the clock the brain keeps on its session, asking the agency before it cuts the session at its
//! cap.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::Instant;

use crate::Failure;
use crate::ports::agency::{Agency, Overrun, SessionEnd};

/// Which batch a run is, of how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
  pub step: i64,
  pub steps: i64,
}

/// A session's end, and whether the clock cut it there.
#[derive(Debug, Clone, PartialEq)]
pub struct Waited {
  pub end: SessionEnd,
  pub cut: bool,
}

fn seconds(seconds: i64) -> Duration {
  Duration::from_secs(u64::try_from(seconds).unwrap_or(0))
}

// The moment a Unix time stands for on the clock the waits run on.
fn instant_of(unix: i64) -> Instant {
  let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
  let ahead = u64::try_from(unix).unwrap_or(0).saturating_sub(now);

  Instant::now() + Duration::from_secs(ahead)
}

/// A run's session waited out under its clock: once it passes its cap the agency is asked whether it is kept
/// going, the ask naming the unit the batch runs in, or the run where it runs in none. Kept, the ask is made
/// again at the moment the answer names, or one cap later where it names none; not kept, or where the ask
/// has no answer, the session is stopped and waited for to its end.
pub async fn wait(
  agency: &dyn Agency,
  run: &str,
  unit: Option<&str>,
  cap_seconds: i64,
) -> Result<Waited, Failure> {
  let clock = unit.unwrap_or(run);
  let ended = agency.wait_session(run);
  let mut look_at = Instant::now() + seconds(cap_seconds);

  tokio::pin!(ended);

  loop {
    tokio::select! {
      end = &mut ended => return Ok(Waited { end: end?, cut: false }),
      () = tokio::time::sleep_until(look_at) => {
        match agency.ask_overrun(run, clock, cap_seconds).await {
          Ok(Overrun { keep: true, until }) => {
            look_at = until.map_or_else(|| Instant::now() + seconds(cap_seconds), instant_of);
          }
          Ok(_) => break,
          Err(why) => {
            eprintln!("{run}: the overrun ask had no answer, so the run is cut at its cap: {why}");
            break;
          }
        }
      }
    }
  }

  agency.stop_session(run).await?;

  Ok(Waited {
    end: ended.await?,
    cut: true,
  })
}
