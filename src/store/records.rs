//! The caller's own records (docs/boundary-api.md, 「A caller's records」): a room's rules, a session's loaded
//! documents and a chat session's handoff note. Their keys are the caller's and the store reads none of them.

use bmo_brain_client::{Load, LoadsKeep, RoomRule, RuleSet};
use sqlx::PgConnection;

use super::{Held, Store, now};

/// The longest a rule's text is.
const WORDS_MOST: usize = 8_000;
/// The longest a note is.
const NOTE_MOST: usize = 64_000;
/// The longest a document's path or version is.
const DOCUMENT_MOST: usize = 4_000;
/// How long a session's loads are kept untouched, in seconds: a session that old has long been replaced.
const LOADS_KEPT_SECONDS: i64 = 7 * 86_400;

/// The rules of a room, in the order they were set.
async fn rules_of(conn: &mut PgConnection, chat_id: i64) -> Held<Vec<RoomRule>> {
  let rows: Vec<(String, i64, String)> =
    sqlx::query_as("SELECT id, subject, text FROM room_rules WHERE chat_id = $1 ORDER BY created_at, id")
      .bind(chat_id)
      .fetch_all(&mut *conn)
      .await?;

  Ok(rows.into_iter().map(|(id, subject, text)| RoomRule { id, subject, text }).collect())
}

impl Store {
  /// A room's rules.
  pub async fn room_rules(&self, chat_id: i64) -> Held<Vec<RoomRule>> {
    rules_of(&mut *self.connection().await?, chat_id).await
  }

  /// One rule set, or revoked where its words are none; the room's rules as they now stand.
  pub async fn room_rule_set(&self, asked: &RuleSet) -> Held<Vec<RoomRule>> {
    super::key("id", &asked.id)?;
    let mut tx = self.begin().await?;

    match &asked.rule {
      Some(rule) => {
        super::text("text", &rule.text, WORDS_MOST)?;
        let at = now();
        sqlx::query(
          "INSERT INTO room_rules (chat_id, id, subject, text, message_id, created_at, changed_at) \
           VALUES ($1, $2, $3, $4, $5, $6, $6) \
           ON CONFLICT (chat_id, id) DO UPDATE SET subject = EXCLUDED.subject, text = EXCLUDED.text, \
           message_id = EXCLUDED.message_id, changed_at = EXCLUDED.changed_at",
        )
        .bind(asked.chat_id)
        .bind(&asked.id)
        .bind(rule.subject)
        .bind(&rule.text)
        .bind(asked.message_id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
      }
      None => {
        sqlx::query("DELETE FROM room_rules WHERE chat_id = $1 AND id = $2")
          .bind(asked.chat_id)
          .bind(&asked.id)
          .execute(&mut *tx)
          .await?;
      }
    }

    let rules = rules_of(&mut tx, asked.chat_id).await?;
    self.commit(tx).await?;

    Ok(rules)
  }

  /// A session's loads, by path.
  pub async fn session_loads(&self, session: &str) -> Held<Vec<Load>> {
    super::text("session", session, 200)?;
    let rows: Vec<(String, String)> =
      sqlx::query_as("SELECT path, version FROM session_loads WHERE session = $1 ORDER BY path")
        .bind(session)
        .fetch_all(&mut *self.connection().await?)
        .await?;

    Ok(rows.into_iter().map(|(path, version)| Load { path, version }).collect())
  }

  /// The loads a session keeps; sessions untouched for a week are let go at the same stroke.
  pub async fn session_loads_keep(&self, asked: &LoadsKeep) -> Held<()> {
    super::text("session", &asked.session, 200)?;
    let at = asked.at.unwrap_or_else(|| now() / 1_000);
    let mut tx = self.begin().await?;

    for load in &asked.loads {
      super::text("path", &load.path, DOCUMENT_MOST)?;
      super::text("version", &load.version, DOCUMENT_MOST)?;
      sqlx::query(
        "INSERT INTO session_loads (session, path, version, loaded_at) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (session, path) DO UPDATE SET version = EXCLUDED.version, loaded_at = EXCLUDED.loaded_at",
      )
      .bind(&asked.session)
      .bind(&load.path)
      .bind(&load.version)
      .bind(at)
      .execute(&mut *tx)
      .await?;
    }

    sqlx::query("DELETE FROM session_loads WHERE loaded_at < $1")
      .bind(at - LOADS_KEPT_SECONDS)
      .execute(&mut *tx)
      .await?;
    self.commit(tx).await?;

    Ok(())
  }

  /// A session's loads let go.
  pub async fn session_loads_forget(&self, session: &str) -> Held<()> {
    super::text("session", session, 200)?;
    sqlx::query("DELETE FROM session_loads WHERE session = $1")
      .bind(session)
      .execute(&mut *self.connection().await?)
      .await?;

    Ok(())
  }

  /// The handoff note an agent's chat session left, where it left one.
  pub async fn session_note(&self, agent: &str) -> Held<Option<String>> {
    super::text("agent", agent, 200)?;
    let note: Option<String> = sqlx::query_scalar("SELECT note FROM session_notes WHERE agent = $1")
      .bind(agent)
      .fetch_optional(&mut *self.connection().await?)
      .await?;

    Ok(note)
  }

  /// A handoff note kept, overwriting the one the agent had.
  pub async fn session_note_keep(&self, agent: &str, note: &str) -> Held<()> {
    super::text("agent", agent, 200)?;
    super::text("note", note, NOTE_MOST)?;
    let at = now();
    sqlx::query(
      "INSERT INTO session_notes (agent, note, created_at, changed_at) VALUES ($1, $2, $3, $3) \
       ON CONFLICT (agent) DO UPDATE SET note = EXCLUDED.note, changed_at = EXCLUDED.changed_at",
    )
    .bind(agent)
    .bind(note)
    .bind(at)
    .execute(&mut *self.connection().await?)
    .await?;

    Ok(())
  }

  /// An agent's handoff note let go.
  pub async fn session_note_clear(&self, agent: &str) -> Held<()> {
    super::text("agent", agent, 200)?;
    sqlx::query("DELETE FROM session_notes WHERE agent = $1")
      .bind(agent)
      .execute(&mut *self.connection().await?)
      .await?;

    Ok(())
  }
}
