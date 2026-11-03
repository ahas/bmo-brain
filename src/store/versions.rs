//! A brain's memories, each row one version (docs/design.md, 「Memories」, 「Versions」, 「Writing」 and
//! 「Use」): listed, written, superseded, moved, used and confirmed.

use std::collections::BTreeMap;

use bmo_brain_client::{
  Confirmations, Hits, Labels, MemoriesQuery, Memory, MemoryMerge, MemoryMove, MemoryStatus, MemoryVersion,
  NewMemory, Page, Restoration, SubjectKey, SubjectRef, TypeCount,
};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::brains::{brain_of, event_scope, given, layer_of, subject_found_or_made, subjects_of};
use super::sheets::{SHEET, written_type};
use super::{Fault, Held, Store, TEXT_MOST, holding, limit, listed, now, page, publish, text};

/// The types code depends on the meaning of.
pub const RESERVED_TYPES: [&str; 9] =
  ["episode", "fact", "rumor", "situation", "rule", "name", "persona", "lore", "knowledge"];

pub(crate) const MEMORY_COLUMNS: &str = "id, brain_id, key, type, text, layer_id, position, status, lineage_id, \
  superseded_by, valid_from, valid_to, recall_condition, release_condition, importance, hits, hit_times, \
  last_hit_at, last_confirmed_at, consolidated_at, actor_name, reason, labels, created_at";

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
  pub recall_condition: Option<String>,
  pub release_condition: Option<String>,
  pub importance: i16,
  pub hits: i64,
  pub hit_times: Vec<i64>,
  pub last_hit_at: Option<i64>,
  pub last_confirmed_at: Option<i64>,
  pub consolidated_at: Option<i64>,
  pub actor_name: String,
  pub reason: Option<String>,
  pub labels: Json<Labels>,
  pub created_at: i64,
}

impl MemoryRow {
  pub(crate) fn status(&self) -> Held<MemoryStatus> {
    MemoryStatus::named(&self.status)
      .ok_or_else(|| Fault::Failed(format!("Row {} holds no known status: {}.", self.id, self.status)))
  }

  pub(crate) fn member(self, subjects: Vec<SubjectRef>) -> Held<Memory> {
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
      recall_condition: self.recall_condition,
      release_condition: self.release_condition,
      importance: i64::from(self.importance),
      use_count: self.hits,
      last_used_at: self.last_hit_at,
      last_confirmed_at: self.last_confirmed_at,
      consolidated_at: self.consolidated_at,
      actor_name: self.actor_name,
      reason: self.reason,
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
  /// Rows as members, each with its subjects.
  pub(crate) async fn members(&self, conn: &mut PgConnection, rows: Vec<MemoryRow>) -> Held<Vec<Memory>> {
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let mut subjects = subjects_by_row(conn, &ids).await?;
    rows
      .into_iter()
      .map(|row| {
        let mine = subjects.remove(&row.id).unwrap_or_default();
        row.member(mine)
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

  /// A page of a brain's memories, under the filters and in the order the query names. A read by key,
  /// subject, type or words records a use of each live row it answers, unless the query says not to.
  pub async fn memories(&self, brain_id: i64, query: &MemoriesQuery) -> Held<Page<Memory>> {
    let limit = limit(query.limit)?;
    let filter = Filter::of(query)?;
    let (field, descending) = order(query.order.as_deref())?;
    let mut conn = self.begin().await?;
    brain_of(&mut conn, brain_id).await?;
    let mut sql =
      QueryBuilder::<Postgres>::new(format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = "));
    sql.push_bind(brain_id);
    filter.push(&mut sql);
    let (beyond, direction) = if descending { ("<", "DESC") } else { (">", "") };
    // The page walks the order the list stands in, the field then id, from past the row last seen.
    if let Some(after) = query.after {
      if field == "id" {
        sql.push(format!(" AND id {beyond} ")).push_bind(after);
      } else {
        sql
          .push(format!(
            " AND ({field}, id) {beyond} (SELECT {field}, id FROM memories WHERE brain_id = "
          ))
          .push_bind(brain_id)
          .push(" AND id = ")
          .push_bind(after)
          .push(")");
      }
    }
    if field == "id" {
      sql.push(format!(" ORDER BY id {direction}"));
    } else {
      sql.push(format!(" ORDER BY {field} {direction}, id {direction}"));
    }
    sql.push(" LIMIT ").push_bind(limit + 1);
    let mut rows: Vec<MemoryRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    if query.record.unwrap_or(true) && filter.is_explicit() {
      let live: Vec<i64> = rows
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .filter(|row| row.status().is_ok_and(MemoryStatus::is_live))
        .map(|row| row.id)
        .collect();
      if !live.is_empty() {
        let at = now();
        self.hit(&mut conn, brain_id, &live, None, at).await?;
        // The page answers the rows as the read left them, in the order it read them.
        let used: Vec<MemoryRow> = sqlx::query_as(&format!(
          "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND id = ANY($2)"
        ))
        .bind(brain_id)
        .bind(&live)
        .fetch_all(&mut *conn)
        .await?;
        let mut by_id: BTreeMap<i64, MemoryRow> = used.into_iter().map(|row| (row.id, row)).collect();
        for row in rows.iter_mut() {
          if let Some(used) = by_id.remove(&row.id) {
            *row = used;
          }
        }
      }
    }
    let members = self.members(&mut conn, rows).await?;
    self.commit(conn).await?;
    Ok(page(members, limit))
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
    written_type(&body.kind)?;
    text("text", &body.text, TEXT_MOST)?;
    conditions(body.recall_condition.as_deref(), body.release_condition.as_deref())?;
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
       recall_condition, release_condition, importance, actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, $9, $10, $11, $12, $13, $14) RETURNING id",
    )
    .bind(brain_id)
    .bind(&body.key)
    .bind(&body.kind)
    .bind(&body.text)
    .bind(body.layer_id)
    .bind(position)
    .bind(status.name())
    .bind(body.valid_from.unwrap_or(at))
    .bind(&body.recall_condition)
    .bind(&body.release_condition)
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
    if let Some(Some(words)) = &body.recall_condition {
      text("recallCondition", words, super::CONDITION_MOST)?;
    }
    if let Some(Some(words)) = &body.release_condition {
      text("releaseCondition", words, super::CONDITION_MOST)?;
    }
    if let Some(kind) = &body.kind {
      written_type(kind)?;
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
    if body.kind.is_none() {
      written_type(&old.kind)?;
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
    let recall_condition = body.recall_condition.clone().unwrap_or_else(|| old.recall_condition.clone());
    let release_condition = body.release_condition.clone().unwrap_or_else(|| old.release_condition.clone());
    sqlx::query("UPDATE memories SET status = 'superseded', valid_to = $2 WHERE id = $1")
      .bind(id)
      .bind(valid_from)
      .execute(&mut *tx)
      .await?;
    let new_id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       recall_condition, release_condition, importance, hits, hit_times, last_hit_at, last_confirmed_at, \
       actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) RETURNING id",
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
    .bind(&recall_condition)
    .bind(&release_condition)
    .bind(importance.unwrap_or(old.importance))
    .bind(old.hits)
    .bind(&old.hit_times)
    .bind(old.last_hit_at)
    .bind(old.last_confirmed_at)
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
    written_type(&body.kind)?;
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
    let labels = body.labels.clone().unwrap_or_default();
    let recall_condition = match &body.recall_condition {
      Some(words) => {
        text("recallCondition", words, super::CONDITION_MOST)?;
        Some(words.clone())
      }
      None => shared(&merged, |row| row.recall_condition.as_deref()),
    };
    let release_condition = match &body.release_condition {
      Some(words) => {
        text("releaseCondition", words, super::CONDITION_MOST)?;
        Some(words.clone())
      }
      None => shared(&merged, |row| row.release_condition.as_deref()),
    };
    // The merged rows leave the live set first, so a key one of them held is free for the new row.
    sqlx::query("UPDATE memories SET status = 'superseded', valid_to = $2 WHERE id = ANY($1)")
      .bind(&ids)
      .bind(valid_from)
      .execute(&mut *tx)
      .await?;
    let new_id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       recall_condition, release_condition, importance, hits, hit_times, last_hit_at, last_confirmed_at, \
       actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, 'active', 0, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
       RETURNING id",
    )
    .bind(brain_id)
    .bind(&body.key)
    .bind(&body.kind)
    .bind(&body.text)
    .bind(body.layer_id)
    .bind(position)
    .bind(valid_from)
    .bind(&recall_condition)
    .bind(&release_condition)
    .bind(importance)
    .bind(hits)
    .bind(&hit_times)
    .bind(last_hit_at)
    .bind(last_confirmed_at)
    .bind(&body.actor_name)
    .bind(Json(&labels))
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE memories SET lineage_id = id WHERE id = $1")
      .bind(new_id)
      .execute(&mut *tx)
      .await?;
    sqlx::query("UPDATE memories SET superseded_by = $2 WHERE id = ANY($1)")
      .bind(&ids)
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

  /// A memory's status moved by a caller's word; a move to `done` with the words it ended on writes them as a
  /// version first and moves that version.
  pub async fn memory_move(&self, brain_id: i64, id: i64, body: &MemoryMove) -> Held<Memory> {
    let mut tx = self.begin().await?;
    let memory = match &body.text {
      None => self.move_in(&mut tx, brain_id, id, body).await?,
      Some(_) if body.status != MemoryStatus::Done => {
        return Err(Fault::BadRequest(format!(
          "text is the words a move to done ends on; this move is to {}.",
          body.status.name()
        )));
      }
      Some(words) => {
        given("actorName", &body.actor_name)?;
        let was = self.row_of(&mut tx, brain_id, id, false).await?.status()?;
        if !was.may_move_to(MemoryStatus::Done) {
          return Err(Fault::Conflict(format!("Memory {id} is {}; it does not move to done.", was.name())));
        }
        let at = body.at.unwrap_or_else(now);
        let version = MemoryVersion {
          text: Some(words.clone()),
          kind: None,
          layer_id: None,
          position: None,
          subject_ids: None,
          subjects: None,
          importance: None,
          valid_from: Some(at),
          recall_condition: None,
          release_condition: None,
          actor_name: body.actor_name.clone(),
          labels: None,
        };
        let written = self.version_in(&mut tx, brain_id, id, &version, at).await?;
        self.move_in(&mut tx, brain_id, written.id, body).await?
      }
    };
    self.commit(tx).await?;
    Ok(memory)
  }

  /// A closed lineage restored: a new active version written after its newest version, which is `done` or
  /// `invalidated` and stays as it was.
  pub async fn memory_restore(&self, brain_id: i64, id: i64, body: &Restoration) -> Held<Memory> {
    given("actorName", &body.actor_name)?;
    if let Some(words) = &body.text {
      text("text", words, TEXT_MOST)?;
    }
    let at = now();
    let mut tx = self.begin().await?;
    let closed = self.row_of(&mut tx, brain_id, id, true).await?;
    if closed.kind == SHEET {
      return Err(Fault::Conflict(format!(
        "Memory {id} is a sheet; a person's sheet is rendered from their facts and is not restored."
      )));
    }
    let status = closed.status()?;
    if !matches!(status, MemoryStatus::Done | MemoryStatus::Invalidated) {
      return Err(Fault::Conflict(format!(
        "Memory {id} is {}; a restoration follows a done or invalidated row.",
        status.name()
      )));
    }
    let newest: i64 =
      sqlx::query_scalar("SELECT MAX(id) FROM memories WHERE brain_id = $1 AND lineage_id = $2")
        .bind(brain_id)
        .bind(closed.lineage_id)
        .fetch_one(&mut *tx)
        .await?;
    if newest != id {
      return Err(Fault::Conflict(format!(
        "Memory {id} is not the newest of lineage {}; memory {newest} is.",
        closed.lineage_id
      )));
    }
    if let Some(key) = &closed.key {
      let holder: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM memories WHERE brain_id = $1 AND key = $2 AND status IN ('active', 'pending')",
      )
      .bind(brain_id)
      .bind(key)
      .fetch_optional(&mut *tx)
      .await?;
      if let Some(holder) = holder {
        return Err(Fault::Conflict(format!("Memory {holder} holds the key {key:?} now.")));
      }
    }
    let subject_ids: Vec<i64> =
      sqlx::query_scalar("SELECT subject_id FROM memory_subjects WHERE memory_id = $1 ORDER BY subject_id")
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
    let new_id: i64 = sqlx::query_scalar(
      "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
       recall_condition, release_condition, importance, hits, hit_times, last_hit_at, last_confirmed_at, \
       consolidated_at, actor_name, labels, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8, $10, $11, $9, $12, $13, $14, $15, $16, $17, $18, $8) \
       RETURNING id",
    )
    .bind(brain_id)
    .bind(&closed.key)
    .bind(&closed.kind)
    .bind(body.text.as_ref().unwrap_or(&closed.text))
    .bind(closed.layer_id)
    .bind(closed.position)
    .bind(closed.lineage_id)
    .bind(at)
    .bind(closed.importance)
    .bind(&closed.recall_condition)
    .bind(&closed.release_condition)
    .bind(closed.hits)
    .bind(&closed.hit_times)
    .bind(closed.last_hit_at)
    .bind(closed.last_confirmed_at)
    .bind(closed.consolidated_at)
    .bind(&body.actor_name)
    .bind(&closed.labels)
    .fetch_one(&mut *tx)
    .await?;
    link_subjects(&mut tx, new_id, &subject_ids).await?;
    let memory = self.member_of(&mut tx, brain_id, new_id).await?;
    publish(&mut tx, "memory.written", Some(&event_scope(brain_id)), &memory).await?;
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
    if let Some(reason) = &body.reason {
      text("reason", reason, REASON_MOST)?;
    }
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
    sqlx::query("UPDATE memories SET status = $2, valid_to = $3, reason = $4 WHERE id = $1")
      .bind(id)
      .bind(body.status.name())
      .bind(valid_to)
      .bind(&body.reason)
      .execute(&mut *tx)
      .await?;
    let memory = self.member_of(&mut *tx, brain_id, id).await?;
    publish(&mut *tx, "memory.moved", Some(&event_scope(brain_id)), &memory).await?;
    Ok(memory)
  }

  /// A hand-out, or the rows the agent used, recorded: a use of each row, and each marked for the session where
  /// one is named.
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

  /// A use of each row recorded at `at`, its moment kept among its newest, and each marked for `session` where
  /// one is named; how many rows the ids reached.
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

  /// Confirmations recorded: a use of each row, and its moment of confirmation set.
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

  /// Confirmations recorded, within a transaction: a use of each row, and its moment of confirmation set; how
  /// many rows the ids reached.
  pub(crate) async fn confirm_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    memory_ids: &[i64],
    at: i64,
  ) -> Held<i64> {
    brain_of(&mut *tx, brain_id).await?;
    let confirmed = self.hit(&mut *tx, brain_id, memory_ids, None, at).await?;
    sqlx::query("UPDATE memories SET last_confirmed_at = $3 WHERE brain_id = $1 AND id = ANY($2)")
      .bind(brain_id)
      .bind(memory_ids)
      .bind(at)
      .execute(&mut *tx)
      .await?;
    Ok(confirmed)
  }

  /// The types in use in a brain, with the count of live memories each, the reserved ones first: a type whose
  /// rows are all closed is not in use.
  pub async fn types(&self, brain_id: i64) -> Held<Vec<TypeCount>> {
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let counted: Vec<(String, i64)> = sqlx::query_as(
      "SELECT type, COUNT(*) FROM memories WHERE brain_id = $1 AND status IN ('active', 'pending') \
       GROUP BY type ORDER BY type",
    )
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

/// The most characters a move's reason holds.
const REASON_MOST: usize = 2_000;

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

/// The filters a memory list and a tally share, read from the query and checked.
pub(crate) struct Filter {
  kinds: Vec<String>,
  statuses: Vec<MemoryStatus>,
  excluded: Vec<String>,
  layer_id: Option<i64>,
  unfiled: bool,
  subject_id: Option<i64>,
  key: Option<String>,
  lineage_id: Option<i64>,
  ids: Option<Vec<i64>>,
  superseded_by: Option<Vec<i64>>,
  label: Option<String>,
  words: Option<String>,
  situation: Option<String>,
}

/// The ids a comma-separated list names, or why one does not read.
fn ids_listed(field: &str, value: Option<&str>) -> Held<Option<Vec<i64>>> {
  value
    .map(|value| {
      listed(Some(value))
        .iter()
        .map(|id| {
          id.parse()
            .map_err(|_| Fault::BadRequest(format!("{field} names {id:?}, which is no id.")))
        })
        .collect()
    })
    .transpose()
}

impl Filter {
  /// Whether the list is an explicit read: by key, subject, type or words.
  pub(crate) fn is_explicit(&self) -> bool {
    self.key.is_some()
      || self.subject_id.is_some()
      || !self.kinds.is_empty()
      || self.words.is_some()
      || self.situation.is_some()
  }

  /// The filters a memory list's query names.
  pub(crate) fn of(query: &MemoriesQuery) -> Held<Self> {
    let statuses = listed(query.status.as_deref())
      .iter()
      .map(|name| {
        MemoryStatus::named(name)
          .ok_or_else(|| Fault::BadRequest(format!("status names {name:?}, which is no status.")))
      })
      .collect::<Held<Vec<_>>>()?;
    Ok(Self {
      kinds: listed(query.kind.as_deref()),
      statuses,
      excluded: listed(query.exclude_types.as_deref()),
      layer_id: query.layer_id,
      unfiled: query.unfiled.unwrap_or(false),
      subject_id: query.subject_id,
      key: query.key.clone(),
      lineage_id: query.lineage_id,
      ids: ids_listed("ids", query.ids.as_deref())?,
      superseded_by: ids_listed("supersededBy", query.superseded_by.as_deref())?,
      label: query.label.clone(),
      words: query.q.clone(),
      situation: query.situation.clone(),
    })
  }

  /// The conditions pushed onto a query over `memories` already narrowed to one brain.
  pub(crate) fn push(&self, sql: &mut QueryBuilder<'_, Postgres>) {
    if !self.kinds.is_empty() {
      sql.push(" AND type = ANY(").push_bind(self.kinds.clone()).push(")");
    }
    if !self.statuses.is_empty() {
      let names: Vec<&str> = self.statuses.iter().map(|status| status.name()).collect();
      sql.push(" AND status = ANY(").push_bind(names).push(")");
    }
    if !self.excluded.is_empty() {
      sql.push(" AND NOT (type = ANY(").push_bind(self.excluded.clone()).push("))");
    }
    if let Some(layer) = self.layer_id {
      sql.push(" AND layer_id = ").push_bind(layer);
    }
    if self.unfiled {
      sql.push(" AND layer_id IS NULL");
    }
    if let Some(subject) = self.subject_id {
      sql
        .push(" AND id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = ")
        .push_bind(subject)
        .push(")");
    }
    if let Some(key) = &self.key {
      sql.push(" AND key = ").push_bind(key.clone());
    }
    if let Some(lineage) = self.lineage_id {
      sql.push(" AND lineage_id = ").push_bind(lineage);
    }
    if let Some(ids) = &self.ids {
      sql.push(" AND id = ANY(").push_bind(ids.clone()).push(")");
    }
    if let Some(ids) = &self.superseded_by {
      sql.push(" AND superseded_by = ANY(").push_bind(ids.clone()).push(")");
    }
    if let Some(label) = &self.label {
      match label.split_once('=') {
        Some((key, value)) => {
          sql.push(" AND labels @> ").push_bind(Json(serde_json::json!({ key: value })))
        }
        None => sql.push(" AND labels ? ").push_bind(label.clone()),
      };
    }
    if let Some(words) = &self.words {
      sql.push(" AND text ILIKE ").push_bind(holding(words));
    }
    if let Some(words) = &self.situation {
      sql.push(" AND recall_condition ILIKE ").push_bind(holding(words));
    }
  }
}

/// The conditions given, each not blank and a condition's length where one is given.
fn conditions(recall: Option<&str>, release: Option<&str>) -> Held<()> {
  if let Some(words) = recall {
    text("recallCondition", words, super::CONDITION_MOST)?;
  }
  if let Some(words) = release {
    text("releaseCondition", words, super::CONDITION_MOST)?;
  }
  Ok(())
}

/// The one condition every one of these rows carries, where they all carry the same one.
fn shared(rows: &[MemoryRow], of: impl Fn(&MemoryRow) -> Option<&str>) -> Option<String> {
  let first = of(rows.first()?)?;
  if rows.iter().all(|row| of(row) == Some(first)) {
    Some(first.to_owned())
  } else {
    None
  }
}

/// The column a memory list is ordered by and whether it runs descending, as `order` names it.
fn order(asked: Option<&str>) -> Held<(&'static str, bool)> {
  let asked = asked.map(str::trim).unwrap_or("id");
  let (name, descending) = match asked.strip_prefix('-') {
    Some(name) => (name, true),
    None => (asked, false),
  };
  let column = match name {
    "id" => "id",
    "position" => "position",
    "validFrom" => "valid_from",
    "importance" => "importance",
    "useCount" => "hits",
    _ => {
      return Err(Fault::BadRequest(format!(
        "order is {asked:?}; it is id, position, validFrom, importance or useCount, descending with a leading -."
      )));
    }
  };
  Ok((column, descending))
}
