//! The service's binary as its unit starts it (docs/boundary-api.md, 「Configuration」 and 「Health」): `serve` over a
//! configuration file naming a scratch database, a brain repository and a data root of the test's own, answering
//! its health route with the token and refusing a call without it, and ending cleanly when it is told to stop.

mod support;

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use support::editing::repository;
use support::scratch::Scratch;
use support::sorting::prompts_root;
use support::{AGENT, OWNER};

const TOKEN: &str = "server-token";

// A string as a TOML basic string, whose escapes are JSON's.
fn quoted(text: &str) -> String {
  json!(text).to_string()
}

fn path(at: &std::path::Path) -> String {
  quoted(&at.to_string_lossy())
}

// A port nothing listens on now, which the server is then told to take.
fn free_port() -> u16 {
  std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

// The process stopped as its unit stops it, and how it ended.
fn terminate(child: &mut Child) -> Option<std::process::ExitStatus> {
  Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();

  let deadline = Instant::now() + Duration::from_secs(20);

  while Instant::now() < deadline {
    if let Some(status) = child.try_wait().unwrap() {
      return Some(status);
    }

    std::thread::sleep(Duration::from_millis(50));
  }

  child.kill().ok();
  None
}

#[tokio::test]
async fn the_binary_serves_on_a_scratch_database_and_stops_when_told() {
  let scratch = Scratch::new().await;
  let brain = tempfile::tempdir().unwrap();
  let data = tempfile::tempdir().unwrap();
  let listen = format!("127.0.0.1:{}", free_port());
  let nowhere = format!("http://127.0.0.1:{}", free_port());
  let mut config = vec![
    format!("listen = {}", quoted(&listen)),
    format!("token = {}", quoted(TOKEN)),
    format!("database = {}", quoted(&scratch.address())),
    format!("data_root = {}", path(data.path())),
    format!("brain = {}", path(brain.path())),
    format!("prompts = {}", path(&prompts_root())),
    format!("chat_key = {}", quoted(AGENT)),
    format!("owner = {OWNER}"),
    format!("service_unit = {}", quoted("brain-service")),
    format!("own_command = [{}]", quoted(env!("CARGO_BIN_EXE_bmo-brain"))),
  ];

  for peer in ["agency", "receiver", "gate"] {
    config.extend([
      format!("[{peer}]"),
      format!("base = {}", quoted(&nowhere)),
      "token = \"unused\"".to_owned(),
    ]);
  }

  repository(brain.path());

  let file = data.path().join("brain.toml");

  std::fs::write(&file, config.join("\n")).unwrap();

  let mut child = Command::new(env!("CARGO_BIN_EXE_bmo-brain"))
    .arg("--config")
    .arg(&file)
    .arg("serve")
    .stdout(Stdio::null())
    .stderr(Stdio::inherit())
    .spawn()
    .unwrap();
  let client = reqwest::Client::new();
  let health = format!("http://{listen}/health");
  let deadline = Instant::now() + Duration::from_secs(60);
  let mut answer = None;

  while Instant::now() < deadline && answer.is_none() {
    if let Some(status) = child.try_wait().unwrap() {
      panic!("the server ended before it answered: {status}");
    }

    if let Ok(response) = client.get(&health).bearer_auth(TOKEN).send().await {
      let status = response.status().as_u16();

      answer = Some((status, serde_json::from_slice::<Value>(&response.bytes().await.unwrap()).unwrap()));
    } else {
      tokio::time::sleep(Duration::from_millis(100)).await;
    }
  }

  let unauthorised = client.get(&health).send().await.map(|response| response.status().as_u16());
  let ended = terminate(&mut child);

  assert_eq!(answer, Some((200, json!({ "ready": true, "reason": null }))));
  assert_eq!(unauthorised.ok(), Some(401));
  assert!(ended.is_some_and(|status| status.success()), "{ended:?}");
}

#[test]
fn a_configuration_it_cannot_read_ends_the_binary_at_once() {
  let data = tempfile::tempdir().unwrap();
  let file = data.path().join("brain.toml");

  std::fs::write(&file, "listen = \"127.0.0.1:1\"\nunknown = 1\n").unwrap();

  let ran = Command::new(env!("CARGO_BIN_EXE_bmo-brain"))
    .arg("--config")
    .arg(&file)
    .arg("serve")
    .output()
    .unwrap();

  assert_eq!(ran.status.code(), Some(2));
  assert!(String::from_utf8_lossy(&ran.stderr).contains("the configuration could not be read"));
}
