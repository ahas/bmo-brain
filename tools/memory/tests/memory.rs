//! The memory tools as the tool host calls them (docs/boundary-api.md, 「Its tools」): the manifest held to the
//! executable, each entry answered through its brain route with the turn the record names, a refusal and a
//! failure said as the tools say them, and the executable answering one request on its standard streams.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::Uri;
use serde_json::{Value, json};
use tool_call::{Brain, Session, differences};

// A brain stood in for on a local port: each path answered as the test set it, every request kept.
#[derive(Clone, Default)]
struct Stub {
  answers: Arc<Mutex<BTreeMap<String, Value>>>,
  taken: Arc<Mutex<Vec<(String, Value)>>>,
}

async fn stood(stub: Stub) -> String {
  async fn answer(State(stub): State<Stub>, uri: Uri, body: Bytes) -> axum::Json<Value> {
    let path = uri.path().to_owned();

    stub
      .taken
      .lock()
      .unwrap()
      .push((path.clone(), serde_json::from_slice(&body).unwrap_or(Value::Null)));
    axum::Json(
      stub
        .answers
        .lock()
        .unwrap()
        .get(&path)
        .cloned()
        .unwrap_or(json!({ "answered": true, "lines": [] })),
    )
  }

  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move {
    axum::serve(listener, Router::new().fallback(answer).with_state(stub)).await.unwrap()
  });
  base
}

fn turn_of(agent: Option<&str>, chat: Option<i64>) -> Session {
  let mut variables = BTreeMap::new();

  if let Some(agent) = agent {
    variables.insert("BMO_TURN_AGENT".to_owned(), agent.to_owned());
  }

  if let Some(chat) = chat {
    variables.insert("BMO_TURN_CHAT".to_owned(), chat.to_string());
  }

  Session {
    run_variables: variables,
    ..Session::default()
  }
}

#[test]
fn the_manifest_declares_each_entry_as_the_executable_answers_it() {
  assert_eq!(
    differences(Path::new(env!("CARGO_MANIFEST_DIR")), memory_tool::ENTRIES),
    Vec::<String>::new()
  );
}

#[tokio::test]
async fn each_entry_is_asked_of_its_route_under_the_turn_the_record_names() {
  let stub = Stub::default();
  let brain = Brain::at(&stood(stub.clone()).await, "t");
  let session = turn_of(Some("agent-a"), Some(-100));

  for (entry, arguments, route) in [
    ("memory_list", json!({ "type": "promise", "all": true }), "/memory/list"),
    (
      "memory_add",
      json!({ "type": "situation", "action": "say it", "condition": "when", "for": 7, "message": 9 }),
      "/memory/add",
    ),
    (
      "memory_update",
      json!({ "memory": "m", "action": "now", "message": 3 }),
      "/memory/update",
    ),
    ("memory_done", json!({ "memory": "m", "text": "done" }), "/memory/done"),
    (
      "memory_retire",
      json!({ "memory": "m", "why": "over", "global": true }),
      "/memory/retire",
    ),
    ("memory_revive", json!({ "memory": "m" }), "/memory/revive"),
    ("memory_candidate", json!({ "memory": "m", "why": "stale" }), "/memory/candidate"),
    ("note_add", json!({ "text": "keep this", "author": 7, "global": true }), "/notes/add"),
    (
      "watch_set",
      json!({ "until": "true", "note": "tell them", "every": 30, "item": 4 }),
      "/watches/set",
    ),
  ] {
    let answered = memory_tool::call_on(&brain, &session, entry, arguments).await;

    assert!(!answered.is_error, "{entry}: {}", answered.plain());
    assert_eq!(stub.taken.lock().unwrap().last().unwrap().0, route);
  }

  let taken = stub.taken.lock().unwrap().clone();

  assert_eq!(
    taken[0].1,
    json!({ "type": "promise", "all": true, "turn": { "agent": "agent-a", "chatId": -100, "messageId": null } })
  );
  assert_eq!(taken[1].1["for"], json!(7));
  assert_eq!(taken[1].1["turn"]["messageId"], json!(9));
  assert_eq!(taken[2].1["turn"]["messageId"], json!(3));
  assert_eq!(taken[2].1["message"], json!(3));
  assert_eq!(taken[4].1["global"], json!(true));
  assert_eq!(
    taken[7].1,
    json!({ "text": "keep this", "author": 7, "global": true, "turn": { "agent": "agent-a", "chatId": -100, "messageId": null } })
  );
  assert_eq!(taken[8].1["item"], json!(4));
}

#[tokio::test]
async fn a_reading_a_refusal_and_a_failure_are_said_as_the_tools_say_them() {
  let stub = Stub::default();
  let brain = Brain::at(&stood(stub.clone()).await, "t");
  let session = turn_of(Some("agent-a"), Some(-100));

  stub
    .answers
    .lock()
    .unwrap()
    .insert("/memory/list".into(), json!({ "answered": true, "lines": ["[on ] #1 promise"] }));

  let listed = memory_tool::call_on(&brain, &session, "memory_list", json!({})).await;

  assert_eq!(listed.structured, Some(json!({ "lines": ["[on ] #1 promise"] })));
  assert_eq!(listed.plain(), json!({ "lines": ["[on ] #1 promise"] }).to_string());

  stub.answers.lock().unwrap().insert(
    "/memory/add".into(),
    json!({ "answered": false, "refusal": { "code": "memory-refused", "message": "taken down again" }, "lines": ["memory 4 written"] }),
  );

  let refused =
    memory_tool::call_on(&brain, &session, "memory_add", json!({ "type": "promise", "action": "x" })).await;

  assert!(refused.is_error);
  assert_eq!(refused.plain(), "memory 4 written\ntaken down again");

  stub.answers.lock().unwrap().insert(
    "/watches/set".into(),
    json!({ "answered": false, "refusal": { "code": "memory-refused", "message": "already watching this as #3" } }),
  );
  assert_eq!(
    memory_tool::call_on(&brain, &session, "watch_set", json!({ "until": "true", "note": "n" }))
      .await
      .plain(),
    "watch: already watching this as #3"
  );

  let malformed =
    memory_tool::call_on(&brain, &session, "memory_add", json!({ "type": "todo", "action": "x" })).await;

  assert!(
    malformed.is_error && malformed.plain().starts_with("ERROR: invalid arguments: memory_add"),
    "{}",
    malformed.plain()
  );

  let nowhere =
    memory_tool::call_on(&Brain::at("http://127.0.0.1:1", "t"), &session, "memory_list", json!({})).await;

  assert!(
    nowhere.is_error && nowhere.plain().contains("could not be reached"),
    "{}",
    nowhere.plain()
  );
  assert!(
    memory_tool::call(&Session::default(), "memory_list", json!({}))
      .await
      .plain()
      .starts_with("ERROR: no BRAIN_ADDRESS")
  );
}

#[tokio::test]
async fn the_executable_answers_one_request_on_its_standard_streams() {
  let stub = Stub::default();
  let base = stood(stub.clone()).await;
  let data = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(data.path().join("env/brain.env"), format!("BRAIN_ADDRESS={base}\nBRAIN_TOKEN=t\n"))
    .unwrap();
  stub
    .answers
    .lock()
    .unwrap()
    .insert("/notes/add".into(), json!({ "answered": true, "lines": ["note 1 kept for chat -100"] }));

  let request = json!({
    "protocol": 1,
    "kind": "call",
    "entry": "bmo/note_add",
    "arguments": { "text": "keep this" },
    "session": { "line": "chat", "capabilities": ["room.tools"], "dataRoot": data.path(), "archive": "",
                 "runVariables": { "BMO_TURN_AGENT": "agent-a", "BMO_TURN_CHAT": "-100" } },
  });
  let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_memory-tool"))
    .env_remove("BRAIN_ADDRESS")
    .env_remove("BRAIN_TOKEN")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .unwrap();

  {
    use tokio::io::AsyncWriteExt;

    let mut input = child.stdin.take().unwrap();

    input.write_all(request.to_string().as_bytes()).await.unwrap();
  }

  let done = child.wait_with_output().await.unwrap();

  assert!(done.status.success());
  assert_eq!(
    serde_json::from_slice::<Value>(&done.stdout).unwrap(),
    json!({ "content": [{ "type": "text", "text": "note 1 kept for chat -100" }], "isError": false })
  );
  assert_eq!(stub.taken.lock().unwrap()[0].1["turn"]["agent"], json!("agent-a"));

  let refused = tokio::process::Command::new(env!("CARGO_BIN_EXE_memory-tool"))
    .stdin(std::process::Stdio::null())
    .output()
    .await
    .unwrap();

  assert!(!refused.status.success());
  assert!(String::from_utf8_lossy(&refused.stderr).contains("does not read"));
}
