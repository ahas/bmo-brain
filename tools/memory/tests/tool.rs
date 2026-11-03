//! The memory tool: its manifest held to the entries the executable answers, each entry's schema a JSON
//! Schema beside it, its argument parsing, and the executable answering calls against a stand-in brain
//! (docs/boundary-api.md, 「The memory tool」).

use std::collections::BTreeMap;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use memory_tool::{ENTRIES, parse};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const TOKEN: &str = "test-token";
const BRAIN: i64 = 1;

type Params = BTreeMap<String, String>;

#[test]
fn the_manifest_declares_exactly_the_entries_the_executable_answers_each_with_a_schema_beside_it() {
  let manifest: toml::Value =
    toml::from_str(&std::fs::read_to_string(format!("{ROOT}/tool.toml")).unwrap()).unwrap();
  assert_eq!(manifest["tool"]["name"].as_str(), Some("memory"));
  assert_eq!(manifest["tool"]["protocol"].as_integer(), Some(1));
  assert_eq!(manifest["tool"]["timeout"].as_integer(), Some(60));
  assert_eq!(manifest["tool"]["run"].as_array().unwrap()[0].as_str(), Some("memory-tool"));
  let entries = manifest["entry"].as_array().unwrap();
  let declared: Vec<&str> = entries.iter().map(|entry| entry["name"].as_str().unwrap()).collect();
  assert_eq!(declared, ENTRIES);
  for entry in entries {
    assert_eq!(entry["namespace"].as_str(), Some("bmo"));
    assert!(entry.get("grants").is_none(), "the memory tool holds no grant");
    assert!(
      entry["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability.as_str() == Some("room.tools"))
    );
    let schema = entry["input"].as_str().unwrap();
    let text = std::fs::read_to_string(format!("{ROOT}/{schema}")).unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(schema["type"].as_str(), Some("object"), "{schema}");
    assert_eq!(schema["additionalProperties"].as_bool(), Some(false), "{schema}");
    for required in schema["required"].as_array().into_iter().flatten() {
      let name = required.as_str().unwrap();
      assert!(schema["properties"].get(name).is_some(), "{schema}");
    }
  }
}

#[test]
fn the_arguments_read_by_the_names_the_schemas_give_and_a_missing_or_unknown_one_is_refused() {
  assert!(
    parse(
      "bmo/memory_list",
      &json!({ "type": "promise", "all": true, "global": false, "chat": 7 })
    )
    .is_ok()
  );

  let why = parse("bmo/memory_add", &json!({ "action": "Paint the fence." })).unwrap_err();
  assert!(why.contains("type is required"), "{why}");

  let why = parse("bmo/memory_add", &json!({ "type": "epic", "action": "Paint the fence." })).unwrap_err();
  assert!(why.contains("promise"), "{why}");

  let why = parse("bmo/memory_add", &json!({ "type": "promise" })).unwrap_err();
  assert!(why.contains("action is required"), "{why}");

  let why = parse("bmo/memory_update", &json!({ "action": "Paint the fence." })).unwrap_err();
  assert!(why.contains("memory is required"), "{why}");

  let why = parse("bmo/memory_retire", &json!({ "memory": "12" })).unwrap_err();
  assert!(why.contains("why is required"), "{why}");

  let why = parse("bmo/memory_candidate", &json!({ "why": "It may be wanted." })).unwrap_err();
  assert!(why.contains("memory is required"), "{why}");

  assert!(
    parse(
      "bmo/memory_add",
      &json!({ "type": "promise", "action": "Paint the fence.", "for": 3, "by": 4, "message": 5 })
    )
    .is_ok()
  );
}

#[test]
fn a_call_missing_an_argument_is_refused_before_the_brain_is_reached() {
  let answered = called("http://127.0.0.1:1", "memory_add", json!({ "action": "Paint the fence." }));
  assert_eq!(answered["isError"], json!(true), "{answered}");
  let text = answered["content"][0]["text"].as_str().unwrap();
  assert!(text.contains("type is required"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_executable_lists_the_rooms_memories_and_writes_by_the_turn_room() {
  let (stand, base, serving) = served().await;

  let listed = called(&base, "memory_list", json!({ "type": "promise" }));
  assert_eq!(listed["isError"], json!(false), "{listed}");
  assert_eq!(
    listed["structuredContent"]["lines"],
    json!(["#1 [promise] Paint the fence.", "#2 [promise] Buy paint."])
  );
  assert_eq!(
    listed["content"][0]["text"],
    json!("#1 [promise] Paint the fence.\n#2 [promise] Buy paint.")
  );

  let calls = stand.calls();
  let brains = calls.iter().find(|call| call.route == "/brains").unwrap();
  assert_eq!(brains.params.get("agentKey").map(String::as_str), Some("agent-a"));
  let memories = calls.iter().find(|call| call.route == "/memories").unwrap();
  assert_eq!(memories.params.get("type").map(String::as_str), Some("promise"));
  assert_eq!(memories.params.get("status").map(String::as_str), Some("active"));
  assert_eq!(memories.params.get("subjectId").map(String::as_str), Some("10"));

  let every = called(&base, "memory_list", json!({ "all": true, "global": true }));
  assert_eq!(every["structuredContent"]["lines"], json!(["No memories stand."]));
  assert_eq!(every["content"][0]["text"], json!("No memories stand."));
  let last = stand
    .calls()
    .into_iter()
    .rfind(|call| call.route == "/memories" && call.body.is_none())
    .unwrap();
  assert!(!last.params.contains_key("status"), "{:?}", last.params);
  assert!(!last.params.contains_key("subjectId"), "{:?}", last.params);

  let added = called(
    &base,
    "memory_add",
    json!({
      "type": "followup",
      "action": "Call the plumber.",
      "condition": "the leak returns",
      "slug": "plumber-call",
      "for": 3,
    }),
  );
  assert_eq!(added["isError"], json!(false), "{added}");
  assert_eq!(
    added["structuredContent"],
    json!({ "memoryId": 20, "type": "followup", "status": "active" })
  );
  assert_eq!(added["content"][0]["text"], json!("Memory #20 [followup] is active."));

  let posted = stand
    .calls()
    .into_iter()
    .rev()
    .find(|call| call.route == "/memories" && call.body.is_some())
    .unwrap();
  let body = posted.body.as_ref().unwrap();
  assert_eq!(body["type"], json!("followup"));
  assert_eq!(body["text"], json!("Call the plumber."));
  assert_eq!(body["key"], json!("plumber-call"));
  assert_eq!(body["recallCondition"], json!("the leak returns"));
  assert_eq!(body["status"], json!("active"));
  assert_eq!(body["actorName"], json!("agent-session"));
  assert_eq!(
    body["subjects"],
    json!([{ "type": "room", "key": "7" }, { "type": "person", "key": "3" }])
  );

  serving.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_executable_resolves_a_key_and_moves_a_named_memory() {
  let (stand, base, serving) = served().await;

  let updated = called(
    &base,
    "memory_update",
    json!({ "memory": "plumber-call", "action": "Call the plumber before noon." }),
  );
  assert_eq!(updated["isError"], json!(false), "{updated}");
  assert_eq!(
    updated["structuredContent"],
    json!({ "memoryId": 30, "lineageId": 1, "status": "active" })
  );

  let calls = stand.calls();
  let looked_up = calls
    .iter()
    .find(|call| {
      call.route == "/memories" && call.params.get("key").map(String::as_str) == Some("plumber-call")
    })
    .unwrap();
  assert_eq!(looked_up.params.get("status").map(String::as_str), Some("active,pending"));
  let versioned = stand.calls().into_iter().find(|call| call.route == "/memories/1/versions").unwrap();
  let body = versioned.body.as_ref().unwrap();
  assert_eq!(body["text"], json!("Call the plumber before noon."));
  assert_eq!(body["actorName"], json!("agent-session"));

  let done = called(&base, "memory_done", json!({ "memory": "1", "text": "The plumber came." }));
  assert_eq!(done["structuredContent"], json!({ "memoryId": 1, "status": "done" }));
  let moved = stand
    .calls()
    .into_iter()
    .rev()
    .find(|call| call.route == "/memories/1" && call.body.is_some())
    .unwrap();
  assert_eq!(moved.body.as_ref().unwrap()["text"], json!("The plumber came."));

  let retired = called(&base, "memory_retire", json!({ "memory": "1", "why": "The plan changed." }));
  assert_eq!(retired["structuredContent"], json!({ "memoryId": 1, "status": "invalidated" }));
  let retired_call = stand
    .calls()
    .into_iter()
    .rev()
    .find(|call| {
      call.route == "/memories/1"
        && call.body.as_ref().is_some_and(|body| body["status"] == json!("invalidated"))
    })
    .unwrap();
  assert_eq!(retired_call.body.as_ref().unwrap()["reason"], json!("The plan changed."));

  let revived = called(&base, "memory_revive", json!({ "memory": "1" }));
  assert_eq!(
    revived["structuredContent"],
    json!({ "memoryId": 40, "lineageId": 1, "status": "active" })
  );
  let restored = stand
    .calls()
    .into_iter()
    .rev()
    .find(|call| call.route == "/memories/1/restorations")
    .unwrap();
  assert_eq!(restored.body.as_ref().unwrap()["actorName"], json!("agent-session"));

  let candidate = called(&base, "memory_candidate", json!({ "memory": "1", "why": "It may be wanted." }));
  assert_eq!(candidate["structuredContent"], json!({ "memoryId": 1, "status": "pending" }));

  let missing = called(&base, "memory_done", json!({ "memory": "no-such-key" }));
  assert_eq!(missing["isError"], json!(true), "{missing}");
  let text = missing["content"][0]["text"].as_str().unwrap();
  assert!(text.contains("no live memory has the key"), "{text}");

  serving.abort();
}

/// The executable handed one call, as the host hands it, with the brain's address in its environment.
fn called(base: &str, entry: &str, arguments: Value) -> Value {
  let handed = json!({
    "protocol": 1,
    "kind": "call",
    "entry": format!("bmo/{entry}"),
    "arguments": arguments,
    "session": {
      "holderName": "agent-a",
      "namespaces": ["bmo"],
      "capabilities": ["room.tools"],
      "variables": { "BMO_TURN_AGENT": "agent-a", "BMO_TURN_CHAT": "7" },
    },
  });
  let mut child = Command::new(env!("CARGO_BIN_EXE_memory-tool"))
    .env_clear()
    .env("BRAIN_ADDRESS", base)
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

/// A stand-in brain: one agent's brain, one room subject, and canned rows.
#[derive(Clone, Default)]
struct Stand(Arc<Mutex<Seen>>);

#[derive(Default)]
struct Seen {
  calls: Vec<Recorded>,
}

#[derive(Clone, Debug)]
struct Recorded {
  route: String,
  params: Params,
  body: Option<Value>,
}

impl Stand {
  fn note(&self, route: &str, params: Params, body: Option<Value>) {
    self.0.lock().unwrap().calls.push(Recorded {
      route: route.to_owned(),
      params,
      body,
    });
  }

  fn calls(&self) -> Vec<Recorded> {
    self.0.lock().unwrap().calls.clone()
  }
}

async fn served() -> (Stand, String, tokio::task::JoinHandle<()>) {
  let stand = Stand::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let app = router(stand.clone());
  let serving = tokio::spawn(async move {
    axum::serve(listener, app).await.unwrap();
  });
  (stand, base, serving)
}

fn router(stand: Stand) -> Router {
  Router::new()
    .route("/brains", get(brains))
    .route("/brains/{brain}/subjects", get(subjects))
    .route("/brains/{brain}/memories", get(memories).post(add_memory))
    .route("/brains/{brain}/memories/{memory}", get(read_memory).patch(move_memory))
    .route("/brains/{brain}/memories/{memory}/versions", post(version_memory))
    .route("/brains/{brain}/memories/{memory}/restorations", post(restore_memory))
    .with_state(stand)
}

async fn brains(State(stand): State<Stand>, Query(params): Query<Params>) -> (StatusCode, Json<Value>) {
  stand.note("/brains", params, None);
  (
    StatusCode::OK,
    Json(json!({
      "items": [{
        "id": BRAIN, "accountKey": "account-a", "agentKey": "agent-a",
        "name": "agent-a", "createdAt": 0
      }],
      "more": false
    })),
  )
}

async fn subjects(
  State(stand): State<Stand>,
  Path(brain): Path<i64>,
  Query(params): Query<Params>,
) -> (StatusCode, Json<Value>) {
  let known = params.get("type").map(String::as_str) == Some("room")
    && params.get("key").map(String::as_str) == Some("7");
  let items = if known {
    json!([{
      "id": 10, "brainId": brain, "type": "room", "key": "7",
      "description": "", "name": null, "createdAt": 0
    }])
  } else {
    json!([])
  };
  stand.note("/subjects", params, None);
  (StatusCode::OK, Json(json!({ "items": items, "more": false })))
}

async fn memories(
  State(stand): State<Stand>,
  Path(brain): Path<i64>,
  Query(params): Query<Params>,
) -> (StatusCode, Json<Value>) {
  let items = if params.get("key").map(String::as_str) == Some("plumber-call") {
    vec![row(1, 1, "followup", "Call the plumber.", "active", brain)]
  } else if params.get("subjectId").map(String::as_str) == Some("10") {
    vec![
      row(1, 1, "promise", "Paint the fence.", "active", brain),
      row(2, 2, "promise", "Buy paint.", "active", brain),
    ]
  } else {
    Vec::new()
  };
  stand.note("/memories", params, None);
  (StatusCode::OK, Json(json!({ "items": items, "more": false })))
}

async fn add_memory(
  State(stand): State<Stand>,
  Path(brain): Path<i64>,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  let kind = body["type"].as_str().unwrap_or("promise").to_owned();
  let text = body["text"].as_str().unwrap_or("").to_owned();
  stand.note("/memories", Params::new(), Some(body));
  (StatusCode::CREATED, Json(row(20, 20, &kind, &text, "active", brain)))
}

async fn read_memory(
  State(stand): State<Stand>,
  Path((brain, memory)): Path<(i64, i64)>,
) -> (StatusCode, Json<Value>) {
  stand.note(&format!("/memories/{memory}"), Params::new(), None);
  if memory == 1 {
    (StatusCode::OK, Json(row(1, 1, "promise", "Paint the fence.", "active", brain)))
  } else {
    (
      StatusCode::NOT_FOUND,
      Json(json!({ "code": "not-found", "error": "nothing stands at the memory" })),
    )
  }
}

async fn move_memory(
  State(stand): State<Stand>,
  Path((brain, memory)): Path<(i64, i64)>,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  let status = body["status"].as_str().unwrap_or("done").to_owned();
  stand.note(&format!("/memories/{memory}"), Params::new(), Some(body));
  (StatusCode::OK, Json(row(memory, memory, "promise", "Paint the fence.", &status, brain)))
}

async fn version_memory(
  State(stand): State<Stand>,
  Path((brain, memory)): Path<(i64, i64)>,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  let text = body["text"].as_str().unwrap_or("").to_owned();
  stand.note(&format!("/memories/{memory}/versions"), Params::new(), Some(body));
  (StatusCode::CREATED, Json(row(30, memory, "promise", &text, "active", brain)))
}

async fn restore_memory(
  State(stand): State<Stand>,
  Path((brain, memory)): Path<(i64, i64)>,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  stand.note(&format!("/memories/{memory}/restorations"), Params::new(), Some(body));
  (
    StatusCode::CREATED,
    Json(row(40, memory, "promise", "Paint the fence.", "active", brain)),
  )
}

fn row(id: i64, lineage: i64, kind: &str, text: &str, status: &str, brain: i64) -> Value {
  json!({
    "id": id, "brainId": brain, "key": null, "type": kind, "text": text,
    "layerId": null, "position": 0,
    "subjects": [{ "id": 10, "type": "room", "key": "7" }],
    "status": status, "lineageId": lineage, "supersededBy": null,
    "validFrom": 0, "validTo": null,
    "recallCondition": null, "releaseCondition": null,
    "importance": 5, "useCount": 0, "lastUsedAt": null, "lastConfirmedAt": null,
    "consolidatedAt": null,
    "actorName": "agent-session", "reason": null, "labels": {}, "createdAt": 0
  })
}
