//! The owner's room rules (docs/boundary-api.md, 「Owner room rules」): each room's rules one document under the data
//! root, every rule naming the sender it grants and what that person may ask for. A rule is replaced or removed by
//! its id, the room's document locked for the change and published whole, so a reader never sees half of one.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use brain_support::disk::publish_bytes;
use brain_support::js::{self, MAX_SAFE_INTEGER};
use serde::{Deserialize, Serialize};

use crate::Failure;

/// The folder under the data root the rooms' documents stand in.
pub const ROOM_RULES_DIR: &str = "room-rules";

/// The longest a rule's text may be, in the units a JavaScript string counts.
pub const TEXT_MOST: usize = 8000;

/// One rule: its id, the sender it grants, and what that sender may ask for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomRule {
  pub id: String,
  pub subject: i64,
  pub text: String,
}

/// Why a change was not made.
#[derive(Debug, PartialEq, Eq)]
pub enum Unchanged {
  /// The id, the subject or the text is not of the shape a rule takes; the sentence says which.
  Invalid(String),
  /// Another change of the room holds its lock.
  Busy,
  /// The document could not be read or written.
  Failed(Failure),
}

/// Where a room's document stands under the data root.
pub fn rules_path(data_root: &Path, chat_id: i64) -> PathBuf {
  data_root.join(ROOM_RULES_DIR).join(format!("{chat_id}.json"))
}

fn disk(path: &Path, why: impl std::fmt::Display) -> Failure {
  Failure::Disk(format!("{}: {why}", path.display()))
}

/// A room's rules in the order they were set; none for a room with no document. A document that is not a list of
/// rules is a failure, never read as empty.
pub fn read_rules(data_root: &Path, chat_id: i64) -> Result<Vec<RoomRule>, Failure> {
  let path = rules_path(data_root, chat_id);
  let text = match fs::read_to_string(&path) {
    Ok(text) => text,
    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(vec![]),
    Err(error) => return Err(disk(&path, error)),
  };
  let rows: Vec<RoomRule> = serde_json::from_str(&text)
    .map_err(|why| disk(&path, format!("invalid room rules document: {why}")))?;

  if rows.iter().any(|row| row.subject.abs() > MAX_SAFE_INTEGER) {
    return Err(disk(&path, "invalid room rules document: a subject is no safe whole number"));
  }

  Ok(rows)
}

/// Why an id cannot name a rule: 1 to 80 letters, digits, `_` or `-`.
pub fn id_problem(id: &str) -> Option<String> {
  let shaped = (1..=80).contains(&id.len())
    && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');

  (!shaped).then(|| "invalid rule id: 1 to 80 letters, digits, _ or -".to_owned())
}

/// Why a subject and text cannot make a rule: a safe whole number, and a text not blank and within its length.
pub fn rule_problem(subject: i64, text: &str) -> Option<String> {
  if subject.abs() > MAX_SAFE_INTEGER {
    return Some("invalid rule: the subject is no safe whole number".to_owned());
  }

  if js::trim(text).is_empty() || js::units(text) > TEXT_MOST {
    return Some(format!("invalid rule: the text is blank or longer than {TEXT_MOST} characters"));
  }

  None
}

// The room's lock, let go however the change ends.
struct Held(PathBuf);

impl Drop for Held {
  fn drop(&mut self) {
    let _ = fs::remove_file(&self.0);
  }
}

/// One rule of a room replaced by `rule`, or removed where none is given, leaving nothing behind: the room's rules
/// as they then stand. The owner check is the caller's, made before this is called.
pub fn change_rule(
  data_root: &Path,
  chat_id: i64,
  id: &str,
  rule: Option<(i64, &str)>,
) -> Result<Vec<RoomRule>, Unchanged> {
  if let Some(problem) = id_problem(id) {
    return Err(Unchanged::Invalid(problem));
  }

  if let Some(problem) = rule.and_then(|(subject, text)| rule_problem(subject, text)) {
    return Err(Unchanged::Invalid(problem));
  }

  let path = rules_path(data_root, chat_id);
  let folder = path.parent().unwrap_or(data_root);

  fs::create_dir_all(folder).map_err(|why| Unchanged::Failed(disk(folder, why)))?;

  let mut lock = path.as_os_str().to_owned();

  lock.push(".lock");

  let lock = PathBuf::from(lock);
  let _held = match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&lock) {
    Ok(_) => Held(lock),
    Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(Unchanged::Busy),
    Err(error) => return Err(Unchanged::Failed(disk(&lock, error))),
  };
  let mut rows: Vec<RoomRule> = read_rules(data_root, chat_id)
    .map_err(Unchanged::Failed)?
    .into_iter()
    .filter(|row| row.id != id)
    .collect();

  if let Some((subject, text)) = rule {
    rows.push(RoomRule {
      id: id.to_owned(),
      subject,
      text: js::trim(text).to_owned(),
    });
  }

  let document =
    serde_json::to_string_pretty(&rows).map_err(|why| Unchanged::Failed(disk(&path, why)))? + "\n";

  publish_bytes(&path, document.as_bytes()).map_err(|why| Unchanged::Failed(disk(&path, why)))?;

  Ok(rows)
}
