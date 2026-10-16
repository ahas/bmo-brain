//! The brain's own repository as the editors write it: what a run wrote committed by the paths it names, a
//! folder put back to its last commit, and a removed folder's removal committed.

use std::fs;
use std::path::Path;

use bmo_brain::editors::book::remove_room_books;
use bmo_brain::editors::commits::{Committed, commit_paths, commit_removal, git, put_back};

fn repository(root: &Path) {
  for args in [
    &["init", "-q"][..],
    &["config", "user.email", "tester@example.com"],
    &["config", "user.name", "tester"],
  ] {
    git(root, args).unwrap();
  }
}

#[tokio::test]
async fn a_commit_takes_the_named_paths_alone_and_nothing_where_nothing_changed() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  repository(root);
  fs::create_dir_all(root.join("rooms/1/rules")).unwrap();
  fs::write(root.join("rooms/1/rules/a.md"), "a\n").unwrap();
  fs::write(root.join("beside.md"), "b\n").unwrap();
  fs::create_dir_all(root.join("empty")).unwrap();

  let named = ["rooms/1/rules".to_owned(), "empty".to_owned()];

  assert_eq!(commit_paths(&named, "first", root).await, Committed::Committed);
  assert_eq!(commit_paths(&named, "again", root).await, Committed::Nothing);
  assert_eq!(git(root, &["status", "--porcelain"]).unwrap(), "?? beside.md");
  assert_eq!(commit_paths(&["empty".to_owned()], "none", root).await, Committed::Nothing);
}

#[test]
fn a_folder_is_put_back_to_its_last_commit_and_its_removal_committed() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  repository(root);
  fs::create_dir_all(root.join("agents/agent-b/people")).unwrap();
  fs::write(root.join("agents/agent-b/people/a.md"), "kept\n").unwrap();
  git(root, &["add", "--all"]).unwrap();
  git(root, &["commit", "-q", "-m", "first"]).unwrap();

  fs::write(root.join("agents/agent-b/people/a.md"), "half written\n").unwrap();
  fs::write(root.join("agents/agent-b/people/new.md"), "left behind\n").unwrap();
  put_back(root, "agents/agent-b").unwrap();

  assert_eq!(fs::read_to_string(root.join("agents/agent-b/people/a.md")).unwrap(), "kept\n");
  assert!(!root.join("agents/agent-b/people/new.md").exists());
  assert_eq!(put_back(root, "agents/gone"), Ok(()));

  fs::remove_dir_all(root.join("agents/agent-b")).unwrap();

  assert_eq!(commit_removal(root, "agents/agent-b", "agent-b is removed"), Ok("committed"));
  assert_eq!(commit_removal(root, "agents/agent-b", "again"), Ok("nothing"));
  assert_eq!(git(root, &["status", "--porcelain"]).unwrap(), "");
}

#[tokio::test]
async fn a_rooms_books_are_removed_under_their_folders_lock_and_the_removal_committed() {
  let data = tempfile::tempdir().unwrap();
  let dir = tempfile::tempdir().unwrap();
  let brain = dir.path();

  repository(brain);
  fs::create_dir_all(brain.join("agents/agent-b/rooms/-5/people")).unwrap();
  fs::write(brain.join("agents/agent-b/rooms/-5/people/4.md"), "sheet\n").unwrap();
  git(brain, &["add", "--all"]).unwrap();
  git(brain, &["commit", "-q", "-m", "first"]).unwrap();

  remove_room_books(data.path(), brain, &brain.join("agents/agent-b/rooms/-5"))
    .await
    .unwrap();

  assert!(!brain.join("agents/agent-b/rooms/-5").exists());
  assert_eq!(git(brain, &["status", "--porcelain"]).unwrap(), "");
  assert!(
    git(brain, &["log", "-1", "--format=%s"])
      .unwrap()
      .contains("room -5's registration is removed")
  );
  assert!(remove_room_books(data.path(), brain, Path::new("/elsewhere/rooms/-5")).await.is_err());
}
