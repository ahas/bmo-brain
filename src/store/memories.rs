//! Memory rows, listed and changed for the console's screens (docs/boundary-api.md, 「Memory rows」).

use bmo_brain_client::{
  Labels, Object, Page, ScopedMemoriesQuery, ScopedMemory, ScopedMemoryChange, ScopedMemoryState,
};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::{Fault, Held, Store, TEXT_MOST, holding, limit, now, page, publish, text};

const SCOPED_MEMORY_COLUMNS: &str = "id, scope, subject_key, kind, text, state, conditions, labels, due_at, \
  match_count, last_matched_at, created_at, changed_at";

#[derive(FromRow)]
struct ScopedMemoryRow {
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

impl ScopedMemoryRow {
  fn state(&self) -> Held<ScopedMemoryState> {
    ScopedMemoryState::named(&self.state)
      .ok_or_else(|| Fault::Failed(format!("Row {} holds no known state: {}.", self.id, self.state)))
  }

  fn member(self) -> Held<ScopedMemory> {
    let state = self.state()?;
    Ok(ScopedMemory {
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
    })
  }
}

/// A row, locked for the transaction.
async fn locked(conn: &mut PgConnection, id: i64) -> Held<ScopedMemoryRow> {
  sqlx::query_as(&format!("SELECT {SCOPED_MEMORY_COLUMNS} FROM scoped_memories WHERE id = $1 FOR UPDATE"))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| Fault::NotFound(format!("There is no memory row {id}.")))
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
  pub async fn scoped_memories(&self, query: &ScopedMemoriesQuery) -> Held<Page<ScopedMemory>> {
    let limit = limit(query.limit)?;
    let mut sql = QueryBuilder::<Postgres>::new(format!(
      "SELECT {SCOPED_MEMORY_COLUMNS} FROM scoped_memories WHERE TRUE"
    ));
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
    let rows: Vec<ScopedMemoryRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    let rows = rows.into_iter().map(ScopedMemoryRow::member).collect::<Held<Vec<_>>>()?;
    Ok(page(rows, limit))
  }

  /// A row changed by the fields given.
  pub async fn scoped_memory_change(&self, id: i64, body: &ScopedMemoryChange) -> Held<ScopedMemory> {
    if let Some(words) = &body.text {
      text("text", words, TEXT_MOST)?;
    }
    let mut tx = self.begin().await?;
    let row = locked(&mut tx, id).await?;
    let was = row.state()?;
    let state = body.state.unwrap_or(was);
    if !was.may_move_to(state) {
      return Err(Fault::Conflict(format!(
        "Row {id} is {}; it does not move to {}.",
        was.name(),
        state.name()
      )));
    }
    let row: ScopedMemoryRow = sqlx::query_as(&format!(
      "UPDATE scoped_memories SET text = COALESCE($2, text), state = $3, labels = COALESCE($4, labels), \
       due_at = CASE WHEN $5 THEN $6 ELSE due_at END, changed_at = $7 WHERE id = $1 \
       RETURNING {SCOPED_MEMORY_COLUMNS}"
    ))
    .bind(id)
    .bind(&body.text)
    .bind(state.name())
    .bind(body.labels.as_ref().map(Json))
    .bind(body.due_at.is_some())
    .bind(body.due_at.flatten())
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    let memory = row.member()?;
    publish(&mut tx, "memory.changed", Some(&memory.scope), &memory).await?;
    self.commit(tx).await?;
    Ok(memory)
  }
}
