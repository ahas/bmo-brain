//! The events every change publishes: read by number, narrowed by kind and scope, and let go of once they pass
//! the window the brain keeps (docs/boundary-api.md, 「Events」).

use bmo_brain_client::{Event, Page};
use serde_json::Value;
use sqlx::types::Json;
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::{Fault, Held, Store, listed};

/// How long the brain keeps an event.
pub const WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Which events a reader reads: past `after`, of these kinds, of members under this scope.
#[derive(Debug, Clone, Default)]
pub struct Narrowing {
  pub kinds: Vec<String>,
  pub scope: Option<String>,
}

impl Narrowing {
  /// The narrowing a query's `kinds` and `scope` ask for.
  pub fn of(kinds: Option<&str>, scope: Option<String>) -> Self {
    Self {
      kinds: listed(kinds),
      scope,
    }
  }
}

#[derive(FromRow)]
struct EventRow {
  id: i64,
  kind: String,
  at: i64,
  data: Json<Value>,
}

impl Store {
  /// The events past `after`, at most `limit`, refused as gone where `after` is before the oldest kept.
  pub async fn events(&self, after: i64, narrowing: &Narrowing, limit: i64) -> Held<Page<Event>> {
    let floor: i64 = sqlx::query_scalar("SELECT pruned_through FROM event_floor")
      .fetch_one(&self.pool)
      .await?;
    if after < floor {
      return Err(Fault::Gone(format!(
        "Events up to {floor} are let go of; read the collections again and resume after the latest event."
      )));
    }
    let mut sql = QueryBuilder::<Postgres>::new("SELECT id, kind, at, data FROM events WHERE id > ");
    sql.push_bind(after);
    if !narrowing.kinds.is_empty() {
      sql.push(" AND kind = ANY(").push_bind(narrowing.kinds.clone()).push(")");
    }
    if let Some(scope) = &narrowing.scope {
      sql.push(" AND scope = ").push_bind(scope.clone());
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<EventRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    let events = rows
      .into_iter()
      .map(|row| Event {
        id: row.id,
        kind: row.kind,
        at: row.at,
        data: row.data.0,
      })
      .collect();
    Ok(super::page(events, limit))
  }

  /// The number of the latest event, or 0 where none was ever published.
  pub async fn latest_event(&self) -> Held<i64> {
    let latest: Option<i64> =
      sqlx::query_scalar("SELECT max(id) FROM events").fetch_one(&self.pool).await?;
    let floor: i64 = sqlx::query_scalar("SELECT pruned_through FROM event_floor")
      .fetch_one(&self.pool)
      .await?;
    Ok(latest.unwrap_or(floor))
  }

  /// The events older than `before` let go of; how many.
  pub async fn prune_events(&self, before: i64) -> Held<u64> {
    let mut tx = self.begin().await?;
    let through: Option<i64> = sqlx::query_scalar("SELECT max(id) FROM events WHERE at < $1")
      .bind(before)
      .fetch_one(&mut *tx)
      .await?;
    let Some(through) = through else {
      return Ok(0);
    };
    let pruned = sqlx::query("DELETE FROM events WHERE id <= $1").bind(through).execute(&mut *tx).await?;
    sqlx::query("UPDATE event_floor SET pruned_through = GREATEST(pruned_through, $1)")
      .bind(through)
      .execute(&mut *tx)
      .await?;
    tx.commit().await?;
    Ok(pruned.rows_affected())
  }
}
