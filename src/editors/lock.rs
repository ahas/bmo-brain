//! The editing lock of an agent's brain folder and the jobs' brain hold (docs/people-book.md, 「One lock per
//! agent's folder」, 「The lock passes in the order waiting began」 and 「A job editing the brain holds the
//! editors off」).
//!
//! The lock is `flock(2)` on the folder's lock file under the data root, held on this process's own descriptor
//! for as long as it wants it. A bare lock hands itself to whichever waiter the kernel wakes, so every taker
//! first stands a ticket in the queue directory beside the lock and goes for the lock, without waiting on it,
//! only once no living ticket stands ahead of its own. A ticket's name sorts by when it was stood, front (`0-`)
//! and next (`0.`) marks sorting ahead of every time, the taker's pid last; the holder's name is on the file's
//! first line, a kept place's key on the second and `kept` on the third. A ticket whose process is gone is taken
//! away by the next taker that looks, and a kept place, whose taker is away, holds no one up.
//!
//! A folder may be fenced (「One lock per agent's folder」): the moment is kept in a file beside the lock, and a
//! process begun before it is refused the folder's lock from then on.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use brain_support::process::process_started_ms;
use sea_orm::DatabaseConnection;

use crate::Failure;
use crate::ports::agency::Agency;
use crate::settings;

// What every agent folder's editing lock file under the data root is named from.
const LOCK_STEM: &str = "people-digest";

// A ticket's third line where a stand-in keeps the place of a taker that is away.
const KEPT: &str = "kept";

// The settings a job's hold stands under, by its run's name.
const HOLD_PREFIX: &str = "brain_hold:";

/// How often a queued taker looks whether its turn has come.
pub const QUEUE_POLL: Duration = Duration::from_millis(500);

/// How often an editor held off by a job looks again.
pub const HOLD_POLL: Duration = Duration::from_secs(30);

static TICKETS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

/// The editing lock file of one agent's brain folder, named relative to the brain root: the folder
/// percent-encoded as a URI component, and the stem alone for the brain root.
pub fn brain_lock_path(data_root: &Path, folder: &str) -> PathBuf {
  if folder.is_empty() {
    return data_root.join(format!("{LOCK_STEM}.lock"));
  }

  let encoded: String = folder
    .bytes()
    .map(|byte| {
      if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
        char::from(byte).to_string()
      } else {
        format!("%{byte:02X}")
      }
    })
    .collect();

  data_root.join(format!("{LOCK_STEM}.{encoded}.lock"))
}

/// The names of every agent folder's editing lock file under the data root that a brain editor holds right now,
/// in name order, asked without waiting for any.
pub fn brain_locks_taken(data_root: &Path) -> Vec<String> {
  let Ok(entries) = fs::read_dir(data_root) else {
    return vec![];
  };
  let mut taken: Vec<String> = entries
    .flatten()
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| {
      name == &format!("{LOCK_STEM}.lock")
        || (name.starts_with(&format!("{LOCK_STEM}.")) && name.ends_with(".lock"))
    })
    .filter(|name| {
      File::open(data_root.join(name)).is_ok_and(|file| match file.try_lock() {
        Ok(()) => {
          let _ = file.unlock();
          false
        }
        Err(_) => true,
      })
    })
    .collect();

  taken.sort();
  taken
}

/// Whether a process of this machine is still running; another user's process answers that it may not be
/// signalled, and is alive all the same.
pub fn process_alive(pid: i64) -> bool {
  let Ok(pid) = libc::pid_t::try_from(pid) else { return false };

  // SAFETY: signal 0 asks only whether the process may be signalled.
  pid > 0
    && (unsafe { libc::kill(pid, 0) } == 0
      || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

/// The queue directory beside a lock file, where its takers stand their tickets.
pub fn queue_of(lock: &Path) -> PathBuf {
  let mut queue = lock.as_os_str().to_owned();

  queue.push(".queue");
  PathBuf::from(queue)
}

fn ticket_pid(name: &str) -> i64 {
  name.rsplit('-').next().and_then(|pid| pid.parse().ok()).unwrap_or(0)
}

// A ticket's name, sorting by when it was stood behind the mark given: none for the back of the queue, the front
// mark to stand ahead of every timed ticket.
fn ticket_name(mark: &str) -> String {
  let counter = TICKETS.fetch_add(1, Ordering::Relaxed) + 1;

  format!("{mark}{:015}-{counter:06}-{}", now_ms(), std::process::id())
}

// The mark a front ticket opens with.
const FRONT: &str = "0-";

// When a ticket was stood, read off its name past any front or next mark.
fn ticket_time(name: &str) -> Option<i64> {
  let timed = name.strip_prefix("0-").or_else(|| name.strip_prefix("0.")).unwrap_or(name);

  timed.split('-').next()?.parse().ok()
}

// Whether the process now under a ticket's pid is the one that stood it: a process begun after the ticket was stood
// holds its pid by reuse.
fn stood_by(name: &str) -> bool {
  let (Some(stood), Ok(pid)) = (ticket_time(name), i32::try_from(ticket_pid(name))) else {
    return false;
  };

  process_started_ms(pid).is_some_and(|began| began <= stood)
}

/// The pids of every living taker standing in the lock's queue, this process aside, in queue order: the holder and
/// whoever waits behind it. A kept place, whose taker is away, is no taker, and neither is a process holding a
/// ticket's pid by reuse.
pub fn takers(path: &Path) -> Vec<i64> {
  let queue = queue_of(path);
  let own = i64::from(std::process::id());

  sorted_names(&queue)
    .unwrap_or_default()
    .into_iter()
    .filter(|name| {
      let kept =
        fs::read_to_string(queue.join(name)).is_ok_and(|text| text.split('\n').nth(2) == Some(KEPT));

      !kept
    })
    .filter(|name| ticket_pid(name) != own && process_alive(ticket_pid(name)) && stood_by(name))
    .map(|name| ticket_pid(&name))
    .collect()
}

// The file a folder's fence is kept in, beside its lock.
fn fence_path(data_root: &Path, folder: &str) -> PathBuf {
  let mut path = brain_lock_path(data_root, folder).into_os_string();

  path.push(".fence");
  PathBuf::from(path)
}

/// The folder fenced now: no process begun before this moment takes its lock again.
pub fn fence(data_root: &Path, folder: &str) -> std::io::Result<()> {
  brain_support::disk::publish_bytes(&fence_path(data_root, folder), now_ms().to_string().as_bytes())
}

/// The folder's lock file, its queue and its fence removed, once nothing writes the folder any more: what
/// could not be removed, each named with why.
pub fn forget(data_root: &Path, folder: &str) -> Vec<String> {
  let lock = brain_lock_path(data_root, folder);
  let mut left = vec![];

  for path in [queue_of(&lock), fence_path(data_root, folder), lock] {
    let removed = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };

    if let Err(why) = removed
      && why.kind() != std::io::ErrorKind::NotFound
    {
      left.push(format!("{}: {why}", path.display()));
    }
  }

  left
}

/// Whether the folder was fenced after this process began.
pub fn fenced_out(data_root: &Path, folder: &str) -> bool {
  let Some(fenced) = fs::read_to_string(fence_path(data_root, folder))
    .ok()
    .and_then(|text| text.trim().parse::<i64>().ok())
  else {
    return false;
  };
  let pid = i32::try_from(std::process::id()).unwrap_or(0);

  process_started_ms(pid).is_some_and(|began| began < fenced)
}

/// The tickets standing in a lock's queue, in the order they are served, or none where the queue is not there.
pub fn sorted_names(queue: &Path) -> Option<Vec<String>> {
  let mut names: Vec<String> = fs::read_dir(queue)
    .ok()?
    .flatten()
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect();

  names.sort();
  Some(names)
}

/// The living ticket standing ahead of this one, as the name its taker stood under (empty where it gave none
/// or the ticket went between the listing and the read), or none when none stands ahead. A kept place is passed
/// over; a ticket whose process is gone is taken away.
fn ticket_ahead(queue: &Path, ticket: &str) -> Option<String> {
  for name in sorted_names(queue)? {
    if name.as_str() >= ticket {
      return None;
    }

    if !process_alive(ticket_pid(&name)) {
      let _ = fs::remove_file(queue.join(&name));
      continue;
    }

    let Ok(text) = fs::read_to_string(queue.join(&name)) else {
      return Some(String::new());
    };
    let lines: Vec<&str> = text.split('\n').collect();

    if lines.get(2) != Some(&KEPT) {
      return Some(lines.first().copied().unwrap_or_default().to_owned());
    }
  }

  None
}

/// What a wait that ran out asks before it gives up: a later moment to wait until behind the named taker, in
/// milliseconds since the epoch — none where the lock is held from outside the queue — or none to give up.
pub trait WaitOn {
  fn later(&self, holder: Option<&str>) -> impl Future<Output = Option<i64>>;
}

/// A wait that gives up when it runs out.
pub struct NoKeep;

impl WaitOn for NoKeep {
  async fn later(&self, _holder: Option<&str>) -> Option<i64> {
    None
  }
}

/// A wait on another run's lock that goes on for as long as the agency keeps that run past its clock, and its
/// slack after, by the agency's clock keep.
pub struct FollowKeep<'a> {
  pub agency: &'a dyn Agency,
  pub slack_ms: i64,
}

impl WaitOn for FollowKeep<'_> {
  async fn later(&self, holder: Option<&str>) -> Option<i64> {
    let holder = holder?;

    match self.agency.kept_until(holder).await {
      Ok(until) => until.map(|until| until.saturating_mul(1000).saturating_add(self.slack_ms)),
      Err(why) => {
        eprintln!("brain lock: whether {holder} is kept could not be read: {why}");
        None
      }
    }
  }
}

/// The lock as its taker holds it: the descriptor holding the lock, and the ticket that goes with it. Both go on
/// release, or when it is dropped.
#[derive(Debug)]
pub struct HeldLock {
  file: Option<File>,
  ticket: PathBuf,
}

impl HeldLock {
  /// The lock let go and the ticket taken away.
  pub fn release(mut self) {
    self.let_go();
  }

  fn let_go(&mut self) {
    if let Some(file) = self.file.take() {
      let _ = file.unlock();
      let _ = fs::remove_file(&self.ticket);
    }
  }
}

impl Drop for HeldLock {
  fn drop(&mut self) {
    self.let_go();
  }
}

/// The lock, or none when it could not be had within the wait. `holder` is the name this taker stands in the
/// queue under, which a taker behind it is told while it waits; where the wait runs out, `wait_on` is asked
/// whether to go on behind the run ahead, and a wait that goes on keeps its place.
pub async fn take_lock<W: WaitOn>(
  path: &Path,
  wait_seconds: u64,
  holder: &str,
  wait_on: &W,
  poll: Duration,
) -> std::io::Result<Option<HeldLock>> {
  take_lock_as(path, &ticket_name(""), wait_seconds, holder, wait_on, poll).await
}

/// The lock taken as [`take_lock`] takes it, but from a front ticket, ahead of every taker waiting in the queue.
pub async fn take_lock_first<W: WaitOn>(
  path: &Path,
  wait_seconds: u64,
  holder: &str,
  wait_on: &W,
  poll: Duration,
) -> std::io::Result<Option<HeldLock>> {
  take_lock_as(path, &ticket_name(FRONT), wait_seconds, holder, wait_on, poll).await
}

async fn take_lock_as<W: WaitOn>(
  path: &Path,
  ticket: &str,
  wait_seconds: u64,
  holder: &str,
  wait_on: &W,
  poll: Duration,
) -> std::io::Result<Option<HeldLock>> {
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }

  let file = OpenOptions::new().create(true).append(true).open(path)?;
  let queue = queue_of(path);

  fs::create_dir_all(&queue)?;

  let ticket_path = queue.join(ticket);
  let poll_ms = i64::try_from(poll.as_millis()).unwrap_or(i64::MAX);
  let mut deadline = now_ms() + i64::try_from(wait_seconds).unwrap_or(i64::MAX / 2_000) * 1000;

  fs::write(&ticket_path, holder)?;

  loop {
    let ahead = ticket_ahead(&queue, ticket);

    if ahead.is_none() {
      match file.try_lock() {
        Ok(()) => {
          return Ok(Some(HeldLock {
            file: Some(file),
            ticket: ticket_path,
          }));
        }
        Err(TryLockError::WouldBlock) => {}
        Err(TryLockError::Error(why)) => {
          let _ = fs::remove_file(&ticket_path);
          return Err(why);
        }
      }
    }

    // Past the wait: asked whether to go on, naming the run ahead — none where the lock is held from outside
    // the queue or by a holder standing behind this one.
    if now_ms() >= deadline {
      let named = ahead.as_deref().filter(|name| !name.is_empty());

      match wait_on.later(named).await {
        Some(later) if later > now_ms() => deadline = later,
        _ => {
          let _ = fs::remove_file(&ticket_path);
          return Ok(None);
        }
      }
    }

    let left = (deadline - now_ms()).clamp(0, poll_ms);

    tokio::time::sleep(Duration::from_millis(u64::try_from(left).unwrap_or(0))).await;
  }
}

/// The run names of the jobs holding the brain against a process running as `own_run`: every hold whose job's
/// run the agency still has open, but that process's own. A hold whose run is over is cleared here.
pub async fn brain_holders(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  own_run: Option<&str>,
) -> Result<Vec<String>, Failure> {
  let mut holders = vec![];

  for (held_by, _) in settings::prefixed(db, HOLD_PREFIX).await? {
    if !agency.run_open(&held_by).await? {
      settings::forget(db, &format!("{HOLD_PREFIX}{held_by}")).await?;
    } else if Some(held_by.as_str()) != own_run {
      holders.push(held_by);
    }
  }

  Ok(holders)
}

/// Whether some other job holds the brain against this process, whose own run is never one. A hold that cannot be
/// read holds nothing: an editor is not stopped by a hold it cannot see.
pub async fn brain_held(db: &DatabaseConnection, agency: &dyn Agency, own_run: Option<&str>) -> bool {
  match brain_holders(db, agency, own_run).await {
    Ok(holders) => !holders.is_empty(),
    Err(why) => {
      eprintln!("brain hold: the holds could not be read: {why}");
      false
    }
  }
}

/// Waits until no job holds the brain, looking again every `poll`. Answers whether it had to wait at all.
pub async fn wait_for_brain(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  who: &str,
  own_run: Option<&str>,
  poll: Duration,
) -> bool {
  if !brain_held(db, agency, own_run).await {
    return false;
  }

  println!("{who}: a job is editing the brain; waiting until it has finished");

  while brain_held(db, agency, own_run).await {
    tokio::time::sleep(poll).await;
  }

  println!("{who}: the brain is free again");

  true
}

/// What taking an agent folder's editing lock is told: who asks, how long it waits in the queue, the folder,
/// the run this process works for, the name it stands in the queue under, and how often each wait looks again.
pub struct BrainLockAsk<'a> {
  pub who: &'a str,
  pub wait_seconds: u64,
  pub folder: &'a str,
  pub own_run: Option<&'a str>,
  pub holder: &'a str,
  pub hold_poll: Duration,
  pub queue_poll: Duration,
}

/// The editing lock of the folder an editor writes in, taken only while no job holds the brain: the hold is
/// waited out before the lock is asked for, and a hold set while it was being taken lets it go and waits once
/// more, so an editor never holds the lock while it waits. None when the lock could not be had, and for good once
/// the folder was fenced after this process began.
pub async fn take_brain_lock<W: WaitOn>(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  data_root: &Path,
  ask: &BrainLockAsk<'_>,
  wait_on: &W,
) -> std::io::Result<Option<HeldLock>> {
  let path = brain_lock_path(data_root, ask.folder);

  loop {
    wait_for_brain(db, agency, ask.who, ask.own_run, ask.hold_poll).await;

    let Some(lock) = take_lock(&path, ask.wait_seconds, ask.holder, wait_on, ask.queue_poll).await? else {
      return Ok(None);
    };

    // Read under the lock, since the fence is put down before its putter takes the lock.
    if fenced_out(data_root, ask.folder) {
      println!(
        "{}: the folder was fenced after this process began, so it writes nothing more there",
        ask.who
      );
      lock.release();
      return Ok(None);
    }

    if !brain_held(db, agency, ask.own_run).await {
      return Ok(Some(lock));
    }

    lock.release();
  }
}
