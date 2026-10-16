//! The brain's migrations over a database of the test's own: the six tables and the settings table in the
//! archive's shape with its indexes, an `agent` column that names no default, and a second run that changes
//! nothing.

mod support;

use bmo_brain::migration::migrate;
use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};

use support::scratch::Scratch;
use support::{AGENT, ROOM};

// Each table's columns in their order, as the archive holds them.
const TABLES: [(&str, &str); 7] = [
  (
    "fragments",
    "id kind chat_id owner slug title path scope position active last_hit_at hits set_by set_at hand \
     hand_at keywords items written_on confirmed_on status body_hash agent all_agents rowid",
  ),
  (
    "memories",
    "id slug type chat_id reach condition action for_user message_id todo_id set_by set_at active \
     last_hit_at hits done_at done_text down_reason down_at candidate_why candidate_at event \
     event_met_at moved_at agent rowid",
  ),
  ("notes", "id chat_id author text created_at agent shared consumed_at rowid"),
  (
    "reminders",
    "id chat_id user_id due_at reason memory_id every fired created_at agent rowid",
  ),
  ("session_people", "session_id path version loaded_at rowid"),
  ("settings", "key value rowid"),
  (
    "watches",
    "id chat_id by_chat kind condition name note dedupe_key interval_seconds due_at deadline_at state \
     unit run_id armed_at last_checked_at last_exit instruction_id created_at ended_at memory_id agent \
     rowid",
  ),
];

const INDEXES: [&str; 8] = [
  "fragments_one",
  "fragments_scope",
  "memories_room",
  "memories_slug",
  "memories_standing",
  "memories_todo",
  "watches_active_dedupe",
  "watches_state",
];

// A row of each table with an agent, its agent's column aside: the table, the other columns it needs and
// their values, the room bound as `$1`.
const ROWS: [(&str, &str, &str); 5] = [
  (
    "fragments",
    r#""kind", "chat_id", "slug", "title", "path", "position", "active", "hits", "set_at""#,
    "'knowledge', $1, 'a-piece', 'A piece', 'knowledge/a-piece.md', 0, 1, 0, 1",
  ),
  (
    "memories",
    r#""slug", "type", "chat_id", "reach", "condition", "action", "set_at", "active", "hits""#,
    "'a-promise', 'promise', $1, 'room', '', 'tell the room', 1, 1, 0",
  ),
  ("notes", r#""chat_id", "text", "created_at""#, "$1, 'a note', 1"),
  ("reminders", r#""chat_id", "due_at", "created_at""#, "$1, 2, 1"),
  (
    "watches",
    r#""chat_id", "by_chat", "kind", "note", "dedupe_key", "deadline_at", "created_at""#,
    "$1, $1, 'bash', 'a check', 'a-check', 2, 1",
  ),
];

async fn strings(scratch: &Scratch, sql: &str, values: Vec<Value>) -> Vec<String> {
  scratch
    .db()
    .query_all(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
    .await
    .unwrap()
    .iter()
    .map(|row| row.try_get_by_index::<String>(0).unwrap())
    .collect()
}

#[tokio::test]
async fn the_migrations_make_the_brain_s_tables_in_the_archive_s_shape() {
  let scratch = Scratch::new().await;
  let tables = strings(
    &scratch,
    "SELECT table_name::text FROM information_schema.tables
     WHERE table_schema = 'public' AND table_name <> 'seaql_migrations' ORDER BY table_name",
    vec![],
  )
  .await;

  assert_eq!(tables, TABLES.map(|(table, _)| table.to_owned()));

  for (table, columns) in TABLES {
    let made = strings(
      &scratch,
      "SELECT column_name::text FROM information_schema.columns
       WHERE table_schema = 'public' AND table_name = $1 ORDER BY ordinal_position",
      vec![table.into()],
    )
    .await;

    assert_eq!(made, columns.split_whitespace().collect::<Vec<_>>(), "{table}");
  }

  let indexes = strings(
    &scratch,
    "SELECT indexname::text FROM pg_indexes
     WHERE schemaname = 'public' AND indexname NOT LIKE '%\\_pkey' AND indexname NOT LIKE '%\\_rowid\\_key'
       AND tablename <> 'seaql_migrations'
     ORDER BY indexname",
    vec![],
  )
  .await;

  assert_eq!(indexes, INDEXES);
}

#[tokio::test]
async fn the_agent_column_names_no_default() {
  let scratch = Scratch::new().await;
  let defaulted = scratch
    .count(
      "SELECT count(*) FROM information_schema.columns
       WHERE table_schema = 'public' AND column_name = 'agent'
         AND column_default IS NULL AND is_nullable = 'NO'",
      vec![],
    )
    .await;

  assert_eq!(defaulted, 5);
}

#[tokio::test]
async fn an_insert_without_an_agent_is_refused() {
  let scratch = Scratch::new().await;

  for (table, columns, values) in ROWS {
    let refused = scratch
      .try_run(&format!("INSERT INTO \"{table}\" ({columns}) VALUES ({values})"), vec![ROOM.into()])
      .await
      .unwrap_err()
      .to_string();

    assert!(refused.contains("null value in column \"agent\""), "{table}: {refused}");

    scratch
      .run(
        &format!("INSERT INTO \"{table}\" ({columns}, \"agent\") VALUES ({values}, $2)"),
        vec![ROOM.into(), AGENT.into()],
      )
      .await;
    assert_eq!(
      scratch
        .count(&format!("SELECT count(*) FROM \"{table}\" WHERE \"agent\" = $1"), vec![AGENT.into()])
        .await,
      1,
      "{table}"
    );
  }
}

#[tokio::test]
async fn the_settings_table_holds_one_value_a_key() {
  let scratch = Scratch::new().await;
  let paused = r#"INSERT INTO "settings" ("key", "value") VALUES ('digest_paused_until', $1)"#;

  scratch.run(paused, vec!["100".into()]).await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('digest_pause_noticed', '1')"#,
      vec![],
    )
    .await;

  assert!(scratch.try_run(paused, vec!["200".into()]).await.is_err());
  assert_eq!(scratch.count(r#"SELECT count(*) FROM "settings""#, vec![]).await, 2);
}

#[tokio::test]
async fn migrating_again_changes_nothing() {
  let scratch = Scratch::new().await;

  migrate(&scratch.address()).await.unwrap();

  assert_eq!(scratch.count("SELECT count(*) FROM seaql_migrations", vec![]).await, 1);
}
