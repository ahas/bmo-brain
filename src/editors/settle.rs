//! What an editor batch does once it stands, and what it does when it does not (docs/knowledge.md, 「When a run
//! fails」 and 「The digest run」; docs/people-book.md, 「Who reads a changed document again」): what it wrote
//! cleaned before anybody reads it, the sessions holding a stale copy told to read it again, a failure told to
//! the owner — or, where even that cannot be said, written to the failure log — and the owner's pause on the
//! digests.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use brain_support::database::{one, run};
use fragment_desk::desk::FragmentFilter;
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value, json};

use super::book::{KNOWLEDGE_ROOT, PEOPLE_ROOT, ROOMS_ROOT};
use super::fragments::libraries::{DeskLibrary, Knowledge};
use super::fragments::store;
use super::recollection::Recollection;
use crate::Failure;
use crate::ports::agency::{Agency, Reload};
use crate::ports::gate::{Gate, Sanitised};
use crate::ports::receiver::{OwnerNotice, Receiver, RoomTitle};
use crate::settings;

pub use crate::editors::commits::{Committed, commit_paths};

// When the digests are paused until, in epoch seconds, and which pause the owner was told of.
const PAUSE_KEY: &str = "digest_paused_until";
const PAUSE_NOTICED_KEY: &str = "digest_pause_noticed";

// A pause with no end is written as the first moment of 2100, and any end from 2099 on reads as "until lifted".
const OPEN_ENDED: i64 = 4_070_908_800;
const WRITTEN_OPEN: i64 = 4_102_444_800;

/// The code the owner is told a pause by.
pub const DIGEST_PAUSED: &str = "digest-paused";

fn now_seconds() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
}

/// The documents a run wrote cleaned before any session reads them: each passes the gate's document sanitiser by
/// its path relative to `root`, and one it changed is written back. A document gone by now, or one the sanitiser
/// would not or could not read, is left as it is. Answers the documents cleaned.
pub async fn clean_documents(gate: &dyn Gate, root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
  let mut cleaned = vec![];

  for path in paths {
    let Ok(before) = fs::read_to_string(path) else { continue };

    if before.trim().is_empty() {
      continue;
    }

    let named = path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned();
    let after = match gate.sanitise(&named, &before).await {
      Ok(Sanitised::Text(text)) => text,
      Ok(Sanitised::Refused(refusal)) => {
        eprintln!("document sanitiser refused {}: {}", path.display(), refusal.message);
        continue;
      }
      Err(why) => {
        eprintln!("document sanitiser failed for {}: {why}", path.display());
        continue;
      }
    };

    if after != before {
      let written = if after.ends_with('\n') { after } else { format!("{after}\n") };

      if let Err(why) = fs::write(path, written) {
        eprintln!("document sanitiser: {} could not be written back: {why}", path.display());
        continue;
      }

      println!("document sanitiser: security-intrusion material was defused in {}", path.display());
      cleaned.push(path.clone());
    }
  }

  cleaned
}

/// The sessions holding a stale copy of what a run rewrote told to read it again, through the agency's document
/// reload: the agent whose documents they are, the paths relative to the brain root, and the rooms whose sessions
/// read them. Nothing is said where no path changed.
pub async fn announce_reload(agency: &dyn Agency, agent: &str, paths: &[String], rooms: &[i64]) {
  if paths.is_empty() {
    return;
  }

  let reload = Reload {
    agent: agent.to_owned(),
    paths: paths.to_vec(),
    rooms: rooms.to_vec(),
  };

  match agency.reload_documents(&reload).await {
    Ok(true) => {}
    Ok(false) => eprintln!("reload: the agency did not take the notice"),
    Err(why) => eprintln!("reload: the sessions could not be told: {why}"),
  }
}

/// The rooms whose sessions read a document of an agent's folder, by its path relative to that folder
/// (docs/people-book.md, 「Who reads a changed document again」): a room's own document that room's; a person's
/// common document, the index excepted, every room the person has stood as a sender in; a knowledge chunk the
/// room its row is scoped to, or every room the receiver knows where its row is shared. `scopes` reads a knowledge
/// chunk's row: none where no row stands for it, some none where it is shared.
pub fn rooms_reading(
  path: &str,
  rooms: &[RoomTitle],
  scopes: &dyn Fn(&str) -> Option<Option<i64>>,
) -> Vec<i64> {
  let parts: Vec<&str> = path.split('/').collect();
  let index = parts.last().is_some_and(|name| *name == "index.md");

  match parts.as_slice() {
    [ROOMS_ROOT, chat, ..] => chat.parse::<i64>().map_or_else(|_| vec![], |chat| vec![chat]),
    [KNOWLEDGE_ROOT, _] if !index => match scopes(path) {
      Some(None) => rooms.iter().map(|room| room.chat_id).collect(),
      Some(Some(chat)) => vec![chat],
      None => vec![],
    },
    [PEOPLE_ROOT, person, rest @ ..] if !(rest.len() == 1 && index) && !rest.is_empty() => {
      let Ok(person) = person.parse::<i64>() else { return vec![] };

      rooms
        .iter()
        .filter(|room| room.senders.contains(&person))
        .map(|room| room.chat_id)
        .collect()
    }
    _ => vec![],
  }
}

/// What an editor wrote in an agent's folder, by absolute path, let go of by every session holding it and
/// announced to the sessions of the rooms that read it, the paths named from the brain root.
pub async fn reload_written(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  receiver: &dyn Receiver,
  recollection: &Recollection,
  written: &[PathBuf],
) {
  let inward: Vec<String> = written
    .iter()
    .filter_map(|path| path.strip_prefix(&recollection.brain).ok())
    .map(|path| path.to_string_lossy().into_owned())
    .collect();

  if inward.is_empty() {
    return;
  }

  let named: Vec<String> = inward.iter().map(|path| recollection.outward(path)).collect();

  // A session holding a stale copy is let go of it before the notice goes out.
  for path in &named {
    if let Err(why) =
      run(db, r#"DELETE FROM "session_people" WHERE "path" = $1"#, vec![path.as_str().into()]).await
    {
      eprintln!("reload: the sessions holding {path} could not be let go of it: {why}");
    }
  }

  let rooms = match receiver.room_titles().await {
    Ok(rooms) => rooms,
    Err(why) => {
      eprintln!("reload: the rooms could not be read, so nobody is told: {why}");
      return;
    }
  };
  let knowledge = match store::rows(db, &knowledge_rows(&recollection.agent)).await {
    Ok(rows) => rows,
    Err(why) => {
      eprintln!("reload: the knowledge rows could not be read: {why}");
      vec![]
    }
  };
  let scopes = |path: &str| {
    let outward = recollection.outward(path);

    knowledge
      .iter()
      .find(|row| row.path == outward && row.active)
      .or_else(|| knowledge.iter().find(|row| row.path == outward))
      .map(|row| row.chat_id)
  };
  let mut reading = BTreeSet::new();

  for path in &inward {
    reading.extend(rooms_reading(path, &rooms, &scopes));
  }

  announce_reload(agency, &recollection.agent, &named, &reading.into_iter().collect::<Vec<_>>()).await;
}

fn knowledge_rows(agent: &str) -> FragmentFilter {
  FragmentFilter {
    kind: Some(Knowledge.kind().to_owned()),
    agent: Some(agent.to_owned()),
    ..FragmentFilter::default()
  }
}

/// Where the whole account of a run is: the journal of the unit it ran in, or the service's own where it ran in
/// none.
pub fn journal_record(unit: Option<&str>, service_unit: &str) -> String {
  match unit {
    None => format!("the service journal (journalctl --user -u {service_unit})"),
    Some(unit) => format!("the journal of its unit (journalctl --user -u {unit})"),
  }
}

/// The cause of a failure the run could not name: that it is unknown, and where to read it.
pub fn unknown_cause(record: &str) -> String {
  format!("The cause is not known from what the run left; the whole of it is in {record}.")
}

/// A failure told to the owner through the receiver's owner notice; only where that cannot be said is the line
/// written to the failure log under the data root instead, so a failure in telling never costs what failed.
pub async fn tell_failure(
  receiver: &dyn Receiver,
  notice: &OwnerNotice,
  failure_log: &Path,
  log_line: impl FnOnce(&str) -> String,
) {
  let why = match receiver.notify_owner(notice).await {
    Ok(true) => return,
    Ok(false) => "the receiver did not take the notice".to_owned(),
    Err(why) => why.to_string(),
  };
  let line = log_line(&why);

  eprintln!("{line}");

  let appended = fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(failure_log)
    .and_then(|mut file| std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes()));

  if let Err(why) = appended {
    eprintln!("writing the failure log failed too: {why}");
  }
}

/// The moment the digests are paused until, in epoch seconds, or none while no pause holds.
pub async fn digest_paused_until(db: &DatabaseConnection) -> Result<Option<i64>, Failure> {
  let until = settings::get(db, PAUSE_KEY).await?.and_then(|value| leading_number(&value));

  Ok(until.filter(|until| *until > now_seconds()))
}

/// A number read as a leading run of digits, with its sign: what follows them is left unread.
pub fn leading_number(value: &str) -> Option<i64> {
  let value = value.trim_start();
  let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
  let length = value.len() - digits.len() + digits.chars().take_while(char::is_ascii_digit).count();

  value[..length].parse().ok()
}

/// The digests paused until the moment given, in epoch seconds, or with none until someone lifts it; the owner is
/// told of the new pause afresh.
pub async fn pause_digests(db: &DatabaseConnection, until: Option<i64>) -> Result<(), Failure> {
  settings::set(db, PAUSE_KEY, &until.unwrap_or(WRITTEN_OPEN).to_string()).await?;
  settings::forget(db, PAUSE_NOTICED_KEY).await?;

  Ok(())
}

/// The owner's pause on the digests lifted.
pub async fn resume_digests(db: &DatabaseConnection) -> Result<(), Failure> {
  settings::forget(db, PAUSE_KEY).await?;
  settings::forget(db, PAUSE_NOTICED_KEY).await?;

  Ok(())
}

/// How the digests stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseState {
  Running,
  /// Paused until the moment, in epoch seconds.
  Until(i64),
  /// Paused until someone lifts it.
  UntilLifted,
}

impl PauseState {
  /// The state a pause's end reads as, none being no pause.
  pub fn of(until: Option<i64>) -> Self {
    match until {
      None => Self::Running,
      Some(until) if until >= OPEN_ENDED => Self::UntilLifted,
      Some(until) => Self::Until(until),
    }
  }
}

/// Whether the owner's pause on the digests holds now.
pub async fn digest_paused(db: &DatabaseConnection) -> Result<bool, Failure> {
  Ok(digest_paused_until(db).await?.is_some())
}

/// One agent's person book of a room with enough waiting behind a pause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PausedRoom {
  pub chat_id: i64,
  pub title: Option<String>,
  /// The agent, by its label.
  pub agent: String,
  pub messages: i64,
}

/// What is waiting behind a pause: the notes not yet filed, and the rooms' person books with enough unread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PausedBacklog {
  pub notes: i64,
  pub rooms: Vec<PausedRoom>,
}

/// What the owner is told about a pause, so the web unit can say what the pause holds back and how to lift it:
/// its end, or null for a pause until it is lifted, and what waits behind it.
pub fn pause_notice(until: i64, backlog: &PausedBacklog) -> OwnerNotice {
  let rooms: Vec<Value> = backlog
    .rooms
    .iter()
    .map(|room| {
      json!({
        "chatId": room.chat_id,
        "title": room.title,
        "agent": room.agent,
        "messages": room.messages,
      })
    })
    .collect();
  let mut params = Map::new();

  params.insert("until".to_owned(), if until >= OPEN_ENDED { Value::Null } else { json!(until) });
  params.insert("notes".to_owned(), json!(backlog.notes));
  params.insert("rooms".to_owned(), Value::Array(rooms));

  OwnerNotice {
    code: DIGEST_PAUSED.to_owned(),
    params,
  }
}

/// The owner told, once for this pause, what it holds back: nothing is said while no pause holds, when nothing
/// waits behind it, or when this pause was told already. The pause is marked told only once the notice is taken,
/// so one that could not be is tried again at the next launch. `backlog` reads what waits.
pub async fn notice_paused_digests<F>(
  db: &DatabaseConnection,
  receiver: &dyn Receiver,
  backlog: F,
) -> Result<bool, Failure>
where
  F: AsyncFnOnce() -> Result<PausedBacklog, Failure>,
{
  let Some(until) = digest_paused_until(db).await? else { return Ok(false) };

  if settings::get(db, PAUSE_NOTICED_KEY).await?.as_deref() == Some(until.to_string().as_str()) {
    return Ok(false);
  }

  let waiting = backlog().await?;

  if waiting.notes == 0 && waiting.rooms.is_empty() {
    return Ok(false);
  }

  match receiver.notify_owner(&pause_notice(until, &waiting)).await {
    Ok(true) => {}
    Ok(false) => {
      eprintln!("digest: the receiver did not take the notice of the pause");
      return Ok(false);
    }
    Err(why) => {
      eprintln!("digest: telling the owner about the pause failed: {why}");
      return Ok(false);
    }
  }

  settings::set(db, PAUSE_NOTICED_KEY, &until.to_string()).await?;

  Ok(true)
}

/// Whether the deployment rests now, as the agency says, which no editor run starts during.
pub async fn resting(agency: &dyn Agency) -> Result<bool, Failure> {
  Ok(agency.rest().await?.resting)
}

/// The number of notes waiting for the memo digest.
pub async fn pending_note_count(db: &DatabaseConnection) -> Result<i64, Failure> {
  Ok(
    one(db, r#"SELECT COUNT(*) AS "count" FROM "notes""#, vec![])
      .await?
      .map(|row| row.try_get::<i64>("", "count"))
      .transpose()?
      .unwrap_or(0),
  )
}
