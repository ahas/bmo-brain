//! What the brain's editors read and mark: the notes waiting for the memo digest, each agent's unread
//! messages of a room and the marks its person book leaves on them, which the receiver keeps
//! (docs/people-book.md, 「Each agent's run」), the day a room's book starts reading from, which the brain's
//! settings keep, and the rooms' titles.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use brain_support::database::{all, run};
use sea_orm::{DatabaseConnection, QueryResult};

use crate::Failure;
use crate::editors::book::ROOMS_ROOT;
pub use crate::ports::receiver::Message;
use crate::ports::receiver::{Receiver, RoomSince};
use crate::settings;

/// One note waiting for the memo digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
  pub id: i64,
  /// The designated agent that kept it, whose recollectors take it.
  pub agent: String,
  /// The room it was said in, whose batch files it.
  pub chat_id: i64,
  pub author: Option<i64>,
  pub text: String,
  /// Kept for every room rather than for its own.
  pub shared: bool,
  pub created_at: i64,
}

fn note_of(row: &QueryResult) -> Result<Note, Failure> {
  Ok(Note {
    id: row.try_get("", "id")?,
    agent: row.try_get("", "agent")?,
    chat_id: row.try_get("", "chat_id")?,
    author: row.try_get("", "author")?,
    text: row.try_get("", "text")?,
    shared: row.try_get::<Option<i64>>("", "shared")?.unwrap_or(0) == 1,
    created_at: row.try_get("", "created_at")?,
  })
}

/// The notes waiting, oldest first: one agent's, or every agent's where none is named.
pub async fn pending_notes(db: &DatabaseConnection, agent: Option<&str>) -> Result<Vec<Note>, Failure> {
  let rows = match agent {
    None => {
      all(
        db,
        r#"SELECT "id", "agent", "chat_id", "author", "text", "shared", "created_at" FROM "notes" ORDER BY "id""#,
        vec![],
      )
      .await?
    }
    Some(agent) => {
      all(
        db,
        r#"SELECT "id", "agent", "chat_id", "author", "text", "shared", "created_at" FROM "notes"
           WHERE "agent" = $1 ORDER BY "id""#,
        vec![agent.into()],
      )
      .await?
    }
  };

  rows.iter().map(note_of).collect()
}

/// The notes a batch handed over taken off, once it stands.
pub async fn delete_notes(db: &DatabaseConnection, ids: &[i64]) -> Result<(), Failure> {
  for id in ids {
    run(db, r#"DELETE FROM "notes" WHERE "id" = $1"#, vec![(*id).into()]).await?;
  }

  Ok(())
}

/// Whether a person said anything in a room between two moments, in epoch seconds.
pub async fn spoke_between(
  receiver: &dyn Receiver,
  chat_id: i64,
  sender_id: i64,
  from: i64,
  to: i64,
) -> Result<bool, Failure> {
  Ok(receiver.spoke_between(chat_id, sender_id, from, to).await?)
}

/// A room's title, where the receiver knows one.
pub async fn chat_title(receiver: &dyn Receiver, chat_id: i64) -> Result<Option<String>, Failure> {
  let rooms = receiver.room_titles().await?;

  Ok(rooms.into_iter().find(|room| room.chat_id == chat_id).and_then(|room| room.title))
}

/// Every room title the receiver holds, which a common document may not carry.
pub async fn chat_titles(receiver: &dyn Receiver) -> Result<Vec<String>, Failure> {
  let rooms = receiver.room_titles().await?;

  Ok(rooms.into_iter().filter_map(|room| room.title).collect())
}

/// The name a person's common book is headed with: the name their latest message went under, as the receiver's
/// display names say, where it names one.
pub async fn display_name(receiver: &dyn Receiver, person: i64) -> Result<Option<String>, Failure> {
  let names = receiver.display_names(&[person]).await?;

  Ok(names.into_iter().find(|named| named.person == person).and_then(|named| named.name))
}

/// What every key of the day an agent's book of a room starts reading from begins with; the agent and the
/// room follow it, each ending in its own part.
pub const DIGEST_SINCE: &str = "people_digest_since:";

/// The settings key of the day an agent's book of a room starts reading from.
pub fn since_key(agent: &str, chat_id: i64) -> String {
  format!("{DIGEST_SINCE}{agent}:{chat_id}")
}

/// The messages of every room not in an agent's person book yet, and how many, by room.
pub async fn unread_counts(
  db: &DatabaseConnection,
  receiver: &dyn Receiver,
  agent: &str,
) -> Result<BTreeMap<i64, i64>, Failure> {
  let mut since = Vec::new();

  for (room, value) in settings::prefixed(db, &format!("{DIGEST_SINCE}{agent}:")).await? {
    if let (Ok(chat_id), Ok(since_at)) = (room.parse::<i64>(), value.trim().parse::<i64>()) {
      since.push(RoomSince {
        chat_id,
        since: since_at,
      });
    }
  }

  let rooms = receiver.unread_counts(agent, &since).await?;

  Ok(rooms.into_iter().map(|room| (room.chat_id, room.count)).collect())
}

/// Whether a room keeps a room part of the person book: every room but a one-to-one with the chat agent,
/// where the person talks with it alone, so the TODO, what runs and the person's common book already say
/// what such a part would.
pub async fn keeps_room_book(receiver: &dyn Receiver, chat_id: i64) -> Result<bool, Failure> {
  Ok(!receiver.roads(chat_id).await?.one_to_one)
}

/// Whether an agent's book is written from a room: one it answers, keeping a room part.
pub async fn book_room(receiver: &dyn Receiver, agent: &str, chat_id: i64) -> Result<bool, Failure> {
  let rooms = receiver.room_answerers(Some(chat_id)).await?;
  let answers = rooms
    .iter()
    .any(|room| room.chat_id == chat_id && room.agents.iter().any(|answerer| answerer == agent));

  Ok(answers && keeps_room_book(receiver, chat_id).await?)
}

/// The rooms of a brain folder's books that keep no room part: what stands under them is left as it is,
/// never written, sorted, cut or trimmed.
pub async fn unkept_rooms(receiver: &dyn Receiver, folder: &Path) -> Result<BTreeSet<i64>, Failure> {
  let mut unkept = BTreeSet::new();

  for entry in std::fs::read_dir(folder.join(ROOMS_ROOT)).into_iter().flatten().flatten() {
    let Ok(chat_id) = entry.file_name().to_string_lossy().parse::<i64>() else { continue };

    if !keeps_room_book(receiver, chat_id).await? {
      unkept.insert(chat_id);
    }
  }

  Ok(unkept)
}

/// What every room an agent's book is written from has waiting for it: a room it does not answer is not its
/// to read, and a one-to-one with the chat agent keeps no room part.
pub async fn people_counts(
  db: &DatabaseConnection,
  receiver: &dyn Receiver,
  agent: &str,
) -> Result<BTreeMap<i64, i64>, Failure> {
  let mut counts = BTreeMap::new();

  for (chat_id, count) in unread_counts(db, receiver, agent).await? {
    if book_room(receiver, agent, chat_id).await? {
      counts.insert(chat_id, count);
    }
  }

  Ok(counts)
}

/// The oldest messages of a room not in an agent's book yet, from the day its book of the room starts.
pub async fn unread_messages(
  db: &DatabaseConnection,
  receiver: &dyn Receiver,
  agent: &str,
  chat_id: i64,
  limit: i64,
) -> Result<Vec<Message>, Failure> {
  let since = reading_since(db, agent, chat_id).await?;

  Ok(receiver.unread_messages(agent, chat_id, limit, since).await?)
}

/// The messages of a room already folded into an agent's book, oldest first, after the one given.
pub async fn read_messages(
  receiver: &dyn Receiver,
  agent: &str,
  chat_id: i64,
  after: i64,
  limit: i64,
) -> Result<Vec<Message>, Failure> {
  Ok(receiver.read_messages(agent, chat_id, after, limit).await?)
}

/// How many of a room's messages are folded into an agent's book.
pub async fn read_count(receiver: &dyn Receiver, agent: &str, chat_id: i64) -> Result<i64, Failure> {
  Ok(receiver.read_count(agent, chat_id).await?)
}

/// A batch's messages marked as read into an agent's book.
pub async fn mark_read(
  receiver: &dyn Receiver,
  agent: &str,
  chat_id: i64,
  message_ids: &[i64],
) -> Result<(), Failure> {
  receiver.keep_read_marks(agent, chat_id, message_ids).await?;

  Ok(())
}

/// Every mark of an agent's book taken off a room's messages, or off every room's; how many lost their mark.
pub async fn forget_read(
  receiver: &dyn Receiver,
  agent: &str,
  chat_id: Option<i64>,
) -> Result<u64, Failure> {
  let forgotten = receiver.forget_read_marks(agent, chat_id).await?;

  Ok(u64::try_from(forgotten).unwrap_or(0))
}

/// The day an agent's book of a room starts reading from, in epoch seconds.
pub async fn reading_since(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
) -> Result<Option<i64>, Failure> {
  let value = settings::get(db, &since_key(agent, chat_id)).await?;

  Ok(value.and_then(|value| value.trim().parse::<i64>().ok()))
}

/// The day an agent's book of a room starts reading from set, or taken off with none.
pub async fn set_reading_since(
  db: &DatabaseConnection,
  agent: &str,
  chat_id: i64,
  moment: Option<i64>,
) -> Result<(), Failure> {
  match moment {
    None => settings::forget(db, &since_key(agent, chat_id)).await?,
    Some(moment) => settings::set(db, &since_key(agent, chat_id), &moment.to_string()).await?,
  }

  Ok(())
}

/// The number an agent's own words stand under in a room, about which no book is written, as the receiver
/// answers it; nought where it answers none.
pub async fn own_number_in(receiver: &dyn Receiver, agent: &str, chat_id: i64) -> Result<i64, Failure> {
  Ok(receiver.own_number(agent, chat_id).await?.unwrap_or(0))
}
