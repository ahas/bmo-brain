//! Where the brain's tools find it (docs/boundary-api.md, 「Its tools」): the binary's `tool-env` writes the data
//! root's env file from the configuration as a deployment runs it, readable by the user alone, rewritten only where
//! it would change, refused with nothing written for a value an env file cannot hold bare; and the tools' own
//! reader finds the brain in what it wrote.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use bmo_brain::tool_env::{ADDRESS_VARIABLE, ENV_FILE, TOKEN_VARIABLE, base_address};
use tool_call::Brain;

// A configuration file in `folder` for a service listening on `listen` with `token`, its data root `data`; the
// database and the other units are named only, since writing the env file reaches none of them.
fn configuration(folder: &Path, data: &Path, listen: &str, token: &str) -> std::path::PathBuf {
  // A string as a TOML basic string, whose escapes are JSON's.
  let quoted = |text: &str| serde_json::json!(text).to_string();
  let path = |at: &Path| quoted(&at.to_string_lossy());
  let mut config = vec![
    format!("listen = {}", quoted(listen)),
    format!("token = {}", quoted(token)),
    format!("database = {}", quoted("postgres://nowhere/brain")),
    format!("data_root = {}", path(data)),
    format!("brain = {}", path(&folder.join("brain"))),
    format!("prompts = {}", path(&folder.join("prompts"))),
    format!("chat_key = {}", quoted("agent-a")),
    format!("service_unit = {}", quoted("brain-service")),
    format!("own_command = [{}]", quoted("bmo-brain")),
  ];

  for peer in ["agency", "receiver", "gate", "job"] {
    config.extend([
      format!("[{peer}]"),
      format!("base = {}", quoted("http://127.0.0.1:9")),
      format!("token = {}", quoted("unused")),
    ]);
  }

  let at = folder.join("brain.toml");

  std::fs::write(&at, config.join("\n")).unwrap();
  at
}

// The binary run as the deployment's step runs it, from the folder its configuration stands in.
fn tool_env(config: &Path) -> Output {
  Command::new(env!("CARGO_BIN_EXE_bmo-brain"))
    .current_dir(config.parent().unwrap())
    .args(["--config", "brain.toml", "tool-env"])
    .env_remove(ADDRESS_VARIABLE)
    .env_remove(TOKEN_VARIABLE)
    .output()
    .unwrap()
}

fn said(output: &Output) -> (String, String) {
  (
    String::from_utf8_lossy(&output.stdout).into_owned(),
    String::from_utf8_lossy(&output.stderr).into_owned(),
  )
}

#[test]
fn the_base_address_is_the_listen_address_over_http_an_unspecified_host_the_loopback_one() {
  assert_eq!(base_address("127.0.0.1:3230"), "http://127.0.0.1:3230");
  assert_eq!(base_address(" 0.0.0.0:3230 "), "http://127.0.0.1:3230");
  assert_eq!(base_address("[::]:3230"), "http://[::1]:3230");
  assert_eq!(base_address("[::1]:3230"), "http://[::1]:3230");
  assert_eq!(base_address("localhost:3230"), "http://localhost:3230");
}

#[test]
fn the_env_file_is_the_one_the_tools_read() {
  assert_eq!(ENV_FILE, tool_call::ENV_FILE);
  assert_eq!(ADDRESS_VARIABLE, tool_call::ADDRESS_VARIABLE);
  assert_eq!(TOKEN_VARIABLE, tool_call::TOKEN_VARIABLE);
}

#[test]
fn the_env_file_is_written_from_the_configuration_privately_and_only_where_it_would_change() {
  let folder = tempfile::tempdir().unwrap();
  let data = folder.path().join("data");
  let config = configuration(folder.path(), &data, "0.0.0.0:3230", "brain-secret");
  let written = data.join(ENV_FILE);

  // The data root's env folder is made where it is missing.
  let first = tool_env(&config);

  assert!(first.status.success(), "{:?}", said(&first));
  assert_eq!(said(&first).0, format!("bmo-brain: wrote {}\n", written.display()));
  assert_eq!(
    std::fs::read_to_string(&written).unwrap(),
    "BRAIN_ADDRESS=http://127.0.0.1:3230\nBRAIN_TOKEN=brain-secret\n"
  );
  assert_eq!(std::fs::metadata(&written).unwrap().permissions().mode() & 0o777, 0o600);
  assert!(!data.join("env/brain.env.new").exists());

  // The tools' own reader, under the same data root, finds the brain in it.
  let brain = Brain::under(&data).unwrap();

  assert_eq!((brain.base.as_str(), brain.token.as_str()), ("http://127.0.0.1:3230", "brain-secret"));

  // Run again unchanged, it leaves the file as it stands.
  let modified = std::fs::metadata(&written).unwrap().modified().unwrap();
  let again = tool_env(&config);

  assert!(again.status.success(), "{:?}", said(&again));
  assert_eq!(said(&again).0, format!("bmo-brain: {} already stands as written\n", written.display()));
  assert_eq!(std::fs::metadata(&written).unwrap().modified().unwrap(), modified);

  // A new token is written over the old one.
  let config = configuration(folder.path(), &data, "127.0.0.1:4000", "rotated");
  let rotated = tool_env(&config);

  assert!(rotated.status.success(), "{:?}", said(&rotated));
  assert_eq!(
    std::fs::read_to_string(&written).unwrap(),
    "BRAIN_ADDRESS=http://127.0.0.1:4000\nBRAIN_TOKEN=rotated\n"
  );
}

#[test]
fn a_value_an_env_file_cannot_hold_bare_writes_nothing() {
  for token in ["two words", "quoted'", "a#b", "$HOME", "back\\slash", ""] {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let config = configuration(folder.path(), &data, "127.0.0.1:3230", token);
    let refused = tool_env(&config);

    assert_eq!(refused.status.code(), Some(1), "{token:?}: {:?}", said(&refused));
    assert!(said(&refused).1.contains("BRAIN_TOKEN cannot stand bare in env/brain.env"), "{token:?}");
    assert!(!data.join(ENV_FILE).exists(), "{token:?}");
  }
}
