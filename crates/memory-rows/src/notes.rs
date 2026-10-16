//! A note tied to a room (docs/knowledge.md, 「Where a note goes」): the row, kept for the room's digest. A
//! note only lands in the brain's database; nothing is ever sent to the chat.

use brain_support::database::{Fault, one};
use brain_support::js;
use sea_orm::DatabaseConnection;

use crate::memories::{Answered, Refused};

/// What a note is kept with, as the refusal of a call missing any of it says.
pub const NOTE_USAGE: &str = "note_add takes text, what to keep; and optionally author, the user id it came \
                              from; global, to keep it for every room; chat, a room other than the turn's; \
                              and message";

/// What a note is kept with.
pub struct NoteAsked<'a> {
  /// The agent keeping it, whose recollectors take it (docs/agent-ownership.md).
  pub agent: &'a str,
  /// The room it was said in, which files it even when it is kept for every room.
  pub chat_id: i64,
  pub text: &'a str,
  pub author: Option<i64>,
  /// Kept for every room: a way of working, know-how or a fact that holds wherever the work is done.
  pub shared: bool,
}

/// A note kept: its line, or why none was kept. `now` is in unix seconds.
pub async fn note(db: &DatabaseConnection, asked: &NoteAsked<'_>, now: i64) -> Result<Answered, Fault> {
  let text = js::trim(asked.text);

  if text.is_empty() {
    return Ok(Err(Refused::coded("usage", vec![("verb", "note_add".to_owned())], NOTE_USAGE)));
  }

  let Some(row) = one(
    db,
    r#"INSERT INTO "notes" ("agent", "chat_id", "author", "text", "shared", "created_at") VALUES ($1, $2, $3, $4, $5, $6) RETURNING "id""#,
    vec![
      asked.agent.into(),
      asked.chat_id.into(),
      asked.author.into(),
      text.into(),
      i64::from(asked.shared).into(),
      now.into(),
    ],
  )
  .await?
  else {
    return Ok(Err(Refused::of("the note was not kept")));
  };
  let id: i64 = row.try_get("", "id")?;

  Ok(Ok(vec![if asked.shared {
    format!("note {id} kept for every room, from chat {}", asked.chat_id)
  } else {
    format!("note {id} kept for chat {}", asked.chat_id)
  }]))
}
