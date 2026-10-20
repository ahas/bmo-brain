//! Locks on the store: who is editing it (docs/boundary-api.md, 「Locks」).

use bmo_brain_client::{Lock, LocksQuery, NewLock, Page};
use sqlx::{FromRow, PgConnection};

use super::{Fault, Held, Store, limit, name, now, page, publish, text};

const LOCK_COLUMNS: &str = "id, holder_name, reason, created_at, expires_at";

/// The lock taken while a lock is weighed, so two takers never both find the store free.
const TAKING: i64 = 0x6c6f_636b;

#[derive(FromRow)]
struct LockRow {
  id: i64,
  holder_name: String,
  reason: String,
  created_at: i64,
  expires_at: Option<i64>,
}

impl LockRow {
  fn member(self) -> Lock {
    Lock {
      id: self.id,
      holder_name: self.holder_name,
      reason: self.reason,
      created_at: self.created_at,
      expires_at: self.expires_at,
    }
  }
}

/// The locks that stand at `at`.
async fn standing(conn: &mut PgConnection, at: i64, after: i64, limit: i64) -> Held<Vec<Lock>> {
  let rows: Vec<LockRow> = sqlx::query_as(&format!(
    "SELECT {LOCK_COLUMNS} FROM locks WHERE (expires_at IS NULL OR expires_at > $1) AND id > $2 \
     ORDER BY id LIMIT $3"
  ))
  .bind(at)
  .bind(after)
  .bind(limit)
  .fetch_all(&mut *conn)
  .await?;
  Ok(rows.into_iter().map(LockRow::member).collect())
}

/// The longest a reader waits for the store to come free.
pub const LONGEST_WAIT_MS: i64 = 600_000;

impl Store {
  /// A page of the locks that stand.
  pub async fn locks(&self, query: &LocksQuery) -> Held<Page<Lock>> {
    let limit = limit(query.limit)?;
    let locks =
      standing(&mut *self.connection().await?, now(), query.after.unwrap_or(0), limit + 1).await?;
    Ok(page(locks, limit))
  }

  /// The lock taken: whether it is new rather than renewed, and the lock.
  pub async fn lock(&self, body: &NewLock) -> Held<(bool, Lock)> {
    name("holderName", &body.holder_name)?;
    text("reason", &body.reason, 2000)?;
    if body.ttl_ms.is_some_and(|ttl| ttl < 1) {
      return Err(Fault::BadRequest("ttlMs is at least 1.".to_owned()));
    }
    let at = now();
    let expires = body.ttl_ms.map(|ttl| at.saturating_add(ttl));
    let mut tx = self.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(TAKING).execute(&mut *tx).await?;
    let held = standing(&mut tx, at, 0, 1).await?;
    let (created, row) = match held.first() {
      Some(lock) if lock.holder_name != body.holder_name => {
        return Err(Fault::Conflict(format!("{} holds the store: {}.", lock.holder_name, lock.reason)));
      }
      Some(lock) => {
        let row: LockRow = sqlx::query_as(&format!(
          "UPDATE locks SET reason = $2, expires_at = $3 WHERE id = $1 RETURNING {LOCK_COLUMNS}"
        ))
        .bind(lock.id)
        .bind(&body.reason)
        .bind(expires)
        .fetch_one(&mut *tx)
        .await?;
        (false, row)
      }
      None => {
        let row: LockRow = sqlx::query_as(&format!(
          "INSERT INTO locks (holder_name, reason, created_at, expires_at) VALUES ($1, $2, $3, $4) \
           RETURNING {LOCK_COLUMNS}"
        ))
        .bind(&body.holder_name)
        .bind(&body.reason)
        .bind(at)
        .bind(expires)
        .fetch_one(&mut *tx)
        .await?;
        (true, row)
      }
    };
    let lock = row.member();
    publish(&mut tx, "lock.changed", None, &lock).await?;
    self.commit(tx).await?;
    Ok((created, lock))
  }

  /// A lock let go.
  pub async fn unlock(&self, id: i64) -> Held<()> {
    let mut tx = self.begin().await?;
    let row: Option<LockRow> =
      sqlx::query_as(&format!("DELETE FROM locks WHERE id = $1 RETURNING {LOCK_COLUMNS}"))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(row) = row else {
      return Err(Fault::NotFound(format!("There is no lock {id}.")));
    };
    publish(&mut tx, "lock.changed", None, &row.member()).await?;
    self.commit(tx).await
  }

  /// Every lock run out by `at`, let go; how many.
  pub async fn expire_locks(&self, at: i64) -> Held<usize> {
    let mut tx = self.begin().await?;
    let rows: Vec<LockRow> =
      sqlx::query_as(&format!("DELETE FROM locks WHERE expires_at <= $1 RETURNING {LOCK_COLUMNS}"))
        .bind(at)
        .fetch_all(&mut *tx)
        .await?;
    let count = rows.len();
    for row in rows {
      publish(&mut tx, "lock.changed", None, &row.member()).await?;
    }
    self.commit(tx).await?;
    Ok(count)
  }
}
