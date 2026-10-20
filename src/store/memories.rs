//! Memory rows, the matches readings record on them, notes, and the withering of a forgotten scope's rows
//! (docs/boundary-api.md, 「Memory rows」, 「Notes」 and 「Scopes」).

use bmo_brain_client::{
  Labels, MemoriesQuery, Memory, MemoryChange, MemoryMatch, MemoryState, NewMemory, NewMemoryMatch,
  NewNote, Note, NotesQuery, Object, Page, Watch,
};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::watches::{self, WATCH_COLUMNS, WatchRow};
use super::{Fault, Held, Store, TEXT_MOST, holding, limit, name, now, page, publish, scope, text};

pub(crate) const MEMORY_COLUMNS: &str = "id, scope, subject_key, kind, text, state, conditions, labels, due_at, \
  match_count, last_matched_at, created_at, changed_at";

#[derive(FromRow)]
pub(crate) struct MemoryRow {
  id: i64,
  scope: String,
  subject_key: Option<String>,
  kind: String,
  text: String,
  state: String,
  conditions: Json<Vec<Object>>,
  labels: Json<Labels>,
  due_at: Option<i64>,
  match_count: i64,
  last_matched_at: Option<i64>,
  created_at: i64,
  changed_at: i64,
}

impl MemoryRow {
  fn state(&self) -> Held<MemoryState> {
    MemoryState::named(&self.state)
      .ok_or_else(|| Fault::Failed(format!("Row {} holds no known state: {}.", self.id, self.state)))
  }

  fn member(self, watches: Vec<Watch>) -> Held<Memory> {
    let state = self.state()?;
    Ok(Memory {
      id: self.id,
      scope: self.scope,
      subject_key: self.subject_key,
      kind: self.kind,
      text: self.text,
      state,
      conditions: self.conditions.0,
      labels: self.labels.0,
      due_at: self.due_at,
      match_count: self.match_count,
      last_matched_at: self.last_matched_at,
      created_at: self.created_at,
      changed_at: self.changed_at,
      watches,
    })
  }
}

/// Rows as members, each with the watches whose condition names it.
pub(crate) async fn members(conn: &mut PgConnection, rows: Vec<MemoryRow>) -> Held<Vec<Memory>> {
  let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
  let watching: Vec<WatchRow> =
    sqlx::query_as(&format!("SELECT {WATCH_COLUMNS} FROM watches WHERE memory_id = ANY($1) ORDER BY id"))
      .bind(&ids)
      .fetch_all(&mut *conn)
      .await?;
  let mut watching: Vec<(Option<i64>, Watch)> = watching
    .into_iter()
    .map(|row| (row.memory_id, row))
    .map(|(id, row)| Ok((id, row.member()?)))
    .collect::<Held<_>>()?;
  rows
    .into_iter()
    .map(|row| {
      let mine = watching.extract_if(.., |(id, _)| *id == Some(row.id)).map(|(_, watch)| watch).collect();
      row.member(mine)
    })
    .collect()
}

async fn read(conn: &mut PgConnection, id: i64, for_update: bool) -> Held<MemoryRow> {
  let lock = if for_update { " FOR UPDATE" } else { "" };
  sqlx::query_as(&format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id = $1{lock}"))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| Fault::NotFound(format!("There is no memory row {id}.")))
}

/// One row as a member.
pub(crate) async fn member(conn: &mut PgConnection, id: i64) -> Held<Memory> {
  let row = read(conn, id, false).await?;
  Ok(members(conn, vec![row]).await?.remove(0))
}

/// The labels a `label` filter names: `key` for any value, `key=value` for one.
fn label_filter(label: &str) -> serde_json::Value {
  match label.split_once('=') {
    Some((key, value)) => serde_json::json!({ key: value }),
    None => serde_json::Value::Null,
  }
}

impl Store {
  /// A page of rows.
  pub async fn memories(&self, query: &MemoriesQuery) -> Held<Page<Memory>> {
    let limit = limit(query.limit)?;
    let mut sql =
      QueryBuilder::<Postgres>::new(format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE TRUE"));
    if let Some(scope) = &query.scope {
      sql.push(" AND scope = ").push_bind(scope.clone());
    }
    if let Some(prefix) = &query.scope_prefix {
      sql.push(" AND starts_with(scope, ").push_bind(prefix.clone()).push(")");
    }
    if let Some(subject) = &query.subject_key {
      sql.push(" AND subject_key = ").push_bind(subject.clone());
    }
    if let Some(kind) = &query.kind {
      sql.push(" AND kind = ").push_bind(kind.clone());
    }
    if let Some(state) = query.state {
      sql.push(" AND state = ").push_bind(state.name());
    }
    if let Some(label) = &query.label {
      match label_filter(label) {
        serde_json::Value::Null => sql.push(" AND labels ? ").push_bind(label.clone()),
        pair => sql.push(" AND labels @> ").push_bind(Json(pair)),
      };
    }
    if let Some(before) = query.due_before {
      sql.push(" AND due_at <= ").push_bind(before);
    }
    if let Some(words) = &query.q {
      sql.push(" AND text ILIKE ").push_bind(holding(words));
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let mut conn = self.connection().await?;
    let rows: Vec<MemoryRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    Ok(page(members(&mut conn, rows).await?, limit))
  }

  /// A row added.
  pub async fn memory_add(&self, body: &NewMemory) -> Held<Memory> {
    scope("scope", &body.scope)?;
    name("kind", &body.kind)?;
    text("text", &body.text, TEXT_MOST)?;
    let at = now();
    let mut tx = self.begin().await?;
    let row: MemoryRow = sqlx::query_as(&format!(
      "INSERT INTO memories (scope, subject_key, kind, text, state, conditions, labels, due_at, created_at, \
       changed_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9) RETURNING {MEMORY_COLUMNS}"
    ))
    .bind(&body.scope)
    .bind(&body.subject_key)
    .bind(&body.kind)
    .bind(&body.text)
    .bind(body.state.unwrap_or(MemoryState::Standing).name())
    .bind(Json(&body.conditions))
    .bind(Json(&body.labels))
    .bind(body.due_at)
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    let memory = members(&mut tx, vec![row]).await?.remove(0);
    publish(&mut tx, "memory.created", Some(&memory.scope), &memory).await?;
    self.commit(tx).await?;
    Ok(memory)
  }

  /// One row.
  pub async fn memory(&self, id: i64) -> Held<Memory> {
    member(&mut *self.connection().await?, id).await
  }

  /// A row changed by the fields given.
  pub async fn memory_change(&self, id: i64, body: &MemoryChange) -> Held<Memory> {
    if let Some(words) = &body.text {
      text("text", words, TEXT_MOST)?;
    }
    let mut tx = self.begin().await?;
    let row = read(&mut tx, id, true).await?;
    let was = row.state()?;
    let state = body.state.unwrap_or(was);
    if !was.may_move_to(state) {
      return Err(Fault::Conflict(format!(
        "Row {id} is {}; it does not move to {}.",
        was.name(),
        state.name()
      )));
    }
    let at = now();
    sqlx::query(
      "UPDATE memories SET text = COALESCE($2, text), state = $3, labels = COALESCE($4, labels), \
       due_at = CASE WHEN $5 THEN $6 ELSE due_at END, changed_at = $7 WHERE id = $1",
    )
    .bind(id)
    .bind(&body.text)
    .bind(state.name())
    .bind(body.labels.as_ref().map(Json))
    .bind(body.due_at.is_some())
    .bind(body.due_at.flatten())
    .bind(at)
    .execute(&mut *tx)
    .await?;
    if state != was {
      watches::row_moved(&mut tx, id, state, at).await?;
    }
    let memory = member(&mut tx, id).await?;
    publish(&mut tx, "memory.changed", Some(&memory.scope), &memory).await?;
    self.commit(tx).await?;
    Ok(memory)
  }

  /// A reading's match recorded, and each row it matched counted.
  pub async fn memory_match_add(&self, body: &NewMemoryMatch) -> Held<MemoryMatch> {
    name("actorName", &body.actor_name)?;
    if body.memory_ids.is_empty() {
      return Err(Fault::BadRequest("memoryIds names no row.".to_owned()));
    }
    if body.spent_ms < 0 || body.token_count < 0 {
      return Err(Fault::BadRequest("spentMs and tokenCount are not below zero.".to_owned()));
    }
    let at = now();
    let mut tx = self.begin().await?;
    let mut ids = body.memory_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    let counted: Vec<i64> = sqlx::query_scalar(
      "UPDATE memories SET match_count = match_count + 1, last_matched_at = $2 WHERE id = ANY($1) RETURNING id",
    )
    .bind(&ids)
    .bind(at)
    .fetch_all(&mut *tx)
    .await?;
    let missing: Vec<i64> = ids.iter().copied().filter(|id| !counted.contains(id)).collect();
    if !missing.is_empty() {
      return Err(Fault::BadRequest(format!("There is no memory row {missing:?}.")));
    }
    let id: i64 = sqlx::query_scalar(
      "INSERT INTO memory_matches (memory_ids, actor_name, spent_ms, token_count, created_at) \
       VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&body.memory_ids)
    .bind(&body.actor_name)
    .bind(body.spent_ms)
    .bind(body.token_count)
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    self.commit(tx).await?;
    Ok(MemoryMatch {
      id,
      memory_ids: body.memory_ids.clone(),
      actor_name: body.actor_name.clone(),
      spent_ms: body.spent_ms,
      token_count: body.token_count,
      created_at: at,
    })
  }

  /// A page of notes.
  pub async fn notes(&self, query: &NotesQuery) -> Held<Page<Note>> {
    let limit = limit(query.limit)?;
    let mut sql = QueryBuilder::<Postgres>::new(
      "SELECT id, scope, subject_key, text, actor_name, created_at FROM notes WHERE TRUE",
    );
    if let Some(scope) = &query.scope {
      sql.push(" AND scope = ").push_bind(scope.clone());
    }
    if let Some(subject) = &query.subject_key {
      sql.push(" AND subject_key = ").push_bind(subject.clone());
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<NoteRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    Ok(page(rows.into_iter().map(NoteRow::member).collect(), limit))
  }

  /// A note added.
  pub async fn note_add(&self, body: &NewNote) -> Held<Note> {
    scope("scope", &body.scope)?;
    text("text", &body.text, TEXT_MOST)?;
    name("actorName", &body.actor_name)?;
    let mut tx = self.begin().await?;
    let row: NoteRow = sqlx::query_as(
      "INSERT INTO notes (scope, subject_key, text, actor_name, created_at) VALUES ($1, $2, $3, $4, $5) \
       RETURNING id, scope, subject_key, text, actor_name, created_at",
    )
    .bind(&body.scope)
    .bind(&body.subject_key)
    .bind(&body.text)
    .bind(&body.actor_name)
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    let note = row.member();
    publish(&mut tx, "note.created", Some(&note.scope), &note).await?;
    self.commit(tx).await?;
    Ok(note)
  }

  /// The standing rows of every forgotten scope that no reading has matched since `before` moved to `off`;
  /// how many moved.
  pub async fn wither(&self, before: i64) -> Held<usize> {
    let mut tx = self.begin().await?;
    let ids: Vec<i64> = sqlx::query_scalar(
      "SELECT m.id FROM memories m JOIN scopes s ON s.scope = m.scope \
       WHERE s.state = 'forgotten' AND m.state = 'standing' AND COALESCE(m.last_matched_at, m.created_at) < $1 \
       ORDER BY m.id LIMIT 500 FOR UPDATE OF m SKIP LOCKED",
    )
    .bind(before)
    .fetch_all(&mut *tx)
    .await?;
    let at = now();
    for &id in &ids {
      sqlx::query("UPDATE memories SET state = 'off', changed_at = $2 WHERE id = $1")
        .bind(id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
      watches::row_moved(&mut tx, id, MemoryState::Off, at).await?;
      let memory = member(&mut tx, id).await?;
      publish(&mut tx, "memory.changed", Some(&memory.scope), &memory).await?;
    }
    self.commit(tx).await?;
    Ok(ids.len())
  }
}

#[derive(FromRow)]
struct NoteRow {
  id: i64,
  scope: String,
  subject_key: Option<String>,
  text: String,
  actor_name: String,
  created_at: i64,
}

impl NoteRow {
  fn member(self) -> Note {
    Note {
      id: self.id,
      scope: self.scope,
      subject_key: self.subject_key,
      text: self.text,
      actor_name: self.actor_name,
      created_at: self.created_at,
    }
  }
}
