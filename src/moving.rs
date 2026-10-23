//! The data move (docs/design.md, 「The data move」): the rows held under scopes moved once, whole, into
//! brains, subjects, layers and memories, with nothing lost. It runs inside one transaction, refuses a store
//! that holds a brain already, and a dry run writes nothing.

use std::collections::BTreeMap;

use bmo_brain_client::Labels;
use serde_json::Value;
use sqlx::types::Json;
use sqlx::{PgConnection, Row};

use crate::store::Store;

/// What the move came to, table by table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Moved {
  pub brains: u64,
  pub subjects: u64,
  pub layers: u64,
  pub scoped_memories: u64,
  pub fragments: u64,
  pub notes: u64,
  pub room_rules: u64,
  pub dry_run: bool,
}

impl std::fmt::Display for Moved {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(
      out,
      "{}: {} brains, {} subjects, {} layers",
      if self.dry_run { "dry run" } else { "moved" },
      self.brains,
      self.subjects,
      self.layers
    )?;
    writeln!(out, "scoped memory rows -> memories: {}", self.scoped_memories)?;
    writeln!(out, "fragments -> sheet, knowledge and rule memories: {}", self.fragments)?;
    writeln!(out, "notes -> episode memories: {}", self.notes)?;
    write!(out, "room rules -> rule memories: {}", self.room_rules)
  }
}

/// A scope read: the agent it names, and the room where it names one as `<agent>:room:<id>`.
pub fn scope_read(scope: &str) -> (String, Option<i64>) {
  if let Some((agent, rest)) = scope.split_once(":room:")
    && let Ok(room) = rest.parse::<i64>()
  {
    return (agent.to_owned(), Some(room));
  }
  (scope.to_owned(), None)
}

/// The status and the moment it stopped holding, for an old row's state.
pub fn status_of(state: &str, changed_at: i64) -> (&'static str, Option<i64>) {
  match state {
    "standing" => ("active", None),
    "done" => ("done", Some(changed_at)),
    "candidate" => ("pending", None),
    _ => ("invalidated", Some(changed_at)),
  }
}

/// The condition's words folded into a situation's text.
pub fn situation_text(kind: &str, text: &str, conditions: &[Value]) -> String {
  if kind != "situation" {
    return text.to_owned();
  }
  let condition = conditions
    .iter()
    .filter_map(|c| c.get("condition").and_then(Value::as_str))
    .map(str::trim)
    .find(|c| !c.is_empty());
  match condition {
    Some(condition) if !text.contains(condition) => format!("When {condition}: {text}"),
    _ => text.to_owned(),
  }
}

struct Mover<'a> {
  conn: &'a mut PgConnection,
  account: String,
  brains: BTreeMap<String, i64>,
  subjects: BTreeMap<(i64, String, String), i64>,
  layers: BTreeMap<(i64, String), i64>,
  moved: Moved,
  at: i64,
}

impl Mover<'_> {
  async fn brain(&mut self, agent: &str) -> Result<i64, String> {
    if let Some(id) = self.brains.get(agent) {
      return Ok(*id);
    }
    let id: i64 = sqlx::query_scalar(
      "INSERT INTO brains (account_key, agent_key, name, created_at) VALUES ($1, $2, $2, $3) RETURNING id",
    )
    .bind(&self.account)
    .bind(agent)
    .bind(self.at)
    .fetch_one(&mut *self.conn)
    .await
    .map_err(|why| why.to_string())?;
    self.brains.insert(agent.to_owned(), id);
    self.moved.brains += 1;
    Ok(id)
  }

  async fn subject(&mut self, brain: i64, kind: &str, key: &str) -> Result<i64, String> {
    let at = (brain, kind.to_owned(), key.to_owned());
    if let Some(id) = self.subjects.get(&at) {
      return Ok(*id);
    }
    let id: i64 = sqlx::query_scalar(
      "INSERT INTO subjects (brain_id, type, key, description, created_at) VALUES ($1, $2, $3, '', $4) RETURNING id",
    )
    .bind(brain)
    .bind(kind)
    .bind(key)
    .bind(self.at)
    .fetch_one(&mut *self.conn)
    .await
    .map_err(|why| why.to_string())?;
    self.subjects.insert(at, id);
    self.moved.subjects += 1;
    Ok(id)
  }

  async fn layer(&mut self, brain: i64, name: &str) -> Result<i64, String> {
    let at = (brain, name.to_owned());
    if let Some(id) = self.layers.get(&at) {
      return Ok(*id);
    }
    let position = self.layers.keys().filter(|(b, _)| *b == brain).count() as i64;
    let id: i64 = sqlx::query_scalar(
      "INSERT INTO layers (brain_id, name, description, position, created_at) VALUES ($1, $2, '', $3, $4) RETURNING id",
    )
    .bind(brain)
    .bind(name)
    .bind(position)
    .bind(self.at)
    .fetch_one(&mut *self.conn)
    .await
    .map_err(|why| why.to_string())?;
    self.layers.insert(at, id);
    self.moved.layers += 1;
    Ok(id)
  }

  #[allow(clippy::too_many_arguments)]
  async fn memory(
    &mut self,
    brain: i64,
    key: Option<&str>,
    kind: &str,
    text: &str,
    layer: Option<i64>,
    status: (&str, Option<i64>),
    valid_from: i64,
    hits: (i64, Option<i64>),
    actor: &str,
    labels: &Labels,
    created_at: i64,
    subjects: &[i64],
  ) -> Result<i64, String> {
    let hit_times: Vec<i64> = hits.1.into_iter().collect();
    let id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       valid_to, importance, hits, hit_times, last_hit_at, actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, 0, $6, 0, $7, $8, 5, $9, $10, $11, $12, $13, $14) RETURNING id",
    )
    .bind(brain)
    .bind(key)
    .bind(kind)
    .bind(text)
    .bind(layer)
    .bind(status.0)
    .bind(valid_from)
    .bind(status.1)
    .bind(hits.0)
    .bind(&hit_times)
    .bind(hits.1)
    .bind(actor)
    .bind(Json(labels))
    .bind(created_at)
    .fetch_one(&mut *self.conn)
    .await
    .map_err(|why| why.to_string())?;
    sqlx::query("UPDATE memories SET lineage_id = id WHERE id = $1")
      .bind(id)
      .execute(&mut *self.conn)
      .await
      .map_err(|why| why.to_string())?;
    if !subjects.is_empty() {
      sqlx::query(
        "INSERT INTO memory_subjects (memory_id, subject_id) SELECT $1, id FROM UNNEST($2::bigint[]) AS id \
         ON CONFLICT DO NOTHING",
      )
      .bind(id)
      .bind(subjects)
      .execute(&mut *self.conn)
      .await
      .map_err(|why| why.to_string())?;
    }
    Ok(id)
  }
}

/// Runs the move over the database at `address`, every brain made under `account`, scope-less fragments going
/// to `default_agent`'s brain; with `dry_run` the transaction is rolled back and only the counts stand.
pub async fn run(
  address: &str,
  account: &str,
  default_agent: &str,
  dry_run: bool,
) -> Result<Moved, String> {
  let store = Store::lazy(address)?;
  store.migrate().await.map_err(|why| format!("the migrations: {why}"))?;
  let mut tx = store.pool.begin().await.map_err(|why| why.to_string())?;
  let standing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM brains")
    .fetch_one(&mut *tx)
    .await
    .map_err(|why| why.to_string())?;
  if standing > 0 {
    return Err(format!(
      "the store holds {standing} brain(s) already; the move runs once, into an empty set"
    ));
  }
  let at = crate::store::now();
  let mut mover = Mover {
    conn: &mut tx,
    account: account.to_owned(),
    brains: BTreeMap::new(),
    subjects: BTreeMap::new(),
    layers: BTreeMap::new(),
    moved: Moved {
      dry_run,
      ..Moved::default()
    },
    at,
  };

  // Scoped memory rows.
  let rows = sqlx::query(
    "SELECT id, scope, subject_key, kind, text, state, conditions, labels, match_count, last_matched_at, \
     created_at, changed_at FROM scoped_memories ORDER BY id",
  )
  .fetch_all(&mut *mover.conn)
  .await
  .map_err(|why| why.to_string())?;
  for row in rows {
    let scope: String = row.get("scope");
    let (agent, room) = scope_read(&scope);
    let brain = mover.brain(&agent).await?;
    let kind: String = row.get("kind");
    let state: String = row.get("state");
    let conditions: Json<Vec<Value>> = row.get("conditions");
    let mut labels: Json<Labels> = row.get("labels");
    if !conditions.0.is_empty() {
      labels
        .0
        .insert("conditions".to_owned(), serde_json::to_string(&conditions.0).unwrap_or_default());
    }
    labels.0.insert("scoped_memory_id".to_owned(), row.get::<i64, _>("id").to_string());
    let text = situation_text(&kind, &row.get::<String, _>("text"), &conditions.0);
    let mut subjects = Vec::new();
    if let Some(room) = room {
      subjects.push(mover.subject(brain, "room", &room.to_string()).await?);
    }
    if let Some(person) = row.get::<Option<String>, _>("subject_key") {
      subjects.push(mover.subject(brain, "person", &person).await?);
    }
    let actor = labels.0.get("agent").cloned().unwrap_or_else(|| "move".to_owned());
    let changed_at: i64 = row.get("changed_at");
    mover
      .memory(
        brain,
        None,
        &kind,
        &text,
        None,
        status_of(&state, changed_at),
        row.get("created_at"),
        (row.get("match_count"), row.get("last_matched_at")),
        &actor,
        &labels.0,
        row.get("created_at"),
        &subjects,
      )
      .await?;
    mover.moved.scoped_memories += 1;
  }

  // Fragments: a person's sheet, a knowledge piece, a room's rule.
  let rows = sqlx::query(
    "SELECT key, kind, scope, subject_key, title, path, text, library_name, state, labels, hit_count, \
     last_hit_at, created_at, changed_at FROM fragments ORDER BY id",
  )
  .fetch_all(&mut *mover.conn)
  .await
  .map_err(|why| why.to_string())?;
  for row in rows {
    let scope: Option<String> = row.get("scope");
    let (agent, room) = scope.as_deref().map_or((default_agent.to_owned(), None), scope_read);
    let brain = mover.brain(&agent).await?;
    let kind: String = row.get("kind");
    let mut labels: Json<Labels> = row.get("labels");
    if let Some(title) = row.get::<Option<String>, _>("title") {
      labels.0.insert("title".to_owned(), title);
    }
    if let Some(path) = row.get::<Option<String>, _>("path") {
      labels.0.insert("path".to_owned(), path);
    }
    let (new_kind, layer) = match kind.as_str() {
      "person" => ("sheet", None),
      "room-rule" => ("rule", None),
      _ => {
        let name = row.get::<Option<String>, _>("library_name").unwrap_or_else(|| "knowledge".to_owned());
        ("knowledge", Some(mover.layer(brain, &name).await?))
      }
    };
    let mut subjects = Vec::new();
    if let Some(room) = room {
      subjects.push(mover.subject(brain, "room", &room.to_string()).await?);
    }
    if let Some(person) = row.get::<Option<String>, _>("subject_key") {
      subjects.push(mover.subject(brain, "person", &person).await?);
    }
    let state: String = row.get("state");
    let changed_at: i64 = row.get("changed_at");
    let status = if state == "active" { ("active", None) } else { ("invalidated", Some(changed_at)) };
    let key: String = row.get("key");
    mover
      .memory(
        brain,
        Some(&key),
        new_kind,
        &row.get::<String, _>("text"),
        layer,
        status,
        row.get("created_at"),
        (row.get("hit_count"), row.get("last_hit_at")),
        "move",
        &labels.0,
        row.get("created_at"),
        &subjects,
      )
      .await?;
    mover.moved.fragments += 1;
  }

  // Notes: episodes.
  let rows = sqlx::query("SELECT scope, subject_key, text, actor_name, created_at FROM notes ORDER BY id")
    .fetch_all(&mut *mover.conn)
    .await
    .map_err(|why| why.to_string())?;
  for row in rows {
    let (agent, room) = scope_read(&row.get::<String, _>("scope"));
    let brain = mover.brain(&agent).await?;
    let mut subjects = Vec::new();
    if let Some(room) = room {
      subjects.push(mover.subject(brain, "room", &room.to_string()).await?);
    }
    if let Some(person) = row.get::<Option<String>, _>("subject_key") {
      subjects.push(mover.subject(brain, "person", &person).await?);
    }
    let created_at: i64 = row.get("created_at");
    mover
      .memory(
        brain,
        None,
        "episode",
        &row.get::<String, _>("text"),
        None,
        ("active", None),
        created_at,
        (0, None),
        &row.get::<String, _>("actor_name"),
        &Labels::new(),
        created_at,
        &subjects,
      )
      .await?;
    mover.moved.notes += 1;
  }

  // Room rules: rules of the default agent's brain, each linked to its room and its grantee.
  let rows =
    sqlx::query("SELECT chat_id, id, subject, text, created_at FROM room_rules ORDER BY created_at, id")
      .fetch_all(&mut *mover.conn)
      .await
      .map_err(|why| why.to_string())?;
  for row in rows {
    let brain = mover.brain(default_agent).await?;
    let room = mover.subject(brain, "room", &row.get::<i64, _>("chat_id").to_string()).await?;
    let grantee = mover.subject(brain, "person", &row.get::<i64, _>("subject").to_string()).await?;
    let created_at: i64 = row.get("created_at");
    let key = format!("rule:{}:{}", row.get::<i64, _>("chat_id"), row.get::<String, _>("id"));
    mover
      .memory(
        brain,
        Some(&key),
        "rule",
        &row.get::<String, _>("text"),
        None,
        ("active", None),
        created_at,
        (0, None),
        "move",
        &Labels::new(),
        created_at,
        &[room, grantee],
      )
      .await?;
    mover.moved.room_rules += 1;
  }

  let moved = mover.moved.clone();
  if dry_run {
    tx.rollback().await.map_err(|why| why.to_string())?;
  } else {
    tx.commit().await.map_err(|why| why.to_string())?;
  }
  Ok(moved)
}
