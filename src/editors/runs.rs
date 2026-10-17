//! An editor's run as the agency carries it (`docs/boundary-api.md`, 「The agency」): which batch it is of how many,
//! its turn on the work key's editor session — asked again while the session carries another — and the brain's
//! watch on it past its cap. The run's clock is the agency's; past the cap the brain reads it, and stops the run
//! only where no bound, keep or open ask stands on it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::Failure;
use crate::ports::agency::{Agency, EditorTurn, TurnEnd, TurnFault};

/// How long a turn met by a session carrying another waits before it is asked again.
pub const BUSY_AGAIN: Duration = Duration::from_secs(5);

// How soon the cap is looked at again where it passed before the run said what it goes out as.
const NOT_YET_STARTED: Duration = Duration::from_secs(1);

/// Which batch a run is, of how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
  pub step: i64,
  pub steps: i64,
}

/// A turn's end, and whether the brain stopped it at its cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turned {
  pub end: TurnEnd,
  pub cut: bool,
}

/// Why a run cut at its cap failed, as a person reads it.
pub fn cut_at(cap_seconds: i64) -> String {
  format!(
    "The model run went past its {}-minute limit and was stopped.",
    (cap_seconds as f64 / 60.0).round()
  )
}

fn seconds(seconds: i64) -> Duration {
  Duration::from_secs(u64::try_from(seconds).unwrap_or(0))
}

fn unix_now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

// The moment a Unix time stands for on the clock the waits run on.
fn instant_of(unix: i64) -> Instant {
  Instant::now() + seconds(unix.saturating_sub(unix_now()))
}

/// One turn of a work key's editor session carried to its end: the session opened, the turn asked again every
/// little while the session carries another, and the run watched past its cap. The clock is read by the unit the
/// batch runs in, or by the run where it runs in none.
pub async fn turn(
  agency: &dyn Agency,
  work_key: &str,
  mut asked: EditorTurn,
  cap_seconds: i64,
) -> Result<Turned, Failure> {
  asked.session = agency.editor_session(work_key).await?;

  loop {
    match watched(agency, &asked, cap_seconds).await {
      Err(TurnFault::Busy) => tokio::time::sleep(BUSY_AGAIN).await,
      Err(TurnFault::Failed(why)) => return Err(why.into()),
      Ok(turned) => return Ok(turned),
    }
  }
}

// The turn carried while the cap is watched: past it, the run's clock is read, and where no bound, keep or open ask
// stands on it any longer the run is stopped and its end waited for.
async fn watched(agency: &dyn Agency, asked: &EditorTurn, cap_seconds: i64) -> Result<Turned, TurnFault> {
  let (started, mut heard) = oneshot::channel();
  let carried = agency.editor_turn(asked, started);
  let mut run: Option<String> = None;
  let mut listening = true;
  let mut cut = false;
  let look = tokio::time::sleep(seconds(cap_seconds));

  tokio::pin!(carried);
  tokio::pin!(look);

  loop {
    tokio::select! {
      end = &mut carried => return end.map(|end| Turned { end, cut }),
      began = &mut heard, if listening => {
        listening = false;
        run = began.ok();
      }
      () = &mut look, if !cut => {
        let Some(going) = run.clone() else {
          look.as_mut().reset(Instant::now() + NOT_YET_STARTED);
          continue;
        };
        let name = asked.parent.as_deref().unwrap_or(&going);

        match agency.run_clock(name).await {
          Ok(clock) => {
            let later = clock.kept_until.unwrap_or(0).max(clock.cap_ends.unwrap_or(0));

            if later > unix_now() {
              look.as_mut().reset(instant_of(later));
              continue;
            }
          }
          Err(why) => eprintln!("{going}: its clock could not be read, so it is cut at its cap: {why}"),
        }

        if let Err(why) = agency.stop_run(&going, &cut_at(cap_seconds)).await {
          eprintln!("{going}: the run could not be stopped at its cap: {why}");
        }

        cut = true;
      }
    }
  }
}
