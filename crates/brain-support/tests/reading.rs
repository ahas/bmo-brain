//! Where a reader reads the database, and that it reads either database: the PostgreSQL side against the
//! machine's own server, in a scratch schema of the test's own.

use std::fs;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use brain_support::reading::{current_place, open_read};
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::{Connection, Executor, Row};

fn test_address() -> String {
  std::env::var("BMO_TEST_POSTGRES")
    .unwrap_or_else(|_| "postgres:///bmo_test?host=/var/run/postgresql".into())
}

#[test]
fn a_data_roots_own_file_is_read_where_its_setting_points() {
  let root = tempfile::tempdir().unwrap();
  let file = root.path().join("bmo.db");
  let address = "postgres:///bmo?host=/var/run/postgresql";

  assert_eq!(current_place(&file), file.to_string_lossy());

  fs::create_dir_all(root.path().join("env")).unwrap();
  fs::write(root.path().join("env").join("archive-url"), "\n").unwrap();
  assert_eq!(current_place(&file), file.to_string_lossy());

  fs::write(root.path().join("env").join("archive-url"), format!("{address}\n")).unwrap();
  assert_eq!(current_place(&file), address);

  let copy = root.path().join("copy.db");

  assert_eq!(current_place(&copy), copy.to_string_lossy());
}

#[tokio::test]
async fn a_postgresql_address_is_read_as_the_file_would_be() {
  let options = PgConnectOptions::from_str(&test_address()).unwrap();
  let mut admin = PgConnection::connect_with(&options).await.unwrap();
  let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
  let schema = format!("t_{seconds}_{}_read", std::process::id());

  admin.execute(format!("CREATE SCHEMA \"{schema}\"").as_str()).await.unwrap();

  let read = async {
    admin
      .execute(
        format!("CREATE TABLE \"{schema}\".todos (id bigint PRIMARY KEY, title text NOT NULL, at bigint, score double precision); INSERT INTO \"{schema}\".todos VALUES (7, 'tidy up', 1700000000, 0.5)")
          .as_str(),
      )
      .await
      .unwrap();

    let separator = if test_address().contains('?') { '&' } else { '?' };
    let address = format!("{}{separator}options[search_path]={schema}", test_address());
    let mut database = open_read(std::path::Path::new(&address)).await.unwrap();
    let row = sqlx::query("SELECT id, title, at, score FROM todos WHERE id = $1")
      .bind(7_i64)
      .fetch_one(&mut database)
      .await
      .unwrap();

    assert_eq!(row.try_get::<String, _>("title").unwrap(), "tidy up");
    assert_eq!(row.try_get::<i64, _>("at").unwrap(), 1_700_000_000);
    assert_eq!(row.try_get::<f64, _>("score").unwrap(), 0.5);
    database.close().await.unwrap();
  };

  read.await;
  admin.execute(format!("DROP SCHEMA \"{schema}\" CASCADE").as_str()).await.unwrap();
}
