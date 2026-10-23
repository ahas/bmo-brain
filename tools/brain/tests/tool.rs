//! The brain's tool: its manifest held to the entries the executable answers, each entry's schema a JSON
//! Schema beside it, and the executable answering a call against a served brain (docs/boundary-api.md,
//! 「Its tool」).

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::str::FromStr;

use bmo_brain::api::{Brain, router};
use bmo_brain::store::{MIGRATOR, Store};
use bmo_brain_client::{BrainClient, MemoriesQuery, NewBrain};
use brain_tool::{ACTOR, ENTRIES};
use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Connection, Executor, PgConnection};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const TOKEN: &str = "the-brain-token";

#[test]
fn the_manifest_declares_exactly_the_entries_the_executable_answers_each_with_a_schema_beside_it() {
  let manifest: toml::Value =
    toml::from_str(&std::fs::read_to_string(format!("{ROOT}/tool.toml")).unwrap()).unwrap();
  assert_eq!(manifest["tool"]["name"].as_str(), Some("brain"));
  assert_eq!(manifest["tool"]["protocol"].as_integer(), Some(1));
  assert_eq!(manifest["tool"]["run"].as_array().unwrap()[0].as_str(), Some("brain-tool"));
  let entries = manifest["entry"].as_array().unwrap();
  let declared: Vec<&str> = entries.iter().map(|entry| entry["name"].as_str().unwrap()).collect();
  assert_eq!(declared, ENTRIES);
  for entry in entries {
    assert_eq!(entry["namespace"].as_str(), Some("bmo"));
    assert!(
      entry["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c.as_str() == Some("room.chat"))
    );
    let schema = entry["input"].as_str().unwrap();
    let text = std::fs::read_to_string(format!("{ROOT}/{schema}")).unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(schema["type"].as_str(), Some("object"), "{schema}");
    assert_eq!(schema["additionalProperties"].as_bool(), Some(false), "{schema}");
  }
}

fn test_address() -> String {
  std::env::var("BMO_TEST_POSTGRES")
    .unwrap_or_else(|_| "postgres:///bmo_test?host=/var/run/postgresql".into())
}

fn options(database: &str) -> PgConnectOptions {
  PgConnectOptions::from_str(&test_address())
    .unwrap()
    .database(database)
    .disable_statement_logging()
}

/// A migrated database of this test's own, served on a port of its own; the database is dropped with the guard.
struct Served {
  database: String,
  base: String,
  client: BrainClient,
  serving: tokio::task::JoinHandle<()>,
}

impl Served {
  async fn new() -> Self {
    let database = format!("brain_tool_{}", std::process::id());
    let mut admin = PgConnection::connect_with(&options("postgres")).await.unwrap();
    let _ = admin
      .execute(format!("DROP DATABASE IF EXISTS \"{database}\" WITH (FORCE)").as_str())
      .await;
    admin.execute(format!("CREATE DATABASE \"{database}\"").as_str()).await.unwrap();
    admin.close().await.unwrap();
    let pool = PgPoolOptions::new().max_connections(4).connect_with(options(&database)).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::over(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let brain = Brain {
      store,
      token: TOKEN.to_owned(),
    };
    let serving = tokio::spawn(async move {
      axum::serve(listener, router(brain)).await.unwrap();
    });
    let client = BrainClient::new(&base, TOKEN).unwrap();
    Self {
      database,
      base,
      client,
      serving,
    }
  }
}

impl Drop for Served {
  fn drop(&mut self) {
    self.serving.abort();
    let database = std::mem::take(&mut self.database);
    let _ = std::thread::spawn(move || {
      let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
      runtime.block_on(async move {
        let mut admin = PgConnection::connect_with(&options("postgres")).await.unwrap();
        let _ = admin
          .execute(format!("DROP DATABASE IF EXISTS \"{database}\" WITH (FORCE)").as_str())
          .await;
        let _ = admin.close().await;
      });
    })
    .join();
  }
}

/// The executable handed one call, as the host hands it, with the brain's address in its environment.
fn called(served: &Served, entry: &str, arguments: Value, agent: &str, chat: &str) -> Value {
  let handed = json!({
    "protocol": 1,
    "kind": "call",
    "entry": format!("bmo/{entry}"),
    "arguments": arguments,
    "session": {
      "holderName": agent,
      "namespaces": ["bmo"],
      "capabilities": ["room.chat"],
      "variables": { "BMO_TURN_AGENT": agent, "BMO_TURN_CHAT": chat, "BMO_RUN_NAME": "run-1" },
    },
  });
  let mut child = Command::new(env!("CARGO_BIN_EXE_brain-tool"))
    .env_clear()
    .env("BRAIN_ADDRESS", &served.base)
    .env("BRAIN_TOKEN", TOKEN)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  child.stdin.take().unwrap().write_all(handed.to_string().as_bytes()).unwrap();
  let out = child.wait_with_output().unwrap();
  assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
  serde_json::from_slice(&out.stdout).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_executable_writes_an_episode_and_a_learning_into_the_agents_brain() {
  let served = Served::new().await;
  let brain = served
    .client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
    })
    .await
    .unwrap();

  let episode = called(
    &served,
    "memory_episode",
    json!({ "text": "I moved to the south.", "speaker": "1", "importance": 7 }),
    "agent-a",
    "7",
  );
  assert_eq!(episode["isError"], json!(false), "{episode}");
  let id = episode["structuredContent"]["memoryId"].as_i64().unwrap();
  let written = served.client.memory(brain.id, id).await.unwrap();
  assert_eq!(
    (written.kind.as_str(), written.importance, written.actor_name.as_str()),
    ("episode", 7, ACTOR)
  );
  let keys: Vec<(String, String)> =
    written.subjects.iter().map(|s| (s.kind.clone(), s.key.clone())).collect();
  assert!(
    keys.contains(&("person".to_owned(), "1".to_owned()))
      && keys.contains(&("room".to_owned(), "7".to_owned())),
    "{keys:?}"
  );

  let learned = called(
    &served,
    "memory_learn",
    json!({ "type": "fact", "text": "The member lives in the south.", "source": "own", "subjects": [{"type": "person", "key": "1"}], "grounds": [id] }),
    "agent-a",
    "7",
  );
  assert_eq!(learned["isError"], json!(false), "{learned}");
  assert_eq!(learned["structuredContent"]["written"]["type"], json!("fact"));

  let likes = called(
    &served,
    "memory_similar",
    json!({ "text": "where does the member live", "subjects": [{"type": "person", "key": "1"}] }),
    "agent-a",
    "7",
  );
  assert_eq!(likes["structuredContent"]["memories"].as_array().unwrap().len(), 1, "{likes}");

  let listed = served
    .client
    .memories(
      brain.id,
      &MemoriesQuery {
        kind: Some("fact".to_owned()),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(listed.items.len(), 1);

  let unknown =
    called(&served, "memory_episode", json!({ "text": "x", "speaker": "1" }), "agent-none", "7");
  assert_eq!(unknown["isError"], json!(true), "{unknown}");
  assert!(unknown["content"][0]["text"].as_str().unwrap().contains("no brain"));
}
