//! The harness plugin: `bmo-brain mcp --brain <url>` starts an MCP server on standard input and output that
//! lists exactly the plugin's thirty-eight tools, and each call reaches the served brain through its client. The
//! command runs as its own process, spoken to over its pipes, and is taken down before each test ends.

mod support;

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use support::{Served, TOKEN};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// How long a test lets the server take to answer one request before it fails; a guard, never a wait.
const PATIENCE: Duration = Duration::from_secs(30);

/// The tools the plugin offers.
const TOOLS: [&str; 38] = [
  "brain_brains",
  "brain_brain_add",
  "brain_subjects",
  "brain_subject_add",
  "brain_layers",
  "brain_layer_add",
  "brain_memories_list",
  "brain_memory_write",
  "brain_memory_read",
  "brain_memory_version",
  "brain_memory_move",
  "brain_hits",
  "brain_confirmations",
  "brain_types",
  "brain_recall",
  "brain_fold",
  "brain_learn",
  "brain_similar",
  "brain_unconsolidated",
  "brain_consolidated",
  "brain_persona",
  "brain_memories",
  "brain_memory_add",
  "brain_memory",
  "brain_memory_update",
  "brain_notes",
  "brain_note_add",
  "brain_watches",
  "brain_watch_add",
  "brain_watch_update",
  "brain_reminders",
  "brain_reminder_add",
  "brain_reminder_cancel",
  "brain_fragments",
  "brain_fragment_put",
  "brain_knowledge",
  "brain_lock",
  "brain_unlock",
];

struct Server {
  child: Child,
  input: ChildStdin,
  output: Lines<BufReader<ChildStdout>>,
  next: u64,
}

impl Server {
  async fn start(brain: &str, token: &str) -> Self {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bmo-brain"));
    command
      .args(["mcp", "--brain", brain])
      .env("BRAIN_TOKEN", token)
      .env_remove("BRAIN_URL")
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .kill_on_drop(true);
    let mut child = command.spawn().expect("the command starts");
    let input = child.stdin.take().unwrap();
    let output = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut server = Self {
      child,
      input,
      output,
      next: 1,
    };
    let initialised = server
      .ask(
        "initialize",
        json!({
          "protocolVersion": "2025-06-18",
          "capabilities": {},
          "clientInfo": { "name": "tester", "version": "1" },
        }),
      )
      .await;
    assert_eq!(initialised["result"]["serverInfo"]["name"], "bmo-brain");
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    server
  }

  async fn send(&mut self, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    self.input.write_all(line.as_bytes()).await.unwrap();
    self.input.flush().await.unwrap();
  }

  async fn ask(&mut self, method: &str, params: Value) -> Value {
    let id = self.next;
    self.next += 1;
    self
      .send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
      .await;
    loop {
      let line = tokio::time::timeout(PATIENCE, self.output.next_line()).await.unwrap().unwrap().unwrap();
      let message: Value = serde_json::from_str(&line).unwrap();
      if message["id"] == json!(id) {
        return message;
      }
    }
  }

  async fn call(&mut self, tool: &str, arguments: Value) -> Value {
    self.ask("tools/call", json!({ "name": tool, "arguments": arguments })).await["result"].clone()
  }

  async fn stop(mut self) {
    self.child.kill().await.unwrap();
  }
}

#[tokio::test]
async fn the_server_lists_exactly_the_plugins_tools() {
  let served = Served::new().await;
  let mut server = Server::start(&served.base, TOKEN).await;
  let listed = server.ask("tools/list", json!({})).await;
  let names: Vec<&str> = listed["result"]["tools"]
    .as_array()
    .unwrap()
    .iter()
    .map(|tool| tool["name"].as_str().unwrap())
    .collect();
  assert_eq!(names, TOOLS);
  for tool in listed["result"]["tools"].as_array().unwrap() {
    assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
  }
  server.stop().await;
}

#[tokio::test]
async fn a_call_reaches_the_brain_and_a_refusal_comes_back_as_a_tool_error() {
  let served = Served::new().await;
  let mut server = Server::start(&served.base, TOKEN).await;
  let added = server
    .call(
      "brain_memory_add",
      json!({ "scope": "room:a", "kind": "promise", "text": "Call back", "conditions": [], "labels": {} }),
    )
    .await;
  assert_eq!(added["isError"], json!(false), "{added}");
  let id = added["structuredContent"]["id"].as_i64().unwrap();
  let updated = server.call("brain_memory_update", json!({ "memoryId": id, "state": "done" })).await;
  assert_eq!(updated["structuredContent"]["state"], "done");
  let listed = server.call("brain_memories", json!({ "scope": "room:a" })).await;
  assert_eq!(listed["structuredContent"]["items"][0]["id"], json!(id));

  let missing = server.call("brain_memory", json!({ "memoryId": id + 9 })).await;
  assert_eq!(missing["isError"], json!(true));
  let said: Value = serde_json::from_str(missing["content"][0]["text"].as_str().unwrap()).unwrap();
  assert_eq!((said["status"].as_u64(), said["code"].as_str()), (Some(404), Some("not-found")));
  let cancelled = server.call("brain_reminder_cancel", json!({ "reminderId": 1 })).await;
  assert_eq!(cancelled["isError"], json!(true));
  server.stop().await;
}
