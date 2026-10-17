//! A statement prepared on the database's shape before a column was added, run again on the same pooled connection
//! after it: prepared afresh on its own, and failing alone inside a transaction that met the old plan. Against the
//! machine's own server, in a scratch schema of the test's own.

use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use brain_support::database::{all, connect, one, pool};
use sea_orm::{DatabaseConnection, TransactionTrait};
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::{Connection, Executor};

// The mark a stale plan leaves stands for the whole process and closes every connection opened before it, so these
// tests take turns rather than close each other's.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn test_address() -> String {
  std::env::var("BMO_TEST_POSTGRES")
    .unwrap_or_else(|_| "postgres:///bmo_test?host=/var/run/postgresql".into())
}

// A scratch schema holding one table of one row, the connection that made it, and the address reading that schema.
async fn scratch(name: &str) -> (PgConnection, String, String) {
  let options = PgConnectOptions::from_str(&test_address()).unwrap();
  let mut admin = PgConnection::connect_with(&options).await.unwrap();
  let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
  let schema = format!("t_{seconds}_{}_{name}", std::process::id());

  admin
    .execute(
      format!(
        "CREATE SCHEMA \"{schema}\"; CREATE TABLE \"{schema}\".shape_probe (id BIGINT PRIMARY KEY); INSERT INTO \"{schema}\".shape_probe (id) VALUES (1)"
      )
      .as_str(),
    )
    .await
    .unwrap();

  let separator = if test_address().contains('?') { '&' } else { '?' };
  let address = format!("{}{separator}options[search_path]={schema}", test_address());

  (admin, schema, address)
}

async fn drop_schema(admin: &mut PgConnection, schema: &str) {
  admin.execute(format!("DROP SCHEMA \"{schema}\" CASCADE").as_str()).await.unwrap();
}

// A pool of one connection, so every statement of the test stands on the connection the first one prepared on.
async fn single(address: &str) -> DatabaseConnection {
  connect(address, pool(1)).await.unwrap()
}

#[tokio::test]
async fn a_statement_reading_every_column_survives_a_column_added_under_it() {
  let _turn = ONE_AT_A_TIME.lock().await;
  let (mut admin, schema, address) = scratch("survives").await;
  let db = single(&address).await;
  let read = "SELECT * FROM shape_probe ORDER BY id";
  let mut connection = db.get_postgres_connection_pool().acquire().await.unwrap();

  assert_eq!(sqlx::query(read).fetch_all(&mut *connection).await.unwrap().len(), 1);

  admin
    .execute(
      format!("ALTER TABLE \"{schema}\".shape_probe ADD COLUMN added TEXT NOT NULL DEFAULT 'filled'")
        .as_str(),
    )
    .await
    .unwrap();

  // The plan the connection keeps is refused by the server as it stands.
  let refused = sqlx::query(read).fetch_all(&mut *connection).await.unwrap_err();

  assert_eq!(refused.as_database_error().and_then(|fault| fault.code()).as_deref(), Some("0A000"));
  drop(connection);

  let rows = all(&db, read, vec![]).await.unwrap();

  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0].try_get::<String>("", "added").unwrap(), "filled");
  db.close().await.unwrap();
  drop_schema(&mut admin, &schema).await;
}

#[tokio::test]
async fn a_transaction_that_met_the_old_plan_fails_alone() {
  let _turn = ONE_AT_A_TIME.lock().await;
  let (mut admin, schema, address) = scratch("alone").await;
  let db = single(&address).await;
  let read = "SELECT * FROM shape_probe WHERE id = $1";

  assert!(one(&db, read, vec![1i64.into()]).await.unwrap().is_some());

  admin
    .execute(format!("ALTER TABLE \"{schema}\".shape_probe ADD COLUMN added TEXT").as_str())
    .await
    .unwrap();

  let transaction = db.begin().await.unwrap();

  assert!(one(&transaction, read, vec![1i64.into()]).await.is_err());
  transaction.rollback().await.unwrap();
  assert!(one(&db, read, vec![1i64.into()]).await.unwrap().is_some());
  db.close().await.unwrap();
  drop_schema(&mut admin, &schema).await;
}
