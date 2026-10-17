//! The memory rows a turn's gate reads and the facts it hands back (docs/memories.md, 「The three roads a row
//! reaches a turn by」 and 「The two closings」): every read is one agent's rows. A row with no condition stands in
//! front of the turn as it is, a state nobody has moved for long enough released first; a row with a condition
//! is the matcher's candidate, the ones that have waited too long withered first; a row waiting on an event is
//! woken by the event alone, an errand among them stamped as met. Situation rules are rows like any other, read by
//! their condition or their event.

use std::collections::HashMap;
use std::sync::Mutex;

use brain_support::database::{all, one, run};
use brain_support::policy::fill_marks;
use memory_rows::events::{Event, event_wakes, read_event};
use sea_orm::{DatabaseConnection, QueryResult};
use serde::Serialize;

use crate::Failure;
use crate::policy::BRAIN;
use crate::ports::receiver::RoadKind;

/// One memory row, whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRow {
  pub id: i64,
  pub agent: String,
  pub slug: String,
  #[serde(rename = "type")]
  pub kind: String,
  pub chat_id: Option<i64>,
  pub reach: String,
  pub condition: String,
  pub action: String,
  pub for_user: Option<i64>,
  pub message_id: Option<i64>,
  pub todo_id: Option<i64>,
  pub set_by: Option<i64>,
  pub set_at: i64,
  pub active: bool,
  pub last_hit_at: Option<i64>,
  pub hits: i64,
  pub done_at: Option<i64>,
  pub done_text: Option<String>,
  pub down_reason: Option<String>,
  pub down_at: Option<i64>,
  pub candidate_why: Option<String>,
  pub candidate_at: Option<i64>,
  pub event: Option<Event>,
  pub event_met_at: Option<i64>,
  pub moved_at: Option<i64>,
}

/// A memory row read whole off the table.
pub fn row_of(row: &QueryResult) -> Result<MemoryRow, Failure> {
  Ok(MemoryRow {
    id: row.try_get("", "id")?,
    agent: row.try_get("", "agent")?,
    slug: row.try_get("", "slug")?,
    kind: row.try_get("", "type")?,
    chat_id: row.try_get("", "chat_id")?,
    reach: row.try_get("", "reach")?,
    condition: row.try_get("", "condition")?,
    action: row.try_get("", "action")?,
    for_user: row.try_get("", "for_user")?,
    message_id: row.try_get("", "message_id")?,
    todo_id: row.try_get("", "todo_id")?,
    set_by: row.try_get("", "set_by")?,
    set_at: row.try_get("", "set_at")?,
    active: row.try_get::<i64>("", "active")? != 0,
    last_hit_at: row.try_get("", "last_hit_at")?,
    hits: row.try_get("", "hits")?,
    done_at: row.try_get("", "done_at")?,
    done_text: row.try_get("", "done_text")?,
    down_reason: row.try_get("", "down_reason")?,
    down_at: row.try_get("", "down_at")?,
    candidate_why: row.try_get("", "candidate_why")?,
    candidate_at: row.try_get("", "candidate_at")?,
    event: read_event(&row.try_get::<String>("", "event")?),
    event_met_at: row.try_get("", "event_met_at")?,
    moved_at: row.try_get("", "moved_at")?,
  })
}

async fn memories(
  db: &DatabaseConnection,
  sql: &str,
  values: Vec<sea_orm::Value>,
) -> Result<Vec<MemoryRow>, Failure> {
  all(db, sql, values).await?.iter().map(row_of).collect()
}

/// One agent's open, standing rows of a room, oldest first.
pub async fn for_room(
  db: &DatabaseConnection,
  chat_id: i64,
  agent: &str,
) -> Result<Vec<MemoryRow>, Failure> {
  memories(
    db,
    r#"SELECT * FROM "memories" WHERE "chat_id" = $1 AND "agent" = $2 AND "active" = 1 AND "done_at" IS NULL ORDER BY "set_at", "id""#,
    vec![chat_id.into(), agent.into()],
  )
  .await
}

/// One agent's open, standing rows tied to no room, oldest first.
pub async fn global(db: &DatabaseConnection, agent: &str) -> Result<Vec<MemoryRow>, Failure> {
  memories(
    db,
    r#"SELECT * FROM "memories" WHERE "chat_id" IS NULL AND "agent" = $1 AND "active" = 1 AND "done_at" IS NULL ORDER BY "set_at", "id""#,
    vec![agent.into()],
  )
  .await
}

/// A turn's matches counted, which is what a row's clock runs from.
pub async fn bump(db: &DatabaseConnection, slugs: &[String], now: i64) -> Result<(), Failure> {
  for slug in slugs {
    run(
      db,
      r#"UPDATE "memories" SET "hits" = "hits" + 1, "last_hit_at" = $1 WHERE "slug" = $2 AND "active" = 1"#,
      vec![now.into(), slug.as_str().into()],
    )
    .await?;
  }

  Ok(())
}

async fn by(
  db: &DatabaseConnection,
  column: &str,
  value: sea_orm::Value,
) -> Result<Option<MemoryRow>, Failure> {
  one(db, &format!(r#"SELECT * FROM "memories" WHERE "{column}" = $1"#), vec![value])
    .await?
    .as_ref()
    .map(row_of)
    .transpose()
}

/// One row taken down with why, where it still stands in its own scope; nothing removes it.
pub async fn take_down(
  db: &DatabaseConnection,
  memory: &MemoryRow,
  reason: &str,
  now: i64,
) -> Result<(), Failure> {
  let standing = by(db, "slug", memory.slug.as_str().into()).await?;

  if standing.is_none_or(|standing| standing.chat_id != memory.chat_id || !standing.active) {
    return Ok(());
  }

  run(
    db,
    r#"UPDATE "memories" SET "active" = 0, "down_reason" = $1, "down_at" = $2 WHERE "slug" = $3"#,
    vec![reason.into(), now.into(), memory.slug.as_str().into()],
  )
  .await?;

  Ok(())
}

// A state checked off with how it ended, where it is still open in its room.
async fn release(db: &DatabaseConnection, memory: &MemoryRow, text: &str, now: i64) -> Result<(), Failure> {
  let standing = by(db, "id", memory.id.into()).await?;

  if standing.is_none_or(|standing| standing.chat_id != memory.chat_id || standing.kind == "situation") {
    return Ok(());
  }

  run(
    db,
    r#"UPDATE "memories" SET "done_at" = $1, "done_text" = $2 WHERE "id" = $3 AND "chat_id" = $4 AND "done_at" IS NULL"#,
    vec![now.into(), text.into(), memory.id.into(), memory.chat_id.into()],
  )
  .await?;

  Ok(())
}

/// Whether a global row is offered in a room of this kind.
pub fn reaches_room(memory: &MemoryRow, room: RoadKind) -> bool {
  memory.reach == "all"
    || memory.reach
      == match room {
        RoadKind::Owner => "owner",
        RoadKind::Group => "group",
      }
}

/// When each agent's rows of each room were last looked over for withering, held for the service's life and let
/// go of a room when it is forgotten.
#[derive(Debug, Default)]
pub struct Withering {
  stamps: Mutex<HashMap<(String, i64), i64>>,
}

impl Withering {
  /// Whether an agent's rows of a room are due to be looked over now, stamping them looked over where they are.
  pub fn due(&self, agent: &str, chat_id: i64, now: i64) -> bool {
    let mut stamps = self.stamps.lock().expect("the withering stamps");
    let key = (agent.to_owned(), chat_id);

    if now - stamps.get(&key).copied().unwrap_or(0) < BRAIN.memories.wither_every_seconds {
      return false;
    }

    stamps.insert(key, now);

    true
  }

  /// A forgotten room never looked over, should it come again.
  pub fn forget(&self, chat_id: i64) {
    self
      .stamps
      .lock()
      .expect("the withering stamps")
      .retain(|(_, room), _| *room != chat_id);
  }
}

// An agent's open rows of a room and the global ones reaching it, each carrying a condition.
async fn with_conditions(
  db: &DatabaseConnection,
  chat_id: i64,
  room: RoadKind,
  agent: &str,
) -> Result<Vec<MemoryRow>, Failure> {
  let mut asked = for_room(db, chat_id, agent).await?;

  asked.extend(global(db, agent).await?.into_iter().filter(|memory| reaches_room(memory, room)));
  asked.retain(|memory| !memory.condition.is_empty());

  Ok(asked)
}

/// The rows a room's matcher is offered — its own and the global ones that reach it, each carrying a condition —
/// the ones that have waited too long taken down first, at most once a sweep's span for a room.
pub async fn conditions(
  db: &DatabaseConnection,
  withering: &Withering,
  chat_id: i64,
  room: RoadKind,
  agent: &str,
  now: i64,
) -> Result<Vec<MemoryRow>, Failure> {
  let asked = with_conditions(db, chat_id, room, agent).await?;

  if !withering.due(agent, chat_id, now) {
    return Ok(asked);
  }

  let mut standing = vec![];

  for memory in asked {
    if now - memory.last_hit_at.unwrap_or(memory.set_at) >= BRAIN.memories.wither_after_seconds {
      take_down(db, &memory, "withered", now).await?;
      println!("memories: chat {chat_id} withered {}", memory.slug);
      continue;
    }

    standing.push(memory);
  }

  Ok(standing)
}

/// What a room's matcher handed back: the rows that matched counted, and the ones it read as spent, among the
/// rows it may have been offered, taken down with the matcher as their reason.
pub async fn matched(
  db: &DatabaseConnection,
  chat_id: i64,
  room: RoadKind,
  agent: &str,
  (matched, spent): (&[String], &[String]),
  now: i64,
) -> Result<(), Failure> {
  bump(db, matched, now).await?;

  if spent.is_empty() {
    return Ok(());
  }

  let offered = with_conditions(db, chat_id, room, agent).await?;

  for slug in spent {
    if let Some(rule) = offered.iter().rev().find(|rule| rule.slug == *slug) {
      take_down(db, rule, "spent", now).await?;
      println!("situation rules: chat {chat_id} retired {slug}: the matcher read it as spent");
    }
  }

  Ok(())
}

/// The rows a turn's events wake that attach here by the event alone — this room's situation rows and the global
/// ones that reach it — every errand the events wake, of any agent and room, stamped as met, and each row answered
/// counted.
pub async fn woken(
  db: &DatabaseConnection,
  chat_id: i64,
  room: RoadKind,
  agent: &str,
  events: &[Event],
  now: i64,
) -> Result<Vec<MemoryRow>, Failure> {
  if events.is_empty() {
    return Ok(vec![]);
  }

  let waiting = memories(
    db,
    r#"SELECT * FROM "memories" WHERE "event" <> '' AND "active" = 1 AND "done_at" IS NULL ORDER BY "set_at", "id""#,
    vec![],
  )
  .await?;
  let woken: Vec<MemoryRow> = waiting
    .into_iter()
    .filter(|memory| {
      memory
        .event
        .as_ref()
        .is_some_and(|waited| events.iter().any(|came| event_wakes(waited, came)))
    })
    .collect();

  for memory in woken
    .iter()
    .filter(|memory| memory.kind != "situation" && memory.event_met_at.is_none())
  {
    run(
      db,
      r#"UPDATE "memories" SET "event_met_at" = $1 WHERE "event_met_at" IS NULL AND "id" = $2"#,
      vec![now.into(), memory.id.into()],
    )
    .await?;
  }

  let attached: Vec<MemoryRow> = woken
    .into_iter()
    .filter(|memory| {
      memory.agent == agent
        && match memory.chat_id {
          None => reaches_room(memory, room),
          Some(room_of) => room_of == chat_id && memory.kind == "situation",
        }
    })
    .collect();
  let slugs: Vec<String> = attached.iter().map(|memory| memory.slug.clone()).collect();

  bump(db, &slugs, now).await?;

  Ok(attached)
}

/// Whether a state has stood still long enough to be released.
pub fn state_stopped(memory: &MemoryRow, now: i64) -> bool {
  memory.kind == "state"
    && now - memory.moved_at.unwrap_or(memory.set_at) >= BRAIN.memories.state_idle_seconds
}

/// The rows standing in front of a turn of a room — open, waiting on nothing, or an errand whose event has come —
/// oldest first, a state that stood still released first and left out.
pub async fn standing(
  db: &DatabaseConnection,
  chat_id: i64,
  agent: &str,
  now: i64,
) -> Result<Vec<MemoryRow>, Failure> {
  let rows = memories(
    db,
    r#"SELECT * FROM "memories"
       WHERE "chat_id" = $1 AND "agent" = $2 AND "active" = 1 AND "done_at" IS NULL AND "condition" = ''
         AND ("event" = '' OR ("event_met_at" IS NOT NULL AND "type" <> 'situation'))
       ORDER BY "set_at", "id""#,
    vec![chat_id.into(), agent.into()],
  )
  .await?;
  let hours = (BRAIN.memories.state_idle_seconds / 3_600).to_string();
  let reason = fill_marks(&BRAIN.memories.released, &[("hours", &hours)]);

  for memory in rows.iter().filter(|row| state_stopped(row, now)) {
    release(db, memory, &reason, now).await?;
    println!("memories: chat {chat_id} released state {}", memory.slug);
  }

  Ok(rows.into_iter().filter(|row| !state_stopped(row, now)).collect())
}
