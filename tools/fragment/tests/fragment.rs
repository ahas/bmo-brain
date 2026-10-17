//! The fragment tool as the tool host calls it (docs/boundary-api.md, 「Its tools」): the manifest held to the
//! executable, each entry answered through its desk route with the desk the record names, a run with no desk and a
//! refusal said as the tool says them, and the executable answering one request on its standard streams.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::Uri;
use serde_json::{Value, json};
use tool_call::{Brain, Session, differences};

const DESK: &str = "/data/fragment-desks/0123456789abcdef0123456789abcdef.json";
const TITLE: &str = "A note on the order of a deploy, read when a deploy has to be run again.";

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
        .unwrap_or(json!({ "answered": true, "lines": ["done"] })),
    )
  }

  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move {
    axum::serve(listener, Router::new().fallback(answer).with_state(stub)).await.unwrap()
  });
  base
}

fn editor() -> Session {
  Session {
    fragment_desk: Some(PathBuf::from(DESK)),
    ..Session::default()
  }
}

#[test]
fn the_manifest_declares_each_desk_entry_as_the_executable_answers_it_for_an_editor_alone() {
  assert_eq!(
    differences(Path::new(env!("CARGO_MANIFEST_DIR")), fragment_tool::ENTRIES),
    Vec::<String>::new()
  );
  assert!(
    fragment_tool::ENTRIES
      .iter()
      .all(|entry| entry.namespace == "fragments" && entry.capabilities == ["fragments.desk"])
  );
  assert_eq!(fragment_tool::ENTRIES.len(), 6);
}

#[tokio::test]
async fn each_entry_is_asked_of_its_desk_route_with_the_records_desk() {
  let stub = Stub::default();
  let brain = Brain::at(&stood(stub.clone()).await, "t");
  let piece = json!({ "path": "knowledge/a.md", "title": TITLE, "keywords": "deploy-order", "items": [758], "shared": null });

  for (entry, arguments) in [
    ("fragment_search", json!({ "text": "deploy", "item": 758, "status": "open" })),
    (
      "fragment_write",
      json!({ "path": "knowledge/a.md", "title": TITLE, "keywords": "deploy-order", "items": [758], "for_every_agent": true, "distinct_from": ["knowledge/b.md"] }),
    ),
    ("fragment_remove", json!({ "path": "knowledge/a.md" })),
    (
      "fragment_merge",
      json!({ "into": "knowledge/a.md", "from": ["knowledge/b.md"], "title": TITLE }),
    ),
    ("fragment_split", json!({ "from": "knowledge/a.md", "pieces": [piece] })),
    ("fragment_owed", json!({})),
  ] {
    let answered = fragment_tool::call_on(&brain, &editor(), entry, arguments).await;

    assert_eq!(answered.plain(), "done", "{entry}");
  }

  let taken = stub.taken.lock().unwrap().clone();
  let routes: Vec<&str> = taken.iter().map(|(path, _)| path.as_str()).collect();

  assert_eq!(
    routes,
    [
      "/fragments/desk/search",
      "/fragments/desk/write",
      "/fragments/desk/remove",
      "/fragments/desk/merge",
      "/fragments/desk/split",
      "/fragments/desk/owed"
    ]
  );
  assert!(taken.iter().all(|(_, body)| body["desk"] == json!(DESK)));
  assert_eq!(
    taken[0].1,
    json!({ "desk": DESK, "text": "deploy", "item": 758, "person": null, "status": "open", "down": null })
  );
  assert_eq!(taken[1].1["forEveryAgent"], json!(true));
  assert_eq!(taken[1].1["distinctFrom"], json!(["knowledge/b.md"]));
  assert_eq!(taken[1].1["items"], json!([758]));
  assert_eq!(taken[3].1["items"], json!(null));
  assert_eq!(taken[4].1["pieces"][0]["keywords"], json!("deploy-order"));
}

#[tokio::test]
async fn a_run_with_no_desk_a_refusal_and_bad_arguments_are_said_as_the_tool_says_them() {
  let stub = Stub::default();
  let brain = Brain::at(&stood(stub.clone()).await, "t");
  let deskless = fragment_tool::call_on(&brain, &Session::default(), "fragment_owed", json!({})).await;

  assert!(deskless.is_error);
  assert_eq!(
    deskless.plain(),
    "REFUSED: This run has no fragment desk: the tool serves an editor batch's run, and nothing else."
  );
  assert!(stub.taken.lock().unwrap().is_empty());

  stub.answers.lock().unwrap().insert(
    "/fragments/desk/remove".into(),
    json!({ "answered": false, "refusal": { "code": "nothing-stands", "message": "Nothing stands at knowledge/x.md: no file and no row." } }),
  );

  let refused =
    fragment_tool::call_on(&brain, &editor(), "fragment_remove", json!({ "path": "knowledge/x.md" })).await;

  assert!(refused.is_error);
  assert_eq!(refused.plain(), "REFUSED: Nothing stands at knowledge/x.md: no file and no row.");

  for (entry, arguments) in [
    ("fragment_write", json!({ "path": "a", "title": "t", "keywords": "k", "items": [0] })),
    ("fragment_merge", json!({ "into": "a", "from": [], "title": "t" })),
    ("fragment_split", json!({ "from": "a", "pieces": [] })),
  ] {
    let answered = fragment_tool::call_on(&brain, &editor(), entry, arguments).await;

    assert!(
      answered.plain().starts_with(&format!("ERROR: invalid arguments: {entry}")),
      "{}",
      answered.plain()
    );
  }
}

#[tokio::test]
async fn the_executable_answers_one_request_on_its_standard_streams() {
  let stub = Stub::default();
  let base = stood(stub.clone()).await;
  let data = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(data.path().join("env/brain.env"), format!("BRAIN_ADDRESS={base}\nBRAIN_TOKEN=t\n"))
    .unwrap();
  stub.answers.lock().unwrap().insert(
    "/fragments/desk/owed".into(),
    json!({ "answered": true, "lines": ["Nothing is owed: every piece this run touched has its row."] }),
  );

  let request = json!({
    "protocol": 1,
    "kind": "call",
    "entry": "fragments/fragment_owed",
    "arguments": {},
    "session": { "line": "editor", "capabilities": ["fragments.desk"], "dataRoot": data.path(), "archive": "", "fragmentDesk": DESK },
  });
  let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_fragment-tool"))
    .env_remove("BRAIN_ADDRESS")
    .env_remove("BRAIN_TOKEN")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .unwrap();

  {
    use tokio::io::AsyncWriteExt;

    child.stdin.take().unwrap().write_all(request.to_string().as_bytes()).await.unwrap();
  }

  let done = child.wait_with_output().await.unwrap();

  assert!(done.status.success());
  assert_eq!(
    serde_json::from_slice::<Value>(&done.stdout).unwrap()["content"][0]["text"],
    json!("Nothing is owed: every piece this run touched has its row.")
  );
  assert_eq!(stub.taken.lock().unwrap()[0].1, json!({ "desk": DESK }));
}
