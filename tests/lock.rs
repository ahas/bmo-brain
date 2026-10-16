//! The editing lock on a brain folder: its file's name, its ticket queue, and who holds it back.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bmo_brain::editors::lock::{
  NoKeep, WaitOn, brain_lock_path, queue_of, sorted_names, take_lock, takers,
};

#[test]
fn a_folders_lock_file_is_named_by_the_folder_escaped() {
  let root = Path::new("/data");

  assert_eq!(brain_lock_path(root, ""), Path::new("/data/people-digest.lock"));
  assert_eq!(
    brain_lock_path(root, "agents/agent-b"),
    Path::new("/data/people-digest.agents%2Fagent-b.lock")
  );
  assert_eq!(brain_lock_path(root, "agents/é"), Path::new("/data/people-digest.agents%2F%C3%A9.lock"));
}

#[tokio::test]
async fn a_held_lock_is_not_had_again_until_it_is_let_go() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("x.lock");
  let held = take_lock(&path, 0, "first", &NoKeep, Duration::from_millis(5)).await.unwrap().unwrap();

  assert!(
    take_lock(&path, 0, "second", &NoKeep, Duration::from_millis(5))
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(sorted_names(&queue_of(&path)).unwrap().len(), 1);

  held.release();

  assert!(sorted_names(&queue_of(&path)).unwrap().is_empty());
  assert!(
    take_lock(&path, 0, "second", &NoKeep, Duration::from_millis(5))
      .await
      .unwrap()
      .is_some()
  );
}

struct Named(std::sync::Mutex<Option<String>>);

impl WaitOn for Named {
  async fn later(&self, holder: Option<&str>) -> Option<i64> {
    *self.0.lock().unwrap() = holder.map(str::to_owned);
    None
  }
}

#[tokio::test]
async fn a_living_ticket_stood_ahead_holds_the_lock_back_and_a_dead_ones_is_taken_away() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("x.lock");
  let queue = queue_of(&path);

  fs::create_dir_all(&queue).unwrap();
  // A front ticket of this very process, which is alive, and ahead of it one of a pid no process has.
  fs::write(queue.join(format!("0-000000000000001-000001-{}", std::process::id())), "the cut").unwrap();
  fs::write(queue.join("0-000000000000000-000001-999999999"), "gone").unwrap();

  let named = Named(std::sync::Mutex::new(None));

  assert!(take_lock(&path, 0, "digest", &named, Duration::from_millis(5)).await.unwrap().is_none());
  assert_eq!(named.0.lock().unwrap().as_deref(), Some("the cut"));
  assert!(!queue.join("0-000000000000000-000001-999999999").exists());
}

#[test]
fn a_ticket_is_a_taker_only_while_the_process_that_stood_it_holds_its_pid() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("x.lock");
  let queue = queue_of(&path);
  let mut child = std::process::Command::new("sleep").arg("60").spawn().unwrap();
  let pid = child.id();
  let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

  fs::create_dir_all(&queue).unwrap();
  // Stood long before the process now under its pid began: that pid was taken since.
  fs::write(queue.join(format!("000000000000001-000001-{pid}")), "gone").unwrap();

  let reused = takers(&path);

  fs::write(queue.join(format!("0-{now_ms:015}-000002-{pid}")), "the reset").unwrap();

  let stood = takers(&path);

  child.kill().unwrap();
  child.wait().unwrap();
  assert!(reused.is_empty());
  assert_eq!(stood, vec![i64::from(pid)]);
}

#[tokio::test]
async fn a_kept_place_holds_no_one_up() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("x.lock");
  let queue = queue_of(&path);

  fs::create_dir_all(&queue).unwrap();
  fs::write(queue.join(format!("000000000000001-000001-{}", std::process::id())), "away\nkey\nkept")
    .unwrap();

  assert!(
    take_lock(&path, 0, "digest", &NoKeep, Duration::from_millis(5))
      .await
      .unwrap()
      .is_some()
  );
}
