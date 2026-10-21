//! The old archive's move into the brain's store (src/import.rs, TODO #1923): the mapping, the brain
//! tree's files, the refusal of a store already holding rows, and the count-and-sample compare, over a
//! small export and tree written into a temporary folder and a migrated scratch database.

mod support;

use std::fs;
use std::path::PathBuf;

use bmo_brain::import;
use serde_json::{Value, json};
use support::Served;

/// A small export and a small brain tree in the shapes the item's export hand and the brain write.
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
  let dir = tempfile::tempdir().expect("a temporary folder");
  let tree = dir.path().join("brain");
  for (path, text) in [
    ("rooms/1/people/chunks/p1-foo.md", "# ahas — Foo\n\n- a fact\n"),
    ("rooms/1/people/1.md", "# ahas\n\n- a sheet\n"),
    ("rooms/1/rules/rule.md", "# the rule\n\n- a rule\n"),
    ("knowledge/thing.md", "# a thing\n\n- knowledge\n"),
    ("knowledge/index.md", "# index\n"),
    ("rooms/1/people/chunks/index.md", "# index\n"),
  ] {
    let path = tree.join(path);
    fs::create_dir_all(path.parent().expect("a parent")).expect("the folder");
    fs::write(&path, text).expect("the file");
  }

  let fragments = vec![
    json!({"id": 1, "kind": "person-book", "chat_id": 1, "owner": 1, "slug": "p1-foo", "title": "the old title", "path": "rooms/1/people/chunks/p1-foo.md", "scope": "chat:1", "position": 0, "active": 1, "last_hit_at": 10, "hits": 5, "set_by": null, "set_at": 20, "hand": null, "hand_at": null, "keywords": "[]", "items": "[]", "written_on": null, "confirmed_on": null, "status": "open", "body_hash": "x", "agent": "bmo", "all_agents": 0}),
    json!({"id": 2, "kind": "person-sheet", "chat_id": 1, "owner": 1, "slug": "1", "title": null, "path": "rooms/1/people/1.md", "scope": "chat:1", "position": 0, "active": 1, "last_hit_at": null, "hits": 0, "set_by": null, "set_at": 30, "hand": null, "hand_at": null, "keywords": "[]", "items": "[]", "written_on": null, "confirmed_on": null, "status": "open", "body_hash": "y", "agent": "bmo", "all_agents": 0}),
    json!({"id": 3, "kind": "room-rule", "chat_id": 1, "owner": null, "slug": "rule", "title": "the old rule", "path": "rooms/1/rules/rule.md", "scope": "chat:1", "position": 0, "active": 1, "last_hit_at": null, "hits": 0, "set_by": null, "set_at": 40, "hand": null, "hand_at": null, "keywords": "[]", "items": "[]", "written_on": null, "confirmed_on": null, "status": "open", "body_hash": "z", "agent": "bmo", "all_agents": 0}),
    json!({"id": 4, "kind": "library-knowledge", "chat_id": null, "owner": null, "slug": "thing", "title": "the knowledge", "path": "knowledge/thing.md", "scope": "[shared]", "position": 0, "active": 1, "last_hit_at": null, "hits": 2, "set_by": null, "set_at": 50, "hand": null, "hand_at": null, "keywords": "[]", "items": "[]", "written_on": null, "confirmed_on": null, "status": "open", "body_hash": "w", "agent": null, "all_agents": 0}),
    json!({"id": 5, "kind": "person-book", "chat_id": 1, "owner": 1, "slug": "gone", "title": "gone", "path": "rooms/1/people/chunks/p1-gone.md", "scope": "chat:1", "position": 0, "active": 0, "last_hit_at": null, "hits": 0, "set_by": null, "set_at": 60, "hand": null, "hand_at": null, "keywords": "[]", "items": "[]", "written_on": null, "confirmed_on": null, "status": "open", "body_hash": "v", "agent": "bmo", "all_agents": 0}),
  ];
  let memories = vec![
    json!({"id": 1, "slug": "m", "type": "followup", "chat_id": 1, "reach": "owner", "condition": "", "action": "do it", "for_user": null, "message_id": null, "todo_id": null, "set_by": null, "set_at": 100, "active": 1, "last_hit_at": 110, "hits": 3, "done_at": null, "done_text": null, "down_reason": null, "down_at": null, "candidate_why": null, "candidate_at": null, "event": "", "event_met_at": null, "moved_at": null, "agent": "bmo"}),
  ];
  let notes = vec![
    json!({"id": 1, "chat_id": 1, "author": "하스", "text": "a note", "created_at": 200, "agent": "bmo", "shared": 0, "consumed_at": null}),
  ];
  let reminders = vec![
    json!({"id": 1, "chat_id": 1, "user_id": null, "due_at": 300, "reason": "a reminder", "memory_id": null, "every": null, "fired": 1, "created_at": 290, "agent": "bmo"}),
  ];
  let watches = vec![
    json!({"id": 1, "chat_id": 1, "by_chat": 1, "kind": "timer", "condition": null, "name": "a timer", "note": "x", "dedupe_key": "t", "interval_seconds": 60, "due_at": 400, "deadline_at": null, "state": "fired", "unit": "u", "run_id": null, "armed_at": 390, "last_checked_at": 400, "last_exit": 0, "instruction_id": null, "created_at": 390, "ended_at": 400, "memory_id": null, "agent": "bmo"}),
  ];
  let session_people =
    vec![json!({"session_id": "s", "path": "rooms/1/people/1.md", "version": "v", "loaded_at": 500})];

  let tables: Vec<(&str, Vec<Value>, Vec<&str>)> = vec![
    ("fragments", fragments, vec!["id"]),
    ("memories", memories, vec!["id"]),
    ("notes", notes, vec!["id"]),
    ("reminders", reminders, vec!["id"]),
    ("watches", watches, vec!["id"]),
    ("session_people", session_people, vec!["session_id"]),
  ];

  let mut manifest = Vec::new();
  for (source, rows, key) in &tables {
    let file = format!("{source}.ndjson");
    let mut body = String::new();
    for row in rows {
      body.push_str(&row.to_string());
      body.push('\n');
    }
    fs::write(dir.path().join(&file), body).expect("the export file");
    manifest.push(json!({
      "source": source,
      "file": file,
      "count": rows.len(),
      "columns": [],
      "key": key
    }));
  }
  fs::write(
    dir.path().join("manifest.json"),
    json!({
      "unit": "brain",
      "snapshot": "1:1:",
      "readAt": "2026-10-21 17:00:00+09",
      "readOnly": true,
      "tables": manifest
    })
    .to_string(),
  )
  .expect("the manifest");

  let export = dir.path().to_path_buf();
  (dir, export, tree)
}

#[test]
fn a_file_is_classified_by_its_place_and_its_agent() {
  let piece = import::classify("rooms/1/people/chunks/p2-foo.md", "# a heading\n");
  assert_eq!(piece.kind, import::Kind::Piece);
  assert_eq!(piece.chat, Some(1));
  assert_eq!(piece.subject.as_deref(), Some("2"));
  assert_eq!(import::key(&piece), "bmo:rooms:1:people:chunks:p2-foo");

  let sheet = import::classify("rooms/1/people/2.md", "# a sheet\n");
  assert_eq!(sheet.kind, import::Kind::Sheet);
  assert_eq!(sheet.subject.as_deref(), Some("2"));

  let companion = import::classify("agents/kakia-chat/people/2/sheet.md", "# a sheet\n");
  assert_eq!(companion.agent, "kakia-chat");
  assert_eq!(import::key(&companion), "kakia-chat:people:2:sheet");
}

#[test]
fn a_memory_state_reads_from_how_it_stands() {
  assert_eq!(
    import::memory_state(&json!({"active": 1, "done_at": null, "candidate_why": null})),
    "standing"
  );
  assert_eq!(import::memory_state(&json!({"active": 1, "done_at": 5, "candidate_why": null})), "done");
  assert_eq!(
    import::memory_state(&json!({"active": 0, "done_at": null, "candidate_why": null})),
    "retired"
  );
  assert_eq!(
    import::memory_state(&json!({"active": 1, "done_at": null, "candidate_why": "why"})),
    "candidate"
  );
}

#[tokio::test]
async fn the_import_writes_its_rows_refuses_a_second_run_and_the_compare_finds_a_changed_row() {
  let (_folder, export, tree) = fixture();
  let served = Served::new().await;
  let address = served.address();

  let report = import::run(&address, &export, &tree, false).await.expect("the import");
  assert_eq!(report.snapshot, "1:1:");
  assert!(report.sample.failures.is_empty(), "{:?}", report.sample.failures);
  let fragments = report.tables.iter().find(|moved| moved.source == "fragments").expect("fragments");
  assert_eq!((fragments.exported, fragments.written), (5, 4));

  let page = served
    .client
    .fragments(&bmo_brain_client::FragmentsQuery {
      kind: Some("person".to_owned()),
      ..Default::default()
    })
    .await
    .expect("the fragments");
  assert_eq!(page.items.len(), 2);

  let again = import::run(&address, &export, &tree, false).await;
  assert!(again.is_err(), "a store holding rows is refused");
  assert!(again.unwrap_err().contains("already holds rows"));

  let passes = import::verify(&address, &export, &tree).await.expect("the compare");
  assert!(passes.failures.is_empty(), "{:?}", passes.failures);

  let pool = sqlx::PgPool::connect(&address).await.expect("the store");
  sqlx::query("UPDATE fragments SET title = 'changed' WHERE kind = 'library-knowledge'")
    .execute(&pool)
    .await
    .expect("the change");
  pool.close().await;
  let changed = import::verify(&address, &export, &tree).await.expect("the compare");
  assert!(
    changed.failures.iter().any(|failure| failure.contains("title")),
    "{:?}",
    changed.failures
  );
}

#[tokio::test]
async fn a_dry_run_writes_nothing() {
  let (_folder, export, tree) = fixture();
  let served = Served::new().await;
  let address = served.address();

  let report = import::run(&address, &export, &tree, true).await.expect("the dry run");
  assert!(report.dry_run);

  let pool = sqlx::PgPool::connect(&address).await.expect("the store");
  let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fragments")
    .fetch_one(&pool)
    .await
    .expect("the count");
  pool.close().await;
  assert_eq!(count, 0);
}
