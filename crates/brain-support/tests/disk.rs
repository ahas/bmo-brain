//! A file published whole: written beside its place, renamed onto it, readable by the owner alone, and nothing
//! staged left behind.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use brain_support::disk::publish_bytes;

#[test]
fn bytes_are_published_whole_and_replace_what_stood() {
  let scratch = tempfile::tempdir().unwrap();
  let path = scratch.path().join("folder.lock.fence");

  publish_bytes(&path, b"100").unwrap();
  publish_bytes(&path, b"200").unwrap();

  assert_eq!(fs::read_to_string(&path).unwrap(), "200");
  assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
  assert!(!scratch.path().join("folder.lock.fence.new").exists());
}

#[test]
fn a_place_whose_folder_is_gone_is_refused() {
  let scratch = tempfile::tempdir().unwrap();

  assert!(publish_bytes(&scratch.path().join("gone/fence"), b"1").is_err());
}
