//! The half of a watch that does the watching (docs/memories.md, 「Watches」): the run the agency launches for a
//! watch, which checks every interval until the check succeeds, the wait runs out, or the watch stops being its own
//! (cancelled, or handed to a run launched after it), and ends its watch with the memo of how it went.
//!
//! The row decides everything. A run only ever writes a watch that names its own unit, so a run the clock replaced
//! cannot end the watch or check it again, and a watch ends once however many runs raced for it.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::Failure;
use crate::editors::archive::room_name;
use crate::editors::fragments::store::now_seconds;
use crate::ports::agency::Agency;
use crate::ports::receiver::Receiver;
use crate::watches::{
  TIMED_OUT_EXIT, Wait, broken_memo, bus_unreachable, end_by_run, fired_memo, item_of, lapsed_memo,
  record_check, watch_of,
};

/// How often a waiting run looks at its watch, so a cancel or a hand-over ends it within moments rather than at
/// its next check.
pub const IDLE_SLICE: Duration = Duration::from_secs(5);

/// How long one check may run before it is cut off.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

// What of a check's output is carried back: enough to say what it found, never a whole log.
const KEPT_STDOUT: usize = 4_000;
const KEPT_STDERR: usize = 1_000;

// How long the output of a check that left something behind holding its pipe is waited on.
const OUTPUT_SETTLE: Duration = Duration::from_secs(2);

// The interval a condition watch is checked at where its row names none.
const DEFAULT_INTERVAL_SECONDS: i64 = 60;

// The exits that mean the check never ran at all: not executable, or no such command.
const CANNOT_RUN: [i64; 2] = [126, 127];

/// What one check came to: its exit, and as much of what it printed as is carried back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
  pub exit: i64,
  pub stdout: String,
  pub stderr: String,
}

impl Check {
  /// Whether the check never ran at all.
  pub fn cannot_run(&self) -> bool {
    CANNOT_RUN.contains(&self.exit)
  }

  /// Whether the check judged nothing: what it printed says it never reached the user's service manager, so it is
  /// never taken as the thing having happened, whatever it exited.
  pub fn undecided(&self) -> bool {
    bus_unreachable(&self.stderr) || bus_unreachable(&self.stdout)
  }
}

// What a stream printed, read to its end so a check that prints a lot is never held on a full pipe, and kept to
// `limit` characters, counted as the console counts them, cut with an ellipsis.
async fn read_clipped(mut stream: impl AsyncRead + Unpin, limit: usize) -> String {
  let mut kept = Vec::new();
  let mut chunk = [0u8; 8192];

  // A character takes at most four bytes, so this many bytes always hold `limit` of them.
  while let Ok(read) = stream.read(&mut chunk).await {
    if read == 0 {
      break;
    }

    if kept.len() < limit * 4 {
      kept.extend_from_slice(&chunk[..read]);
    }
  }

  let text = String::from_utf8_lossy(&kept);
  let units: Vec<u16> = text.encode_utf16().collect();

  if units.len() > limit {
    format!("{}…", String::from_utf16_lossy(&units[..limit]))
  } else {
    text.into_owned()
  }
}

// The environment a check runs in: this process's own, with the user's service manager reachable, since the check
// most often written is whether a unit still runs.
fn check_environment() -> BTreeMap<String, String> {
  let mut env: BTreeMap<String, String> = std::env::vars().collect();

  if env.get("XDG_RUNTIME_DIR").is_none_or(String::is_empty) {
    // SAFETY: getuid reads this process's own user and cannot fail.
    env.insert("XDG_RUNTIME_DIR".into(), format!("/run/user/{}", unsafe { libc::getuid() }));
  }

  if env.get("DBUS_SESSION_BUS_ADDRESS").map(String::as_str) == Some("disabled:") {
    env.remove("DBUS_SESSION_BUS_ADDRESS");
  }

  env
}

/// One check run. Its input is closed, so a command that falls back to reading stdin sees the end at once; it runs
/// in a process group of its own, and once it has exited or been cut off the whole group is killed, so nothing it
/// started outlives it or holds its output open.
pub async fn run_check(condition: &str, cwd: &Path, timeout: Duration) -> Check {
  let spawned = tokio::process::Command::new("bash")
    .arg("-c")
    .arg(condition)
    .current_dir(cwd)
    .env_clear()
    .envs(check_environment())
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .process_group(0)
    .spawn();
  let mut child = match spawned {
    Ok(child) => child,
    Err(why) => {
      return Check {
        exit: 127,
        stdout: String::new(),
        stderr: why.to_string(),
      };
    }
  };
  // The group is named by the check's own id, taken now: once the check has been waited for, it is no longer
  // known by it.
  let group = child.id();
  let stdout = child.stdout.take().map(|out| tokio::spawn(read_clipped(out, KEPT_STDOUT)));
  let stderr = child.stderr.take().map(|err| tokio::spawn(read_clipped(err, KEPT_STDERR)));
  let exited = tokio::time::timeout(timeout, child.wait()).await;

  if let Some(pid) = group.and_then(|pid| i32::try_from(pid).ok()) {
    // SAFETY: the group is the check's own, made for it at its spawn.
    unsafe {
      libc::kill(-pid, libc::SIGKILL);
    }
  }

  let status = match exited {
    Ok(status) => status.ok(),
    Err(_) => {
      let _ = child.wait().await;
      None
    }
  };
  let exit = status.map_or(TIMED_OUT_EXIT, |status| {
    use std::os::unix::process::ExitStatusExt;

    status.code().map_or_else(|| 128 + i64::from(status.signal().unwrap_or(0)), i64::from)
  });
  let settled = async |reading: Option<tokio::task::JoinHandle<String>>| match reading {
    Some(reading) => tokio::time::timeout(OUTPUT_SETTLE, reading)
      .await
      .ok()
      .and_then(Result::ok)
      .unwrap_or_default(),
    None => String::new(),
  };

  Check {
    exit,
    stdout: settled(stdout).await,
    stderr: settled(stderr).await,
  }
}

/// What a watch's run stands on: the brain's database, the agency its memos go through, the receiver its room
/// names come from, and the folder its checks run in.
pub struct WatchRun<'a> {
  pub db: &'a DatabaseConnection,
  pub agency: &'a dyn Agency,
  pub receiver: &'a dyn Receiver,
  pub cwd: &'a Path,
  /// How often a waiting run looks at its watch.
  pub slice: Duration,
}

/// One watch watched by the run of `unit`, each check's line printed as it is made: how it ended.
pub async fn watch(run: &WatchRun<'_>, id: i64, unit: &str) -> Result<&'static str, Failure> {
  let db = run.db;
  let mut checks = 0;
  let mut last: Option<(i64, i64)> = None;
  let mut next_check_at = now_seconds();

  loop {
    let standing = match watch_of(db, id).await? {
      Some(standing) if standing.state == "active" => standing,
      _ => return Ok("the watch is no longer standing"),
    };
    let condition = match (&standing.unit, &standing.condition) {
      (Some(held), Some(condition)) if held == unit => condition.clone(),
      _ => return Ok("the watch is held by another run"),
    };
    let now = now_seconds();
    let last_call = now >= standing.deadline_at && last.map_or(0, |(_, at)| at) < standing.deadline_at;

    if now >= next_check_at || last_call {
      let check = run_check(&condition, run.cwd, CHECK_TIMEOUT).await;
      let at = now_seconds();

      checks += 1;
      last = Some((check.exit, at));
      println!("{} check {checks} exited {}", brain_support::clock::clock(at), check.exit);

      if !record_check(db, id, unit, check.exit, at).await? {
        return Ok("the watch stopped being this run's while it checked");
      }

      let watch = &standing.watch;
      let room = room_name(run.receiver, watch.chat_id).await?;
      let item = match watch.memory_id {
        Some(memory) => item_of(db, memory).await?,
        None => None,
      };

      // A check that could not run, or that ran and judged nothing, is broken where it is the first one: it will
      // not start working by being asked again. A later one that judged nothing is waited past like a no, since the
      // thing it asks about has not been seen to happen.
      if checks == 1 && (check.cannot_run() || check.undecided()) {
        let memo = broken_memo(watch, &room, item.as_ref(), &condition, check.exit, &check.stderr);

        end_by_run(db, run.agency, watch, unit, "broken", &memo, now_seconds()).await?;
        return Ok("the check could not run");
      }

      if check.exit == 0 && !check.undecided() {
        let memo = fired_memo(watch, &room, item.as_ref(), &condition, at, &check.stdout);

        end_by_run(db, run.agency, watch, unit, "fired", &memo, now_seconds()).await?;
        return Ok("the check succeeded");
      }

      if at >= standing.deadline_at {
        let wait = Wait {
          condition: &condition,
          deadline_at: standing.deadline_at,
          interval_seconds: standing.interval_seconds,
          last,
        };
        let memo = lapsed_memo(watch, &room, item.as_ref(), &wait);

        end_by_run(db, run.agency, watch, unit, "lapsed", &memo, now_seconds()).await?;
        return Ok("the wait ran out");
      }

      next_check_at = at + standing.interval_seconds.unwrap_or(DEFAULT_INTERVAL_SECONDS);
    }

    let until_ms = next_check_at.min(standing.deadline_at) * 1000 - now_seconds() * 1000;
    let wait_ms = until_ms.min(i64::try_from(run.slice.as_millis()).unwrap_or(i64::MAX)).max(200);

    tokio::time::sleep(Duration::from_millis(wait_ms.unsigned_abs())).await;
  }
}
