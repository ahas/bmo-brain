//! The fragment tool's work on a desk, read through an opened desk over a SQLite fragment table and a
//! brain folder on disk, as a run's calls reach it.

use std::fs;
use std::path::Path;

use fragment_desk::desk::{Desk, DeskError};
use fragment_desk::meta::body_hash;
use fragment_desk::ops::{Open, Query, WriteRequest, normalized};
use fragment_desk::rows::AgentFolder;
use sqlx::{Connection, SqliteConnection};

const ROOM: i64 = -100;
const STANDING: &str = "A note on the order of a deploy, read when a deploy has to be run again.";

const GREETING: &str = "- [date 2026-09-01 · confirmed 2026-09-02] hello\n";

// A brain with one knowledge chunk of agent-b's in the table, standing as the run found it, a new chunk
// on disk with no row yet, and a desk for a run of agent-b's in the room.
async fn opened(root: &Path) -> std::path::PathBuf {
  let brain = root.join("brain");
  let knowledge = brain.join("agents/agent-b/knowledge");
  let db = root.join("rows.db");
  let desk = root.join("desks/run.json");

  fs::create_dir_all(&knowledge).unwrap();
  fs::write(knowledge.join("greeting.md"), GREETING).unwrap();
  fs::write(
    knowledge.join("deploy.md"),
    "- [date 2026-10-03 · confirmed 2026-10-05] #758 the order\n",
  )
  .unwrap();

  let mut connection =
    SqliteConnection::connect(&format!("sqlite://{}?mode=rwc", db.display())).await.unwrap();

  sqlx::query(
    "CREATE TABLE fragments (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, chat_id INTEGER, owner INTEGER,
       slug TEXT NOT NULL, title TEXT NOT NULL, path TEXT NOT NULL, scope TEXT, position INTEGER NOT NULL,
       active INTEGER NOT NULL, keywords TEXT NOT NULL, items TEXT NOT NULL, written_on TEXT,
       confirmed_on TEXT, status TEXT NOT NULL, agent TEXT NOT NULL, all_agents INTEGER NOT NULL)",
  )
  .execute(&mut connection)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO fragments VALUES (1, 'library-knowledge', -100, NULL, 'greeting',
       'How the room is greeted, read when somebody new arrives.', 'agents/agent-b/knowledge/greeting.md',
       '[chat:-100]', 1, 1, '[\"greeting\",\"welcome\"]', '[12]', '2026-09-01', '2026-09-02', 'open',
       'agent-b', 0)",
  )
  .execute(&mut connection)
  .await
  .unwrap();
  connection.close().await.unwrap();

  fs::create_dir_all(desk.parent().unwrap()).unwrap();
  fs::write(
    &desk,
    serde_json::json!({
      "room": ROOM,
      "agent": "agent-b",
      "folder": "agents/agent-b",
      "places": ["knowledge"],
      "reach": [{ "kind": "library-knowledge" }],
      "beside": null,
      "carried": [],
      "db": db,
      "brain": brain.join("agents/agent-b"),
      "before": { "knowledge/greeting.md": body_hash(GREETING) },
      "writes": [],
      "names": { "agent": "Agent A", "agentKey": "agent-a", "commonWords": ["agent-a", "rule"] },
      "openedBy": "the batch",
    })
    .to_string(),
  )
  .unwrap();

  desk
}

fn folders() -> Vec<AgentFolder> {
  vec![AgentFolder {
    agent: "agent-b".to_owned(),
    folder: "agents/agent-b/".to_owned(),
  }]
}

fn request(path: &str, title: &str, keywords: &[&str]) -> WriteRequest {
  WriteRequest {
    path: path.to_owned(),
    title: title.to_owned(),
    keywords: Some(keywords.iter().map(|word| (*word).to_owned()).collect()),
    items: Some(Vec::new()),
    shared: None,
    all_agents: None,
    distinct_from: Vec::new(),
  }
}

fn refusal<T: std::fmt::Debug>(answer: Result<T, DeskError>) -> String {
  match answer.unwrap_err() {
    DeskError::Refused(refusal) => refusal.message,
    DeskError::Failed(why) => panic!("failed rather than refused: {why}"),
  }
}

#[test]
fn a_path_climbs_out_only_where_its_normal_form_does() {
  assert!(normalized("../x.md").starts_with(".."));
  assert!(normalized("a/../../x.md").starts_with(".."));
  assert!(!normalized("a/../x.md").starts_with(".."));
  assert_eq!(normalized("./knowledge/a.md"), "knowledge/a.md");
}

#[tokio::test]
async fn a_run_finds_its_rows_writes_a_new_piece_and_owes_nothing_after() {
  let root = tempfile::tempdir().unwrap();
  let path = opened(root.path()).await;
  let mut open = Open::at(&path, folders()).await.unwrap();

  let found = open.search(&Query::default()).unwrap();

  assert_eq!(found.len(), 1);
  assert!(found[0].starts_with("- greeting [chat:-100] — How the room is greeted"), "{}", found[0]);
  assert!(found[0].contains("(path knowledge/greeting.md · keywords greeting, welcome · items #12)"));
  assert_eq!(open.owed(), ["knowledge/deploy.md"]);

  let written = open.write(&request("./knowledge/deploy.md", STANDING, &["deploy-order"])).unwrap();

  assert_eq!(
    written,
    format!(
      "- deploy [chat:-100] — {STANDING} · 2026-10 (path knowledge/deploy.md · keywords deploy-order · items #758) [written in this run]"
    )
  );
  assert!(open.owed().is_empty());

  open.desk.save(&path).unwrap();

  let saved = Desk::read(&path).unwrap();

  assert_eq!(saved.writes.len(), 1);
  assert_eq!(saved.writes[0].path, "knowledge/deploy.md");
  assert_eq!(saved.rest.get("openedBy").and_then(|by| by.as_str()), Some("the batch"));
}

#[tokio::test]
async fn a_write_that_cannot_stand_is_refused_and_leaves_the_desk_as_it_was() {
  let root = tempfile::tempdir().unwrap();
  let path = opened(root.path()).await;
  let mut open = Open::at(&path, folders()).await.unwrap();

  assert!(refusal(open.write(&request("knowledge/deploy.md", "Too short.", &[]))).contains("too short"));
  assert!(
    refusal(open.write(&request("rooms/-100/rules/x.md", STANDING, &[])))
      .starts_with("rooms/-100/rules/x.md is outside where this run may write: knowledge.")
  );
  assert!(
    refusal(open.write(&request("knowledge/missing.md", STANDING, &[])))
      .ends_with("is not on disk. Write the piece's body with your file tools first, then its row.")
  );
  assert!(
    refusal(open.write(&request("knowledge/deploy.md", STANDING, &["Rule"])))
      .starts_with("knowledge/deploy.md carries Rule as keywords")
  );

  let repeats = refusal(open.write(&request("knowledge/deploy.md", STANDING, &["greeting", "welcome"])));

  assert!(repeats.starts_with("knowledge/deploy.md would be a new piece"), "{repeats}");
  assert!(repeats.contains("- greeting [chat:-100]"), "{repeats}");
  assert!(open.desk.writes.is_empty());
}
