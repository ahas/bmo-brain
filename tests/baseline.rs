//! The schema's starting point (docs/design.md, 「The schema」): a blank database gets the baseline whole, an
//! install holding the last record before 1.0.0 is taken as it stands, one holding part of it is refused, and the
//! migration after the baseline removes the imported closing labels and nothing else.

use crate::support::Bare;

use bmo_brain::store::{MIGRATOR, Store};
use sqlx::PgConnection;

const BASELINE: &str = include_str!("../migrations/0001_baseline.sql");

// The record every migrated database holds.
const MIGRATED: [(i64, &str); 2] = [(1, "baseline"), (2, "imported closing labels")];

// The record an install from before 1.0.0 holds once every migration of its last release ran.
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

// The record's table as the migrator makes it.
const RECORD: &str = "CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, \
  installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, \
  execution_time BIGINT NOT NULL)";

// Every column and index of the public schema but the record's, in one line.
async fn shape(db: &mut PgConnection) -> String {
  let columns: String = sqlx::query_scalar(
    "SELECT string_agg(table_name || '.' || column_name || ' ' || data_type || ' ' || is_nullable || ' ' || \
     COALESCE(column_default, ''), ', ' ORDER BY table_name, ordinal_position) \
     FROM information_schema.columns WHERE table_schema = 'public' AND table_name <> '_sqlx_migrations'",
  )
  .fetch_one(&mut *db)
  .await
  .unwrap();
  let indexes: String = sqlx::query_scalar(
    "SELECT string_agg(indexdef, ', ' ORDER BY indexname) FROM pg_indexes \
     WHERE schemaname = 'public' AND tablename <> '_sqlx_migrations'",
  )
  .fetch_one(&mut *db)
  .await
  .unwrap();
  format!("{columns}\n{indexes}")
}

async fn recorded(db: &mut PgConnection) -> Vec<(i64, String)> {
  sqlx::query_as("SELECT version, description FROM _sqlx_migrations ORDER BY version")
    .fetch_all(&mut *db)
    .await
    .unwrap()
}

fn migrated() -> Vec<(i64, String)> {
  MIGRATED
    .iter()
    .map(|(version, description)| (*version, (*description).to_owned()))
    .collect()
}

// A database holding the baseline's schema and a brain, recorded as an install from before 1.0.0 that ran
// the first `through` migrations of its last release.
async fn installed(through: usize) -> Bare {
  let bare = Bare::blank().await;
  let mut db = bare.connect().await;
  sqlx::raw_sql(BASELINE).execute(&mut db).await.unwrap();
  sqlx::raw_sql(RECORD).execute(&mut db).await.unwrap();
  for (version, description) in &BEFORE[..through] {
    sqlx::query(
      "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
       VALUES ($1, $2, TRUE, '\\x00', 1)",
    )
    .bind(version)
    .bind(description)
    .execute(&mut db)
    .await
    .unwrap();
  }
  sqlx::query(
    "INSERT INTO brains (account_key, agent_key, name, created_at) VALUES ('account-a', 'agent-a', 'a', 1)",
  )
  .execute(&mut db)
  .await
  .unwrap();
  bare
}

#[tokio::test]
async fn a_blank_database_gets_the_baseline_whole() {
  let bare = Bare::blank().await;
  Store::over(bare.pool().await).migrate().await.unwrap();
  let mut db = bare.connect().await;
  assert_eq!(recorded(&mut db).await, migrated());
  let floor: i64 = sqlx::query_scalar("SELECT pruned_through FROM event_floor")
    .fetch_one(&mut db)
    .await
    .unwrap();
  assert_eq!(floor, 0);

  let laid = Bare::blank().await;
  let mut plain = laid.connect().await;
  sqlx::raw_sql(BASELINE).execute(&mut plain).await.unwrap();
  assert_eq!(shape(&mut db).await, shape(&mut plain).await);
}

#[tokio::test]
async fn an_install_at_the_last_release_before_the_baseline_only_has_its_record_rewritten() {
  let bare = installed(BEFORE.len()).await;
  let mut db = bare.connect().await;
  let before = shape(&mut db).await;

  Store::over(bare.pool().await).migrate().await.unwrap();
  assert_eq!(recorded(&mut db).await, migrated());
  assert_eq!(shape(&mut db).await, before);
  let brains: Vec<String> =
    sqlx::query_scalar("SELECT agent_key FROM brains").fetch_all(&mut db).await.unwrap();
  assert_eq!(brains, ["agent-a"]);

  // A second start finds the baseline recorded and changes nothing.
  Store::over(bare.pool().await).migrate().await.unwrap();
  assert_eq!(recorded(&mut db).await, migrated());
  assert_eq!(shape(&mut db).await, before);
}

#[tokio::test]
async fn an_install_short_of_the_last_release_before_the_baseline_is_refused() {
  let bare = installed(7).await;
  let refused = Store::over(bare.pool().await).migrate().await.unwrap_err();
  assert!(refused.contains("through version 7"), "{refused}");
  let mut db = bare.connect().await;
  assert_eq!(recorded(&mut db).await.len(), 7);
}

// Every column of every memory, the labels apart, and the labels, ordered by id.
async fn memories(db: &mut PgConnection) -> (Vec<String>, Vec<serde_json::Value>) {
  let rest: Vec<String> =
    sqlx::query_scalar("SELECT (to_jsonb(m) - 'labels')::text FROM memories m ORDER BY id")
      .fetch_all(&mut *db)
      .await
      .unwrap();
  let labels: Vec<serde_json::Value> = sqlx::query_scalar("SELECT labels FROM memories ORDER BY id")
    .fetch_all(&mut *db)
    .await
    .unwrap();
  (rest, labels)
}

#[tokio::test]
async fn the_migration_after_the_baseline_removes_the_imported_closing_labels_and_nothing_else() {
  let bare = Bare::blank().await;
  let mut db = bare.connect().await;
  let baseline = MIGRATOR.iter().next().unwrap();
  sqlx::raw_sql(BASELINE).execute(&mut db).await.unwrap();
  sqlx::raw_sql(RECORD).execute(&mut db).await.unwrap();
  sqlx::query(
    "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
     VALUES ($1, $2, TRUE, $3, 1)",
  )
  .bind(baseline.version)
  .bind(baseline.description.as_ref())
  .bind(baseline.checksum.as_ref())
  .execute(&mut db)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO brains (account_key, agent_key, name, created_at) VALUES ('account-a', 'agent-a', 'a', 1)",
  )
  .execute(&mut db)
  .await
  .unwrap();
  for (status, labels) in [
    (
      "done",
      r#"{"done_text": "It ended.", "down_reason": "spent", "down_at": "5", "slug": "row-a"}"#,
    ),
    ("invalidated", r#"{"down_reason": "spent", "down_at": "5"}"#),
    ("active", r#"{"slug": "row-c", "source": "writer-a"}"#),
    ("superseded", r#"{"done_text": "It ended.", "source": "writer-a"}"#),
  ] {
    sqlx::query(
      "INSERT INTO memories (brain_id, type, text, status, lineage_id, valid_from, importance, actor_name, labels, \
       created_at, reason) SELECT id, 'check', 'A check', $1, 0, 1, 5, 'writer-a', $2::jsonb, 1, 'why' FROM brains",
    )
    .bind(status)
    .bind(labels)
    .execute(&mut db)
    .await
    .unwrap();
  }
  let (rest, _) = memories(&mut db).await;

  Store::over(bare.pool().await).migrate().await.unwrap();
  assert_eq!(recorded(&mut db).await, migrated());
  let (after, labels) = memories(&mut db).await;
  assert_eq!(after, rest);
  assert_eq!(
    labels,
    [
      serde_json::json!({ "slug": "row-a" }),
      serde_json::json!({}),
      serde_json::json!({ "slug": "row-c", "source": "writer-a" }),
      serde_json::json!({ "source": "writer-a" }),
    ]
  );
}
