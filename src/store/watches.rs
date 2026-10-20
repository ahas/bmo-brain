//! Watches and reminders, and the moments and row moves that fire them (docs/boundary-api.md, 「Watches」 and
//! 「Reminders」).

use bmo_brain_client::{
  Labels, MemoryState, NewReminder, NewWatch, Object, Page, Reminder, RemindersQuery, Watch, WatchChange,
  WatchState, WatchesQuery,
};
use serde_json::{Value, json};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::{Fault, Held, Store, TEXT_MOST, limit, name, now, page, publish, scope, text};

pub(crate) const WATCH_COLUMNS: &str = "id, scope, condition, state, setter_name, holder_name, labels, fired_at, created_at, changed_at, memory_id";

#[derive(FromRow)]
pub(crate) struct WatchRow {
  id: i64,
  scope: String,
  condition: Json<Object>,
  state: String,
  setter_name: String,
  holder_name: Option<String>,
  labels: Json<Labels>,
  fired_at: Option<i64>,
  created_at: i64,
  changed_at: i64,
  pub(crate) memory_id: Option<i64>,
}

impl WatchRow {
  fn state(&self) -> Held<WatchState> {
    WatchState::named(&self.state)
      .ok_or_else(|| Fault::Failed(format!("Watch {} holds no known state: {}.", self.id, self.state)))
  }

  pub(crate) fn member(self) -> Held<Watch> {
    let state = self.state()?;
    Ok(Watch {
      id: self.id,
      scope: self.scope,
      condition: self.condition.0,
      state,
      setter_name: self.setter_name,
      holder_name: self.holder_name,
      labels: self.labels.0,
      fired_at: self.fired_at,
      created_at: self.created_at,
      changed_at: self.changed_at,
    })
  }
}

/// What a condition waits on, read off it.
enum Waits {
  At(i64),
  Row(i64, MemoryState),
  External,
}

fn waits(condition: &Object) -> Held<Waits> {
  let field = |name: &str| condition.get(name);
  let refused = |why: &str| Err(Fault::BadRequest(format!("The condition {why}.")));
  match field("kind").and_then(Value::as_str) {
    Some("at") => match field("at").and_then(Value::as_i64) {
      Some(at) => Ok(Waits::At(at)),
      None => refused("of kind at names no moment `at`"),
    },
    Some("memory") => {
      let row = field("memoryId").and_then(Value::as_i64);
      let state = field("state").and_then(Value::as_str).and_then(MemoryState::named);
      match (row, state) {
        (Some(row), Some(state)) => Ok(Waits::Row(row, state)),
        _ => refused("of kind memory names no row `memoryId` and row `state`"),
      }
    }
    Some("external") => match field("key").and_then(Value::as_str) {
      Some(key) if !key.trim().is_empty() => Ok(Waits::External),
      _ => refused("of kind external names no `key`"),
    },
    _ => refused("is of no known kind: at, memory or external"),
  }
}

async fn read(conn: &mut PgConnection, id: i64, for_update: bool) -> Held<WatchRow> {
  let lock = if for_update { " FOR UPDATE" } else { "" };
  sqlx::query_as(&format!("SELECT {WATCH_COLUMNS} FROM watches WHERE id = $1{lock}"))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| Fault::NotFound(format!("There is no watch {id}.")))
}

/// A watch moved to `fired`, and `watch.fired` published.
async fn fire(conn: &mut PgConnection, id: i64, at: i64) -> Held<Watch> {
  let row: WatchRow = sqlx::query_as(&format!(
    "UPDATE watches SET state = 'fired', fired_at = $2, changed_at = $2 WHERE id = $1 RETURNING {WATCH_COLUMNS}"
  ))
  .bind(id)
  .bind(at)
  .fetch_one(&mut *conn)
  .await?;
  let watch = row.member()?;
  publish(conn, "watch.fired", Some(&watch.scope), &watch).await?;
  Ok(watch)
}

/// Every armed watch waiting on row `memory_id` reaching `state`, fired.
pub(crate) async fn row_moved(
  conn: &mut PgConnection,
  memory_id: i64,
  state: MemoryState,
  at: i64,
) -> Held<()> {
  let waiting: Vec<i64> = sqlx::query_scalar(
    "SELECT id FROM watches WHERE memory_id = $1 AND state = 'armed' AND condition->>'state' = $2 \
     ORDER BY id FOR UPDATE",
  )
  .bind(memory_id)
  .bind(state.name())
  .fetch_all(&mut *conn)
  .await?;
  for id in waiting {
    fire(conn, id, at).await?;
  }
  Ok(())
}

impl Store {
  /// A page of watches.
  pub async fn watches(&self, query: &WatchesQuery) -> Held<Page<Watch>> {
    let limit = limit(query.limit)?;
    let mut sql = QueryBuilder::<Postgres>::new(format!("SELECT {WATCH_COLUMNS} FROM watches WHERE TRUE"));
    if let Some(scope) = &query.scope {
      sql.push(" AND scope = ").push_bind(scope.clone());
    }
    if let Some(setter) = &query.setter_name {
      sql.push(" AND setter_name = ").push_bind(setter.clone());
    }
    if let Some(holder) = &query.holder_name {
      sql.push(" AND holder_name = ").push_bind(holder.clone());
    }
    if let Some(state) = query.state {
      sql.push(" AND state = ").push_bind(state.name());
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<WatchRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    Ok(page(rows.into_iter().map(WatchRow::member).collect::<Held<_>>()?, limit))
  }

  /// A watch set: armed, or fired at once where its condition holds already.
  pub async fn watch_add(&self, body: &NewWatch) -> Held<Watch> {
    scope("scope", &body.scope)?;
    name("setterName", &body.setter_name)?;
    let waits = waits(&body.condition)?;
    let at = now();
    let mut tx = self.begin().await?;
    let (moment, row, holds) = match waits {
      Waits::At(moment) => (Some(moment), None, false),
      Waits::Row(row, state) => {
        let stands: Option<String> = sqlx::query_scalar("SELECT state FROM memories WHERE id = $1")
          .bind(row)
          .fetch_optional(&mut *tx)
          .await?;
        let Some(stands) = stands else {
          return Err(Fault::BadRequest(format!(
            "The condition names memory row {row}, which there is not."
          )));
        };
        (None, Some(row), stands == state.name())
      }
      Waits::External => (None, None, false),
    };
    let set: WatchRow = sqlx::query_as(&format!(
      "INSERT INTO watches (scope, condition, state, setter_name, labels, created_at, changed_at, at, memory_id) \
       VALUES ($1, $2, 'armed', $3, $4, $5, $5, $6, $7) RETURNING {WATCH_COLUMNS}"
    ))
    .bind(&body.scope)
    .bind(Json(&body.condition))
    .bind(&body.setter_name)
    .bind(Json(&body.labels))
    .bind(at)
    .bind(moment)
    .bind(row)
    .fetch_one(&mut *tx)
    .await?;
    let watch = set.member()?;
    publish(&mut tx, "watch.changed", Some(&watch.scope), &watch).await?;
    let watch = if holds { fire(&mut tx, watch.id, at).await? } else { watch };
    self.commit(tx).await?;
    Ok(watch)
  }

  /// One watch.
  pub async fn watch(&self, id: i64) -> Held<Watch> {
    read(&mut *self.connection().await?, id, false).await?.member()
  }

  /// A watch moved on.
  pub async fn watch_change(&self, id: i64, body: &WatchChange) -> Held<Watch> {
    if body.state == WatchState::Claimed {
      match &body.holder_name {
        Some(holder) => name("holderName", holder)?,
        None => return Err(Fault::BadRequest("A claim names its holderName.".to_owned())),
      }
    }
    let mut tx = self.begin().await?;
    let row = read(&mut tx, id, true).await?;
    let was = row.state()?;
    let at = now();
    if was == WatchState::Claimed && body.state == WatchState::Claimed {
      if row.holder_name == body.holder_name {
        return row.member();
      }
      return Err(Fault::Conflict(format!(
        "Watch {id} is claimed by {}.",
        row.holder_name.as_deref().unwrap_or("another holder")
      )));
    }
    if !was.may_move_to(body.state) {
      return Err(Fault::Conflict(format!(
        "Watch {id} is {}; it does not move to {}.",
        was.name(),
        body.state.name()
      )));
    }
    let watch = if body.state == WatchState::Fired {
      fire(&mut tx, id, at).await?
    } else {
      let holder = if body.state == WatchState::Claimed {
        body.holder_name.clone()
      } else {
        row.holder_name
      };
      let moved: WatchRow = sqlx::query_as(&format!(
        "UPDATE watches SET state = $2, holder_name = $3, changed_at = $4 WHERE id = $1 RETURNING {WATCH_COLUMNS}"
      ))
      .bind(id)
      .bind(body.state.name())
      .bind(holder)
      .bind(at)
      .fetch_one(&mut *tx)
      .await?;
      let watch = moved.member()?;
      publish(&mut tx, "watch.changed", Some(&watch.scope), &watch).await?;
      watch
    };
    self.commit(tx).await?;
    Ok(watch)
  }

  /// Every armed watch whose moment has come by `at`, fired; how many.
  pub async fn fire_moments(&self, at: i64) -> Held<usize> {
    let mut tx = self.begin().await?;
    let due: Vec<i64> = sqlx::query_scalar(
      "SELECT id FROM watches WHERE state = 'armed' AND at <= $1 ORDER BY at, id LIMIT 500 FOR UPDATE SKIP LOCKED",
    )
    .bind(at)
    .fetch_all(&mut *tx)
    .await?;
    for &id in &due {
      fire(&mut tx, id, at).await?;
    }
    self.commit(tx).await?;
    Ok(due.len())
  }

  /// A page of reminders.
  pub async fn reminders(&self, query: &RemindersQuery) -> Held<Page<Reminder>> {
    let limit = limit(query.limit)?;
    let mut sql =
      QueryBuilder::<Postgres>::new(format!("SELECT {REMINDER_COLUMNS} FROM reminders WHERE TRUE"));
    if let Some(scope) = &query.scope {
      sql.push(" AND scope = ").push_bind(scope.clone());
    }
    if let Some(before) = query.due_before {
      sql.push(" AND due_at <= ").push_bind(before);
    }
    match query.fired {
      Some(true) => {
        sql.push(" AND fired_at IS NOT NULL");
      }
      Some(false) => {
        sql.push(" AND fired_at IS NULL");
      }
      None => {}
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<ReminderRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    Ok(page(rows.into_iter().map(ReminderRow::member).collect(), limit))
  }

  /// A reminder set.
  pub async fn reminder_add(&self, body: &NewReminder) -> Held<Reminder> {
    scope("scope", &body.scope)?;
    text("text", &body.text, TEXT_MOST)?;
    name("setterName", &body.setter_name)?;
    let mut tx = self.begin().await?;
    let row: ReminderRow = sqlx::query_as(&format!(
      "INSERT INTO reminders (scope, due_at, text, labels, setter_name, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6) RETURNING {REMINDER_COLUMNS}"
    ))
    .bind(&body.scope)
    .bind(body.due_at)
    .bind(&body.text)
    .bind(Json(&body.labels))
    .bind(&body.setter_name)
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    let reminder = row.member();
    publish(&mut tx, "reminder.created", Some(&reminder.scope), &reminder).await?;
    self.commit(tx).await?;
    Ok(reminder)
  }

  /// A reminder cancelled.
  pub async fn reminder_cancel(&self, id: i64) -> Held<()> {
    let mut tx = self.begin().await?;
    let scope: Option<String> = sqlx::query_scalar("DELETE FROM reminders WHERE id = $1 RETURNING scope")
      .bind(id)
      .fetch_optional(&mut *tx)
      .await?;
    let Some(scope) = scope else {
      return Err(Fault::NotFound(format!("There is no reminder {id}.")));
    };
    publish(&mut tx, "reminder.deleted", Some(&scope), &json!({ "id": id })).await?;
    self.commit(tx).await
  }

  /// Every reminder whose moment has come by `at`, fired; how many.
  pub async fn fire_reminders(&self, at: i64) -> Held<usize> {
    let mut tx = self.begin().await?;
    let due: Vec<ReminderRow> = sqlx::query_as(&format!(
      "UPDATE reminders SET fired_at = $1 WHERE id IN (SELECT id FROM reminders WHERE fired_at IS NULL \
       AND due_at <= $1 ORDER BY due_at, id LIMIT 500 FOR UPDATE SKIP LOCKED) RETURNING {REMINDER_COLUMNS}"
    ))
    .bind(at)
    .fetch_all(&mut *tx)
    .await?;
    let mut due: Vec<Reminder> = due.into_iter().map(ReminderRow::member).collect();
    due.sort_by_key(|reminder| (reminder.due_at, reminder.id));
    for reminder in &due {
      publish(&mut tx, "reminder.due", Some(&reminder.scope), reminder).await?;
    }
    self.commit(tx).await?;
    Ok(due.len())
  }
}

const REMINDER_COLUMNS: &str = "id, scope, due_at, text, labels, setter_name, created_at, fired_at";

#[derive(FromRow)]
struct ReminderRow {
  id: i64,
  scope: String,
  due_at: i64,
  text: String,
  labels: Json<Labels>,
  setter_name: String,
  created_at: i64,
  fired_at: Option<i64>,
}

impl ReminderRow {
  fn member(self) -> Reminder {
    Reminder {
      id: self.id,
      scope: self.scope,
      due_at: self.due_at,
      text: self.text,
      labels: self.labels.0,
      setter_name: self.setter_name,
      created_at: self.created_at,
      fired_at: self.fired_at,
    }
  }
}
