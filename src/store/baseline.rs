//! The schema's starting point (docs/design.md, 「The schema」): the 1.0.0 baseline, and an install from before
//! it taken as it stands.

use sqlx::{PgPool, Row};

use super::MIGRATOR;

/// The record of migrations an install from before 1.0.0 holds once every migration of its last release ran,
/// each by its version and description. Its schema is the baseline's.
const BEFORE: &[(i64, &str)] = &[
  (1, "brain"),
  (2, "caller records"),
  (3, "brains"),
  (4, "old members"),
  (5, "move reason"),
  (6, "outside brains"),
  (7, "memory conditions"),
  (8, "no strength"),
  (9, "memory words"),
  (10, "superseded at"),
  (11, "search misses"),
  (12, "memory vectors"),
  (13, "vector rejections"),
  (14, "org brains"),
  (15, "shared mark"),
];

/// The migrations run, the baseline first: a database holding the whole record of the last release before
/// 1.0.0 has that record rewritten to name the baseline alone, touching no table and no row; one holding
/// only part of it is refused.
pub async fn migrate(pool: &PgPool) -> Result<(), String> {
  adopt(pool).await?;
  MIGRATOR.run(pool).await.map_err(|why| why.to_string())
}

async fn adopt(pool: &PgPool) -> Result<(), String> {
  let failed = |why: sqlx::Error| format!("the record of migrations: {why}");
  let mut tx = pool.begin().await.map_err(failed)?;
  let recorded: bool = sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
    .fetch_one(&mut *tx)
    .await
    .map_err(failed)?;
  if !recorded {
    return Ok(());
  }
  sqlx::query("LOCK TABLE _sqlx_migrations IN EXCLUSIVE MODE")
    .execute(&mut *tx)
    .await
    .map_err(failed)?;
  let rows = sqlx::query("SELECT version, description, success FROM _sqlx_migrations ORDER BY version")
    .fetch_all(&mut *tx)
    .await
    .map_err(failed)?;
  let held: Vec<(i64, String, bool)> = rows
    .iter()
    .map(|row| (row.get("version"), row.get("description"), row.get("success")))
    .collect();
  let before = |(version, description, success): &(i64, String, bool)| {
    *success && BEFORE.iter().any(|(old, named)| old == version && named == description)
  };
  if held.is_empty() || !held.iter().all(before) {
    return Ok(());
  }
  if held.len() != BEFORE.len() {
    let reached = held.last().map_or(0, |(version, _, _)| *version);
    return Err(format!(
      "the database holds the migrations of a release before 1.0.0 only through version {reached}; \
       that release's last migration runs first"
    ));
  }
  let baseline = MIGRATOR.iter().next().ok_or("the baseline migration is missing")?;
  sqlx::query("DELETE FROM _sqlx_migrations").execute(&mut *tx).await.map_err(failed)?;
  sqlx::query(
    "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
     VALUES ($1, $2, TRUE, $3, 0)",
  )
  .bind(baseline.version)
  .bind(baseline.description.as_ref())
  .bind(baseline.checksum.as_ref())
  .execute(&mut *tx)
  .await
  .map_err(failed)?;
  tx.commit().await.map_err(failed)
}
