//! A brain's memories, each row one version (docs/design.md, 「Memories」, 「Versions」, 「Writing」 and
//! 「Strength and decay」): listed, written, superseded, moved, hit and confirmed.

use std::collections::BTreeMap;

use bmo_brain_client::{
  Confirmations, Hits, Labels, MemoriesQuery, Memory, MemoryMerge, MemoryMove, MemoryStatus, MemoryVersion,
  NewMemory, Page, SubjectKey, SubjectRef, TypeCount,
};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::brains::{brain_of, event_scope, given, layer_of, subject_found_or_made, subjects_of};
use super::rules::Reckoning as _;
use super::{Fault, Held, Store, TEXT_MOST, holding, limit, now, page, publish, text};

/// The types code depends on the meaning of.
pub const RESERVED_TYPES: [&str; 10] = [
  "episode",
  "fact",
  "rumor",
  "situation",
  "rule",
  "name",
  "sheet",
  "persona",
  "lore",
  "knowledge",
];

pub(crate) const MEMORY_COLUMNS: &str = "id, brain_id, key, type, text, layer_id, position, status, lineage_id, \
  superseded_by, valid_from, valid_to, importance, hits, hit_times, last_hit_at, last_confirmed_at, strength, \
  consolidated_at, actor_name, labels, created_at";

#[derive(FromRow)]
pub(crate) struct MemoryRow {
  pub id: i64,
  pub brain_id: i64,
  pub key: Option<String>,
  #[sqlx(rename = "type")]
  pub kind: String,
  pub text: String,
  pub layer_id: Option<i64>,
  pub position: i64,
  pub status: String,
  pub lineage_id: i64,
  pub superseded_by: Option<i64>,
  pub valid_from: i64,
  pub valid_to: Option<i64>,
  pub importance: i16,
  pub hits: i64,
  pub hit_times: Vec<i64>,
  pub last_hit_at: Option<i64>,
  pub last_confirmed_at: Option<i64>,
  pub strength: f64,
  pub consolidated_at: Option<i64>,
  pub actor_name: String,
  pub labels: Json<Labels>,
  pub created_at: i64,
}

impl MemoryRow {
  pub(crate) fn status(&self) -> Held<MemoryStatus> {
    MemoryStatus::named(&self.status)
      .ok_or_else(|| Fault::Failed(format!("Row {} holds no known status: {}.", self.id, self.status)))
  }

  pub(crate) fn member(self, subjects: Vec<SubjectRef>, retrieval_strength: f64) -> Held<Memory> {
    let status = self.status()?;
    Ok(Memory {
      id: self.id,
      brain_id: self.brain_id,
      key: self.key,
      kind: self.kind,
      text: self.text,
      layer_id: self.layer_id,
      position: self.position,
      subjects,
      status,
      lineage_id: self.lineage_id,
      superseded_by: self.superseded_by,
      valid_from: self.valid_from,
      valid_to: self.valid_to,
      importance: i64::from(self.importance),
      hits: self.hits,
      last_hit_at: self.last_hit_at,
      last_confirmed_at: self.last_confirmed_at,
      strength: self.strength,
      retrieval_strength,
      consolidated_at: self.consolidated_at,
      actor_name: self.actor_name,
      labels: self.labels.0,
      created_at: self.created_at,
    })
  }
}

#[derive(FromRow)]
struct LinkRow {
  memory_id: i64,
  id: i64,
  #[sqlx(rename = "type")]
  kind: String,
  key: String,
}

/// The subjects of each of these rows, by row id.
pub(crate) async fn subjects_by_row(
  conn: &mut PgConnection,
  ids: &[i64],
) -> Held<BTreeMap<i64, Vec<SubjectRef>>> {
  let links: Vec<LinkRow> = sqlx::query_as(
    "SELECT ms.memory_id, s.id, s.type, s.key FROM memory_subjects ms JOIN subjects s ON s.id = ms.subject_id \
     WHERE ms.memory_id = ANY($1) ORDER BY ms.memory_id, s.id",
  )
  .bind(ids)
  .fetch_all(&mut *conn)
  .await?;
  let mut by_row: BTreeMap<i64, Vec<SubjectRef>> = BTreeMap::new();
  for link in links {
    by_row.entry(link.memory_id).or_default().push(SubjectRef {
      id: link.id,
      kind: link.kind,
      key: link.key,
    });
  }
  Ok(by_row)
}

impl Store {
  /// Rows as members, each with its subjects and its retrieval strength as of now.
  pub(crate) async fn members(&self, conn: &mut PgConnection, rows: Vec<MemoryRow>) -> Held<Vec<Memory>> {
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let mut subjects = subjects_by_row(conn, &ids).await?;
    let at = now();
    rows
      .into_iter()
      .map(|row| {
        let strength = self.rules.retrieval_strength(&row.hit_times, at);
        let mine = subjects.remove(&row.id).unwrap_or_default();
        row.member(mine, strength)
      })
      .collect()
  }

  pub(crate) async fn row_of(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
    id: i64,
    for_update: bool,
  ) -> Held<MemoryRow> {
    let lock = if for_update { " FOR UPDATE" } else { "" };
    sqlx::query_as(&format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND id = $2{lock}"))
      .bind(brain_id)
      .bind(id)
      .fetch_optional(&mut *conn)
      .await?
      .ok_or_else(|| Fault::NotFound(format!("Brain {brain_id} has no memory {id}.")))
  }

  pub(crate) async fn member_of(&self, conn: &mut PgConnection, brain_id: i64, id: i64) -> Held<Memory> {
    let row = self.row_of(conn, brain_id, id, false).await?;
    Ok(self.members(conn, vec![row]).await?.remove(0))
  }

  /// A page of a brain's memories.
  pub async fn memories(&self, brain_id: i64, query: &MemoriesQuery) -> Held<Page<Memory>> {
    let limit = limit(query.limit)?;
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let mut sql =
      QueryBuilder::<Postgres>::new(format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = "));
    sql.push_bind(brain_id);
    if let Some(kind) = &query.kind {
      sql.push(" AND type = ").push_bind(kind.clone());
    }
    if let Some(status) = query.status {
      sql.push(" AND status = ").push_bind(status.name());
    }
    if let Some(layer) = query.layer_id {
      sql.push(" AND layer_id = ").push_bind(layer);
    }
    if let Some(subject) = query.subject_id {
      sql
        .push(" AND id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = ")
        .push_bind(subject)
        .push(")");
    }
    if let Some(key) = &query.key {
      sql.push(" AND key = ").push_bind(key.clone());
    }
    if let Some(lineage) = query.lineage_id {
      sql.push(" AND lineage_id = ").push_bind(lineage);
    }
    if let Some(label) = &query.label {
      match label.split_once('=') {
        Some((key, value)) => {
          sql.push(" AND labels @> ").push_bind(Json(serde_json::json!({ key: value })))
        }
        None => sql.push(" AND labels ? ").push_bind(label.clone()),
      };
    }
    if let Some(words) = &query.q {
      sql.push(" AND text ILIKE ").push_bind(holding(words));
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<MemoryRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    Ok(page(self.members(&mut conn, rows).await?, limit))
  }

  /// One memory.
  pub async fn memory(&self, brain_id: i64, id: i64) -> Held<Memory> {
    self.member_of(&mut *self.connection().await?, brain_id, id).await
  }

  /// Every version of a lineage, oldest first.
  pub async fn lineage(&self, brain_id: i64, lineage_id: i64) -> Held<Vec<Memory>> {
    let mut conn = self.connection().await?;
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND lineage_id = $2 ORDER BY id"
    ))
    .bind(brain_id)
    .bind(lineage_id)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
      return Err(Fault::NotFound(format!("Brain {brain_id} has no lineage {lineage_id}.")));
    }
    self.members(&mut conn, rows).await
  }

  /// A memory written as a first version.
  pub async fn memory_add(&self, brain_id: i64, body: &NewMemory) -> Held<Memory> {
    let mut tx = self.begin().await?;
    let memory = self.add_in(&mut tx, brain_id, body, now()).await?;
    self.commit(tx).await?;
    Ok(memory)
  }

  /// A memory written as a first version, within a transaction.
  pub(crate) async fn add_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    body: &NewMemory,
    at: i64,
  ) -> Held<Memory> {
    given("type", &body.kind)?;
    text("text", &body.text, TEXT_MOST)?;
    given("actorName", &body.actor_name)?;
    if let Some(key) = &body.key {
      super::key("key", key)?;
    }
    let importance = importance(body.importance)?;
    let status = body.status.unwrap_or(MemoryStatus::Active);
    if !status.is_live() {
      return Err(Fault::BadRequest(format!(
        "status is {}; a first version is active or pending.",
        status.name()
      )));
    }
    brain_of(&mut *tx, brain_id).await?;
    if let Some(layer) = body.layer_id {
      layer_of(&mut *tx, brain_id, layer).await?;
    }
    if let Some(key) = &body.key {
      let holder: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM memories WHERE brain_id = $1 AND key = $2 AND status IN ('active', 'pending')",
      )
      .bind(brain_id)
      .bind(key)
      .fetch_optional(&mut *tx)
      .await?;
      if let Some(id) = holder {
        return Err(Fault::Conflict(format!(
          "Memory {id} holds the key {key:?}; write a version of it instead."
        )));
      }
    }
    let subject_ids = resolve_subjects(&mut *tx, brain_id, &body.subject_ids, &body.subjects, at).await?;
    let position = match body.position {
      Some(position) => position,
      None => next_position(&mut *tx, brain_id, body.layer_id).await?,
    };
    let id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       importance, actor_name, labels, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, $9, $10, $11, $12) \
       RETURNING id",
    )
    .bind(brain_id)
    .bind(&body.key)
    .bind(&body.kind)
    .bind(&body.text)
    .bind(body.layer_id)
    .bind(position)
    .bind(status.name())
    .bind(body.valid_from.unwrap_or(at))
    .bind(importance)
    .bind(&body.actor_name)
    .bind(Json(&body.labels))
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE memories SET lineage_id = id WHERE id = $1")
      .bind(id)
      .execute(&mut *tx)
      .await?;
    link_subjects(&mut *tx, id, &subject_ids).await?;
    let memory = self.member_of(&mut *tx, brain_id, id).await?;
    publish(&mut *tx, "memory.written", Some(&event_scope(brain_id)), &memory).await?;
    Ok(memory)
  }

  /// A new version of a memory, superseding the version named.
  pub async fn memory_version(&self, brain_id: i64, id: i64, body: &MemoryVersion) -> Held<Memory> {
    let mut tx = self.begin().await?;
    let memory = self.version_in(&mut tx, brain_id, id, body, now()).await?;
    self.commit(tx).await?;
    Ok(memory)
  }

  /// A new version written over the version named, within a transaction.
  pub(crate) async fn version_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    id: i64,
    body: &MemoryVersion,
    at: i64,
  ) -> Held<Memory> {
    given("actorName", &body.actor_name)?;
    if let Some(words) = &body.text {
      text("text", words, TEXT_MOST)?;
    }
    if let Some(kind) = &body.kind {
      given("type", kind)?;
    }
    let importance = body.importance.map(Some).map(importance).transpose()?;
    let old = self.row_of(&mut *tx, brain_id, id, true).await?;
    let status = old.status()?;
    if !status.is_live() {
      return Err(Fault::Conflict(format!(
        "Memory {id} is {}; a version is written over a live one.",
        status.name()
      )));
    }
    let layer_id = match body.layer_id {
      Some(layer) => layer,
      None => old.layer_id,
    };
    if let Some(layer) = layer_id {
      layer_of(&mut *tx, brain_id, layer).await?;
    }
    let subject_ids = match (&body.subject_ids, &body.subjects) {
      (None, None) => {
        sqlx::query_scalar(
          "SELECT subject_id FROM memory_subjects WHERE memory_id = $1 ORDER BY subject_id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?
      }
      (ids, keys) => {
        resolve_subjects(
          &mut *tx,
          brain_id,
          ids.as_deref().unwrap_or(&[]),
          keys.as_deref().unwrap_or(&[]),
          at,
        )
        .await?
      }
    };
    let valid_from = body.valid_from.unwrap_or(at);
    sqlx::query("UPDATE memories SET status = 'superseded', valid_to = $2 WHERE id = $1")
      .bind(id)
      .bind(valid_from)
      .execute(&mut *tx)
      .await?;
    let new_id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       importance, hits, hit_times, last_hit_at, last_confirmed_at, strength, actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) RETURNING id",
    )
    .bind(brain_id)
    .bind(&old.key)
    .bind(body.kind.as_ref().unwrap_or(&old.kind))
    .bind(body.text.as_ref().unwrap_or(&old.text))
    .bind(layer_id)
    .bind(body.position.unwrap_or(old.position))
    .bind(status.name())
    .bind(old.lineage_id)
    .bind(valid_from)
    .bind(importance.unwrap_or(old.importance))
    .bind(old.hits)
    .bind(&old.hit_times)
    .bind(old.last_hit_at)
    .bind(old.last_confirmed_at)
    .bind(old.strength)
    .bind(&body.actor_name)
    .bind(Json(body.labels.as_ref().unwrap_or(&old.labels.0)))
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE memories SET superseded_by = $2 WHERE id = $1")
      .bind(id)
      .bind(new_id)
      .execute(&mut *tx)
      .await?;
    link_subjects(&mut *tx, new_id, &subject_ids).await?;
    let superseded = self.member_of(&mut *tx, brain_id, id).await?;
    publish(&mut *tx, "memory.moved", Some(&event_scope(brain_id)), &superseded).await?;
    let memory = self.member_of(&mut *tx, brain_id, new_id).await?;
    publish(&mut *tx, "memory.written", Some(&event_scope(brain_id)), &memory).await?;
    Ok(memory)
  }

  /// One row written over several, each merged row superseded by it.
  pub async fn memory_merge(&self, brain_id: i64, body: &MemoryMerge) -> Held<Memory> {
    let mut tx = self.begin().await?;
    let memory = self.merge_in(&mut tx, brain_id, body, now()).await?;
    self.commit(tx).await?;
    Ok(memory)
  }

  /// A merge within a transaction: the new row starts a lineage and keeps the most the merged rows had
  /// earned; each merged row becomes superseded, pointing at it.
  pub(crate) async fn merge_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    body: &MemoryMerge,
    at: i64,
  ) -> Held<Memory> {
    given("type", &body.kind)?;
    text("text", &body.text, TEXT_MOST)?;
    given("actorName", &body.actor_name)?;
    if let Some(key) = &body.key {
      super::key("key", key)?;
    }
    let mut ids = body.memory_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < 2 {
      return Err(Fault::BadRequest("memoryIds names fewer than two memories.".to_owned()));
    }
    brain_of(&mut *tx, brain_id).await?;
    if let Some(layer) = body.layer_id {
      layer_of(&mut *tx, brain_id, layer).await?;
    }
    let mut merged = Vec::with_capacity(ids.len());
    for id in &ids {
      let row = self.row_of(&mut *tx, brain_id, *id, true).await?;
      let status = row.status()?;
      if !status.is_live() {
        return Err(Fault::Conflict(format!("Memory {id} is {}; a merge takes live rows.", status.name())));
      }
      merged.push(row);
    }
    if let Some(key) = &body.key {
      let holder: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM memories WHERE brain_id = $1 AND key = $2 AND status IN ('active', 'pending')",
      )
      .bind(brain_id)
      .bind(key)
      .fetch_optional(&mut *tx)
      .await?;
      if let Some(id) = holder
        && !ids.contains(&id)
      {
        return Err(Fault::Conflict(format!(
          "Memory {id} holds the key {key:?}; name it among the merged or write a version of it."
        )));
      }
    }
    let importance = match body.importance {
      Some(_) => importance(body.importance)?,
      None => merged.iter().map(|row| row.importance).max().unwrap_or(5),
    };
    let subject_ids: Vec<i64> = match (&body.subject_ids, &body.subjects) {
      (None, None) => {
        sqlx::query_scalar(
          "SELECT DISTINCT subject_id FROM memory_subjects WHERE memory_id = ANY($1) ORDER BY subject_id",
        )
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await?
      }
      (given_ids, keys) => {
        resolve_subjects(
          &mut *tx,
          brain_id,
          given_ids.as_deref().unwrap_or(&[]),
          keys.as_deref().unwrap_or(&[]),
          at,
        )
        .await?
      }
    };
    let position = match body.position {
      Some(position) => position,
      None => next_position(&mut *tx, brain_id, body.layer_id).await?,
    };
    let valid_from = body.valid_from.unwrap_or(at);
    let most_hit = merged.iter().max_by_key(|row| row.hits);
    let hits = most_hit.map_or(0, |row| row.hits);
    let hit_times = most_hit.map(|row| row.hit_times.clone()).unwrap_or_default();
    let last_hit_at = merged.iter().filter_map(|row| row.last_hit_at).max();
    let last_confirmed_at = merged.iter().filter_map(|row| row.last_confirmed_at).min();
    let strength = merged.iter().map(|row| row.strength).fold(1.0_f64, f64::max);
    let labels = body.labels.clone().unwrap_or_default();
    let new_id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       importance, hits, hit_times, last_hit_at, last_confirmed_at, strength, actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, 'active', 0, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) RETURNING id",
    )
    .bind(brain_id)
    .bind(&body.key)
    .bind(&body.kind)
    .bind(&body.text)
    .bind(body.layer_id)
    .bind(position)
    .bind(valid_from)
    .bind(importance)
    .bind(hits)
    .bind(&hit_times)
    .bind(last_hit_at)
    .bind(last_confirmed_at)
    .bind(strength)
    .bind(&body.actor_name)
    .bind(Json(&labels))
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE memories SET lineage_id = id WHERE id = $1")
      .bind(new_id)
      .execute(&mut *tx)
      .await?;
    sqlx::query(
      "UPDATE memories SET status = 'superseded', valid_to = $2, superseded_by = $3 WHERE id = ANY($1)",
    )
    .bind(&ids)
    .bind(valid_from)
    .bind(new_id)
    .execute(&mut *tx)
    .await?;
    link_subjects(&mut *tx, new_id, &subject_ids).await?;
    for id in &ids {
      let superseded = self.member_of(&mut *tx, brain_id, *id).await?;
      publish(&mut *tx, "memory.moved", Some(&event_scope(brain_id)), &superseded).await?;
    }
    let memory = self.member_of(&mut *tx, brain_id, new_id).await?;
    publish(&mut *tx, "memory.written", Some(&event_scope(brain_id)), &memory).await?;
    Ok(memory)
  }

  /// A memory's status moved by a caller's word.
  pub async fn memory_move(&self, brain_id: i64, id: i64, body: &MemoryMove) -> Held<Memory> {
    let mut tx = self.begin().await?;
    let memory = self.move_in(&mut tx, brain_id, id, body).await?;
    self.commit(tx).await?;
    Ok(memory)
  }

  /// A memory's status moved, within a transaction.
  pub(crate) async fn move_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    id: i64,
    body: &MemoryMove,
  ) -> Held<Memory> {
    given("actorName", &body.actor_name)?;
    let row = self.row_of(&mut *tx, brain_id, id, true).await?;
    let was = row.status()?;
    if was == body.status {
      let memory = self.member_of(&mut *tx, brain_id, id).await?;
      return Ok(memory);
    }
    if !was.may_move_to(body.status) {
      return Err(Fault::Conflict(format!(
        "Memory {id} is {}; it does not move to {}.",
        was.name(),
        body.status.name()
      )));
    }
    let at = body.at.unwrap_or_else(now);
    let valid_to = match body.status {
      MemoryStatus::Done | MemoryStatus::Invalidated => Some(at),
      _ => None,
    };
    sqlx::query("UPDATE memories SET status = $2, valid_to = $3 WHERE id = $1")
      .bind(id)
      .bind(body.status.name())
      .bind(valid_to)
      .execute(&mut *tx)
      .await?;
    let memory = self.member_of(&mut *tx, brain_id, id).await?;
    publish(&mut *tx, "memory.moved", Some(&event_scope(brain_id)), &memory).await?;
    Ok(memory)
  }

  /// A reading's hand-out recorded: each row hit, and marked for the session where one is named.
  pub async fn hits_record(&self, brain_id: i64, body: &Hits) -> Held<i64> {
    given("actorName", &body.actor_name)?;
    if body.memory_ids.is_empty() {
      return Err(Fault::BadRequest("memoryIds names no row.".to_owned()));
    }
    let at = body.at.unwrap_or_else(now);
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let count = self.hit(&mut tx, brain_id, &body.memory_ids, body.session_key.as_deref(), at).await?;
    self.commit(tx).await?;
    Ok(count)
  }

  /// The rows hit at `at`, marked for `session` where one is named; how many rows the ids reached.
  pub(crate) async fn hit(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
    memory_ids: &[i64],
    session: Option<&str>,
    at: i64,
  ) -> Held<i64> {
    let mut ids = memory_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let kept = self.rules.hit_times_kept.max(1);
    let hit: Vec<i64> = sqlx::query_scalar(
      "UPDATE memories SET hits = hits + 1, last_hit_at = $3, \
       hit_times = (array_append(hit_times, $3))[GREATEST(1, array_length(hit_times, 1) + 2 - $4):] \
       WHERE brain_id = $1 AND id = ANY($2) RETURNING id",
    )
    .bind(brain_id)
    .bind(&ids)
    .bind(at)
    .bind(kept)
    .fetch_all(&mut *conn)
    .await?;
    if let Some(session) = session {
      given("sessionKey", session)?;
      sqlx::query(
        "INSERT INTO handouts (brain_id, session_key, memory_id, at) \
         SELECT $1, $2, id, $4 FROM UNNEST($3::bigint[]) AS id ON CONFLICT DO NOTHING",
      )
      .bind(brain_id)
      .bind(session)
      .bind(&hit)
      .bind(at)
      .execute(&mut *conn)
      .await?;
    }
    Ok(hit.len() as i64)
  }

  /// Confirmations recorded: each row's storage strength raised by the gap since its last, and its moment set.
  pub async fn confirmations_record(&self, brain_id: i64, body: &Confirmations) -> Held<i64> {
    given("actorName", &body.actor_name)?;
    if body.memory_ids.is_empty() {
      return Err(Fault::BadRequest("memoryIds names no row.".to_owned()));
    }
    let mut tx = self.begin().await?;
    let count = self
      .confirm_in(&mut tx, brain_id, &body.memory_ids, body.at.unwrap_or_else(now))
      .await?;
    self.commit(tx).await?;
    Ok(count)
  }

  /// Confirmations recorded, within a transaction: each row's storage strength raised by the gap since its
  /// last, and its moment set; how many rows the ids reached.
  pub(crate) async fn confirm_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    memory_ids: &[i64],
    at: i64,
  ) -> Held<i64> {
    brain_of(&mut *tx, brain_id).await?;
    let mut ids = memory_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let rows: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
      "SELECT id, created_at, last_confirmed_at FROM memories WHERE brain_id = $1 AND id = ANY($2) FOR UPDATE",
    )
    .bind(brain_id)
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await?;
    for (id, created_at, last) in &rows {
      let gained = self.rules.strength_gained(last.unwrap_or(*created_at), at);
      sqlx::query("UPDATE memories SET strength = strength + $2, last_confirmed_at = $3 WHERE id = $1")
        .bind(id)
        .bind(gained)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    }
    Ok(rows.len() as i64)
  }

  /// The types in use in a brain, with a count each, the reserved ones first.
  pub async fn types(&self, brain_id: i64) -> Held<Vec<TypeCount>> {
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let counted: Vec<(String, i64)> =
      sqlx::query_as("SELECT type, COUNT(*) FROM memories WHERE brain_id = $1 GROUP BY type ORDER BY type")
        .bind(brain_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut types: Vec<TypeCount> = RESERVED_TYPES
      .iter()
      .map(|name| TypeCount {
        kind: (*name).to_owned(),
        count: counted.iter().find(|(kind, _)| kind == name).map_or(0, |(_, count)| *count),
        reserved: true,
      })
      .collect();
    types.extend(counted.into_iter().filter(|(kind, _)| !RESERVED_TYPES.contains(&kind.as_str())).map(
      |(kind, count)| TypeCount {
        kind,
        count,
        reserved: false,
      },
    ));
    Ok(types)
  }
}

/// An importance as a row takes it: 1 to 10, 5 where none is given.
fn importance(asked: Option<i64>) -> Held<i16> {
  match asked {
    None => Ok(5),
    Some(importance) if (1..=10).contains(&importance) => Ok(importance as i16),
    Some(importance) => Err(Fault::BadRequest(format!("importance is {importance}; it is 1 to 10."))),
  }
}

/// The next position in a layer, or among the unfiled.
async fn next_position(conn: &mut PgConnection, brain_id: i64, layer_id: Option<i64>) -> Held<i64> {
  let last: Option<i64> = sqlx::query_scalar(
    "SELECT MAX(position) FROM memories WHERE brain_id = $1 AND layer_id IS NOT DISTINCT FROM $2",
  )
  .bind(brain_id)
  .bind(layer_id)
  .fetch_one(&mut *conn)
  .await?;
  Ok(last.map_or(0, |last| last + 1))
}

/// The subject ids a write names, by id and by type and key, each checked or made; sorted, no doubles.
pub(crate) async fn resolve_subjects(
  conn: &mut PgConnection,
  brain_id: i64,
  ids: &[i64],
  keys: &[SubjectKey],
  at: i64,
) -> Held<Vec<i64>> {
  let mut all: Vec<i64> =
    subjects_of(conn, brain_id, ids).await?.into_iter().map(|subject| subject.id).collect();
  for key in keys {
    let (_, subject) = subject_found_or_made(conn, brain_id, key, at).await?;
    all.push(subject.id);
  }
  all.sort_unstable();
  all.dedup();
  Ok(all)
}

pub(crate) async fn link_subjects(
  conn: &mut PgConnection,
  memory_id: i64,
  subject_ids: &[i64],
) -> Held<()> {
  if subject_ids.is_empty() {
    return Ok(());
  }
  sqlx::query(
    "INSERT INTO memory_subjects (memory_id, subject_id) SELECT $1, id FROM UNNEST($2::bigint[]) AS id \
     ON CONFLICT DO NOTHING",
  )
  .bind(memory_id)
  .bind(subject_ids)
  .execute(&mut *conn)
  .await?;
  Ok(())
}
