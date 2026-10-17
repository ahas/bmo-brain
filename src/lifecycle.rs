//! A room and an agent leaving the brain (docs/boundary-api.md, 「Rooms leaving」 and 「Agents forgetting」): what a
//! room holds of the brain counted and removed, an agent's memory counted and reset, and an agent's editing lock
//! forgotten once nothing writes its folder any more. A reset goes before the agent's writer, never waiting on it:
//! every program still working in its folder killed, its folder fenced and its editing lock taken first.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use brain_support::database::{one, run};
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::Serialize;

use crate::Failure;
use crate::editors::archive::{DIGEST_SINCE, set_reading_since};
use crate::editors::book::{remove_room_books, room_books};
use crate::editors::commits::{commit_removal, put_back, remove};
use crate::editors::lock::{self, HeldLock, NoKeep, brain_lock_path};
use crate::ports::agency::Agency;
use crate::ports::receiver::Receiver;
use crate::settings;

// The name a reset stands in the lock's queue under.
const HOLDER: &str = "companion reset";

// How long a killed process is given to let the lock go, and how often the lock is tried meanwhile.
const KILL_SETTLE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);

// How many takers of the lock are killed, one after another, before the reset goes on without it.
const TAKERS_KILLED: usize = 5;

// The tables a room's rows stand in, each with the condition naming the room.
const ROOM_TABLES: [(&str, &str); 5] = [
  ("memories", r#""chat_id" = $1"#),
  ("fragments", r#""chat_id" = $1"#),
  ("notes", r#""chat_id" = $1"#),
  ("reminders", r#""chat_id" = $1"#),
  ("watches", r#""chat_id" = $1 OR "by_chat" = $1"#),
];

// The tables an agent's memory stands in, in the order a reset takes them.
const AGENT_TABLES: [&str; 5] = ["memories", "reminders", "watches", "notes", "fragments"];

/// What a room holds of the brain, as its removal counts it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RoomHolding {
  pub rows: BTreeMap<String, i64>,
  pub settings: Vec<String>,
  pub folders: Vec<String>,
}

async fn counted(
  db: &DatabaseConnection,
  table: &str,
  condition: &str,
  value: sea_orm::Value,
) -> Result<i64, Failure> {
  let row =
    one(db, &format!(r#"SELECT COUNT(*) AS "n" FROM "{table}" WHERE {condition}"#), vec![value]).await?;

  Ok(row.map(|row| row.try_get::<i64>("", "n")).transpose()?.unwrap_or(0))
}

// A path under the brain root, as an answer names it.
fn relative(brain: &Path, path: &Path) -> String {
  path.strip_prefix(brain).unwrap_or(path).to_string_lossy().into_owned()
}

/// What a room holds of the brain: its rows per table, the settings keys naming it, and its folder under each
/// agent's, with the folders as directories.
pub async fn room_holding(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  brain: &Path,
  chat_id: i64,
) -> Result<(RoomHolding, Vec<PathBuf>), Failure> {
  let mut holding = RoomHolding::default();

  for (table, condition) in ROOM_TABLES {
    holding
      .rows
      .insert(table.to_owned(), counted(db, table, condition, chat_id.into()).await?);
  }

  let suffix = format!(":{chat_id}");

  holding.settings = settings::prefixed(db, DIGEST_SINCE)
    .await?
    .into_iter()
    .filter(|(rest, _)| rest.ends_with(&suffix))
    .map(|(rest, _)| format!("{DIGEST_SINCE}{rest}"))
    .collect();

  let folders = room_books(agency, brain, chat_id).await?;

  holding.folders = folders.iter().map(|path| relative(brain, path)).collect();

  Ok((holding, folders))
}

/// A room's holding removed: its rows and keys in one write, then each folder removed and its removal committed
/// under the lock of the agent's folder it stands in. What could not be removed, each said.
pub async fn remove_room(
  db: &DatabaseConnection,
  data_root: &Path,
  brain: &Path,
  chat_id: i64,
  holding: &RoomHolding,
  folders: &[PathBuf],
) -> Result<Vec<String>, Failure> {
  let transaction = db.begin().await?;

  for (table, condition) in ROOM_TABLES {
    run(
      &transaction,
      &format!(r#"DELETE FROM "{table}" WHERE {condition}"#),
      vec![chat_id.into()],
    )
    .await?;
  }

  for key in &holding.settings {
    run(&transaction, r#"DELETE FROM "settings" WHERE "key" = $1"#, vec![key.clone().into()]).await?;
  }

  transaction.commit().await?;

  let mut could_not = vec![];

  for path in folders {
    if let Err(why) = remove_room_books(data_root, brain, path).await {
      could_not.push(format!("{}: {why}", relative(brain, path)));
    }
  }

  Ok(could_not)
}

/// What an agent's reset finds to take: its rows per table, and its folder relative to the brain root where it
/// stands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AgentHolding {
  pub rows: BTreeMap<String, i64>,
  pub folder: Option<String>,
}

/// The agent's folder as the agency registers it, relative to the brain root: empty for the brain root, none where
/// the agency registers no such agent.
pub async fn agent_folder(agency: &dyn Agency, agent: &str) -> Result<Option<String>, Failure> {
  Ok(
    agency
      .registrations(Some(agent))
      .await?
      .into_iter()
      .find(|registration| registration.name == agent)
      .map(|registration| registration.brain_folder.trim_end_matches('/').to_owned()),
  )
}

/// What an agent holds of the brain, its folder named where the agency registers one and it stands.
pub async fn agent_holding(
  db: &DatabaseConnection,
  brain: &Path,
  agent: &str,
  folder: Option<&str>,
) -> Result<AgentHolding, Failure> {
  let mut holding = AgentHolding::default();

  for table in AGENT_TABLES {
    holding
      .rows
      .insert(table.to_owned(), counted(db, table, r#""agent" = $1"#, agent.into()).await?);
  }

  holding.folder = folder.filter(|folder| brain.join(folder).exists()).map(str::to_owned);

  Ok(holding)
}

// A process killed with its process group, or alone where it stands in this process's own group.
fn kill_taker(pid: i64) {
  let Ok(pid) = i32::try_from(pid) else { return };
  // SAFETY: getpgid reads a process's group and touches no memory.
  let (group, own) = unsafe { (libc::getpgid(pid), libc::getpgid(0)) };

  // SAFETY: the group or the process is the one named, and a signal to one gone since reaches nobody.
  unsafe {
    if group > 1 && group != own {
      libc::kill(-group, libc::SIGKILL);
    } else {
      libc::kill(pid, libc::SIGKILL);
    }
  }
}

// Whether a working directory lies in the folder, or in a folder of its name removed since, which the kernel names
// with a mark after it.
fn works_in(cwd: &Path, folder: &Path) -> bool {
  let read = cwd.to_string_lossy();

  Path::new(read.strip_suffix(" (deleted)").unwrap_or(&read)).starts_with(folder)
}

/// Every program working in the folder killed with its process group, or alone where it stands in this process's
/// own group: an editor run stands apart from the run that sent it out and holds words the reset takes. A folder
/// of its name removed since counts as the folder. Whether any was killed.
pub fn stop_in_folder(folder: &Path) -> bool {
  // Read as the kernel names working directories, through whatever links lead to the folder.
  let folder = match (folder.parent().and_then(|parent| parent.canonicalize().ok()), folder.file_name()) {
    (Some(parent), Some(name)) => parent.join(name),
    _ => folder.to_path_buf(),
  };
  let Ok(entries) = std::fs::read_dir("/proc") else {
    return false;
  };
  let own = std::process::id();
  let mut killed = false;

  for entry in entries.flatten() {
    let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else {
      continue;
    };

    if pid != own && std::fs::read_link(entry.path().join("cwd")).is_ok_and(|cwd| works_in(&cwd, &folder)) {
      kill_taker(pid.into());
      killed = true;
    }
  }

  killed
}

/// The folder as a reset holds it: its editing lock, none where it could not be had, and whether a taker of the
/// lock had to be killed for it.
pub struct Hold {
  pub lock: Option<HeldLock>,
  pub stopped: bool,
}

/// The folder fenced and its editing lock taken first in its queue. Where it is held, the lock is given a moment to
/// come free from the programs just killed, then the first taker standing in the queue is killed, one at a time and
/// the queue read afresh each time, until it is had; none where it still is not, which the reset goes on without,
/// the fence keeping every writer begun before it from writing in the folder again.
pub async fn hold_folder(data_root: &Path, folder: &str, settle: bool) -> std::io::Result<Hold> {
  lock::fence(data_root, folder)?;

  let path = brain_lock_path(data_root, folder);
  let mut until = tokio::time::Instant::now() + if settle { KILL_SETTLE } else { Duration::ZERO };
  let mut stopped = false;

  for round in 0..=TAKERS_KILLED {
    loop {
      if let Some(held) = lock::take_lock_first(&path, 0, HOLDER, &NoKeep, POLL).await? {
        return Ok(Hold {
          lock: Some(held),
          stopped,
        });
      }

      if tokio::time::Instant::now() >= until {
        break;
      }

      tokio::time::sleep(POLL).await;
    }

    let Some(pid) = lock::takers(&path).into_iter().next().filter(|_| round < TAKERS_KILLED) else {
      break;
    };

    kill_taker(pid);
    stopped = true;
    until = tokio::time::Instant::now() + KILL_SETTLE;
  }

  Ok(Hold {
    lock: None,
    stopped,
  })
}

/// What an agent's reset stands on.
pub struct Reset<'a> {
  pub db: &'a DatabaseConnection,
  pub receiver: &'a dyn Receiver,
  pub brain: &'a Path,
  pub data_root: &'a Path,
  pub agent: &'a str,
  /// The agent's folder as the agency registers it, none where it registers none.
  pub folder: Option<&'a str>,
  pub now: i64,
}

/// An agent's memory reset: its writer put out of the way, its rows removed, its read marks forgotten, its book of
/// every room it answers made to read from now, its folder removed and the removal committed. What it could not
/// do, each said.
pub async fn reset_agent(reset: &Reset<'_>) -> Result<Vec<String>, Failure> {
  let mut could_not = vec![];
  let lock = match reset.folder {
    Some(folder) => {
      let killed = stop_in_folder(&reset.brain.join(folder));
      let hold = hold_folder(reset.data_root, folder, killed)
        .await
        .map_err(|why| Failure::Disk(why.to_string()))?;

      if hold.lock.is_none() {
        could_not.push(format!(
          "{}: the editing lock was not had; the reset went on without it",
          relative(reset.data_root, &brain_lock_path(reset.data_root, folder))
        ));
      }

      // Put back only under the lock: a writer holding it was begun after the fence, and its files are its own.
      if hold.lock.is_some()
        && (killed || hold.stopped)
        && let Err(said) = put_back(reset.brain, folder)
      {
        could_not.push(format!("{folder}: what a stopped run left was not put back: {said}"));
      }

      hold.lock
    }
    None => None,
  };
  let transaction = reset.db.begin().await?;

  for table in AGENT_TABLES {
    run(
      &transaction,
      &format!(r#"DELETE FROM "{table}" WHERE "agent" = $1"#),
      vec![reset.agent.into()],
    )
    .await?;
  }

  transaction.commit().await?;

  if let Err(why) = reset.receiver.forget_read_marks(reset.agent, None).await {
    could_not.push(format!("the read marks were not forgotten: {why}"));
  }

  // Its book of every room it answers reads from now on, so nothing said before comes back.
  match reset.receiver.room_answerers(None).await {
    Ok(rooms) => {
      for room in rooms.iter().filter(|room| room.agents.iter().any(|agent| agent == reset.agent)) {
        set_reading_since(reset.db, reset.agent, room.chat_id, Some(reset.now)).await?;
      }
    }
    Err(why) => could_not.push(format!("the rooms it answers were not read: {why}")),
  }

  if let Some(folder) = reset.folder {
    let path = reset.brain.join(folder);

    if let Err(why) = remove(&path)
      && why.kind() != std::io::ErrorKind::NotFound
    {
      could_not.push(format!("{folder}: {why}"));
    }

    if reset.brain.join(".git").exists()
      && let Err(said) = commit_removal(reset.brain, folder, &format!("{}'s memory is reset", reset.agent))
    {
      could_not.push(format!("brain commit: {said}"));
    }

    // Fenced again as the lock goes, so a writer launched while the reset ran, which may have read what the reset
    // took, does not write it back.
    if let Err(why) = lock::fence(reset.data_root, folder) {
      could_not.push(format!("{folder}: the fence was not set again: {why}"));
    }
  }

  if let Some(lock) = lock {
    lock.release();
  }

  Ok(could_not)
}
