//! An agent's folder put out of a reset's way (docs/boundary-api.md, 「Agents forgetting」): every program working in
//! it killed and no other, and its editing lock held ahead of every waiting taker with the folder fenced, a taker
//! still holding it killed rather than waited for and another folder's editor never waited on.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bmo_brain::editors::lock::{brain_lock_path, fenced_out, queue_of};
use bmo_brain::lifecycle::{KILL_SETTLE, hold_folder, stop_in_folder};

const FOLDER: &str = "agents/agent-b";

// A program in a process group of its own, killed with its group however the test ends.
struct Sleeper(std::process::Child);

impl Sleeper {
  fn new(args: &[&str], cwd: &Path) -> Self {
    std::fs::create_dir_all(cwd).unwrap();
    Self(
      std::process::Command::new(args[0])
        .args(&args[1..])
        .current_dir(cwd)
        .process_group(0)
        .spawn()
        .unwrap(),
    )
  }

  fn group(&self) -> i32 {
    i32::try_from(self.0.id()).unwrap()
  }
}

impl Drop for Sleeper {
  fn drop(&mut self) {
    // SAFETY: the group is this test's own child's, and a signal to one gone since reaches nobody.
    unsafe {
      libc::kill(-self.group(), libc::SIGKILL);
    }
    let _ = self.0.wait();
  }
}

#[test]
fn every_program_working_in_the_folder_or_one_of_its_name_removed_since_is_killed_and_no_other() {
  let dir = tempfile::tempdir().unwrap();
  let folder = dir.path().join(FOLDER);
  let other = dir.path().join("agents/agent-b-2");
  let mut inside = Sleeper::new(&["sleep", "60"], &folder.join("rooms"));
  let mut beside = Sleeper::new(&["sleep", "60"], &other);

  assert!(stop_in_folder(&folder));
  assert!(inside.0.wait().unwrap().code().is_none());
  assert!(beside.0.try_wait().unwrap().is_none());

  let mut removed = Sleeper::new(&["sleep", "60"], &folder);

  std::fs::remove_dir_all(&folder).unwrap();
  assert!(stop_in_folder(&folder));
  assert!(removed.0.wait().unwrap().code().is_none());
  assert!(beside.0.try_wait().unwrap().is_none());
}

#[tokio::test]
async fn a_writer_still_holding_the_lock_is_killed_rather_than_waited_for() {
  let dir = tempfile::tempdir().unwrap();
  let path = brain_lock_path(dir.path(), FOLDER);
  let queue = queue_of(&path);

  std::fs::create_dir_all(&queue).unwrap();

  let mut holder =
    Sleeper::new(&["flock", "--exclusive", path.to_str().unwrap(), "sleep", "60"], dir.path());
  let probe = OpenOptions::new().create(true).truncate(false).write(true).open(&path).unwrap();
  let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

  // A writer's ticket, timed as it stood in the queue; the reset stands its own ahead of every timed one.
  std::fs::write(queue.join(format!("{now_ms:015}-000001-{}", holder.group())), "people sort").unwrap();

  // Bounded on the holder having the lock.
  for _ in 0..100 {
    if matches!(probe.try_lock(), Err(TryLockError::WouldBlock)) {
      break;
    }

    probe.unlock().unwrap();
    std::thread::sleep(Duration::from_millis(20));
  }

  let started = Instant::now();
  let hold = hold_folder(dir.path(), FOLDER, false).await.unwrap();

  assert!(hold.lock.is_some() && hold.stopped);
  assert!(holder.0.wait().unwrap().code().is_none());
  assert!(started.elapsed() < KILL_SETTLE);
}

#[tokio::test]
async fn another_agents_editor_holding_its_own_lock_does_not_hold_the_reset_up() {
  let dir = tempfile::tempdir().unwrap();
  let chat_agent = OpenOptions::new()
    .create(true)
    .truncate(false)
    .write(true)
    .open(brain_lock_path(dir.path(), ""))
    .unwrap();

  chat_agent.try_lock().unwrap();

  let started = Instant::now();
  let hold = hold_folder(dir.path(), FOLDER, false).await.unwrap();

  assert!(hold.lock.is_some() && !hold.stopped);
  assert!(started.elapsed() < KILL_SETTLE);
}

#[tokio::test]
async fn the_reset_holds_the_lock_ahead_of_every_waiting_taker_and_fences_the_folder() {
  let dir = tempfile::tempdir().unwrap();
  let held = hold_folder(dir.path(), FOLDER, false).await.unwrap().lock.unwrap();
  let other = File::open(brain_lock_path(dir.path(), FOLDER)).unwrap();

  assert!(matches!(other.try_lock(), Err(TryLockError::WouldBlock)));
  // This process began before the fence, as a writer running through a reset did.
  assert!(fenced_out(dir.path(), FOLDER));
  assert!(!fenced_out(dir.path(), "agents/agent-c"));
  drop(held);
  assert!(other.try_lock().is_ok());
}
