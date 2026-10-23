//! What the brain holds under a caller's scope: read, marked forgotten or remembered, and let go of
//! (docs/boundary-api.md, 「Scopes」).

use bmo_brain_client::{ScopeCounts, ScopeHolding, ScopeState};
use serde_json::json;
use sqlx::PgConnection;

use super::{Fault, Held, Store, now, publish, scope};

/// The rows of every resource held under a scope, and the matches on its rows.
const COUNTS: &str = "SELECT \
  (SELECT count(*) FROM scoped_memories WHERE scope = $1), \
  (SELECT count(*) FROM memory_matches WHERE memory_ids && ARRAY(SELECT id FROM scoped_memories WHERE scope = $1)), \
  (SELECT count(*) FROM notes WHERE scope = $1), \
  (SELECT count(*) FROM watches WHERE scope = $1), \
  (SELECT count(*) FROM reminders WHERE scope = $1), \
  (SELECT count(*) FROM fragments WHERE scope = $1), \
  (SELECT count(*) FROM fragment_hits WHERE scope = $1), \
  (SELECT state FROM scopes WHERE scope = $1)";

type Counted = (i64, i64, i64, i64, i64, i64, i64, Option<String>);

/// The holding under a scope, or not found where nothing is held and the scope was never marked.
async fn holding(conn: &mut PgConnection, name: &str) -> Held<ScopeHolding> {
  scope("scope", name)?;
  let counted: Counted = sqlx::query_as(COUNTS).bind(name).fetch_one(&mut *conn).await?;
  let (memories, memory_matches, notes, watches, reminders, fragments, fragment_hits, state) = counted;
  let counts = ScopeCounts {
    memories,
    memory_matches,
    notes,
    watches,
    reminders,
    fragments,
    fragment_hits,
  };
  let state = match state.as_deref() {
    None if counts.is_empty() => {
      return Err(Fault::NotFound(format!("Nothing is held under the scope {name:?}.")));
    }
    Some("forgotten") => ScopeState::Forgotten,
    _ => ScopeState::Remembered,
  };
  Ok(ScopeHolding {
    scope: name.to_owned(),
    counts,
    state,
  })
}

/// The withering span: a forgotten scope's standing row unmatched this long moves to `off`.
pub const WITHER_AFTER_MS: i64 = 30 * 24 * 60 * 60 * 1000;

impl Store {
  /// What is held under a scope.
  pub async fn scope(&self, name: &str) -> Held<ScopeHolding> {
    holding(&mut *self.connection().await?, name).await
  }

  /// A scope marked forgotten or remembered.
  pub async fn scope_mark(&self, name: &str, state: ScopeState) -> Held<ScopeHolding> {
    let mut tx = self.begin().await?;
    holding(&mut tx, name).await?;
    let word = match state {
      ScopeState::Remembered => "remembered",
      ScopeState::Forgotten => "forgotten",
    };
    sqlx::query(
      "INSERT INTO scopes (scope, state, changed_at) VALUES ($1, $2, $3) \
       ON CONFLICT (scope) DO UPDATE SET state = EXCLUDED.state, changed_at = EXCLUDED.changed_at",
    )
    .bind(name)
    .bind(word)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    let marked = holding(&mut tx, name).await?;
    publish(&mut tx, "scope.changed", Some(name), &marked).await?;
    self.commit(tx).await?;
    Ok(marked)
  }

  /// Everything held under a scope let go of, or with `dry_run` only read; the holding as it stood.
  pub async fn scope_let_go(&self, name: &str, dry_run: bool) -> Held<ScopeHolding> {
    let mut tx = self.begin().await?;
    let held = holding(&mut tx, name).await?;
    if dry_run {
      return Ok(held);
    }
    sqlx::query(
      "DELETE FROM memory_matches WHERE memory_ids && ARRAY(SELECT id FROM scoped_memories WHERE scope = $1)",
    )
    .bind(name)
    .execute(&mut *tx)
    .await?;
    for table in ["scoped_memories", "notes", "watches", "reminders", "fragment_hits", "scopes"] {
      sqlx::query(&format!("DELETE FROM {table} WHERE scope = $1"))
        .bind(name)
        .execute(&mut *tx)
        .await?;
    }
    let fragments: Vec<String> = sqlx::query_scalar("DELETE FROM fragments WHERE scope = $1 RETURNING key")
      .bind(name)
      .fetch_all(&mut *tx)
      .await?;
    for key in fragments {
      publish(&mut tx, "fragment.deleted", Some(name), &json!({ "key": key })).await?;
    }
    publish(&mut tx, "scope.changed", Some(name), &held).await?;
    self.commit(tx).await?;
    Ok(held)
  }
}
