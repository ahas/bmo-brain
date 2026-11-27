//! `bmo-brain holder declare <file>` and `holder withdraw <key>` (docs/boundary-api.md, 「Declarations and
//! holders」): a service's standing set read from a TOML file in its own repository and stood through the brain's
//! routes, at the address and with the token the data root's env file holds, the file the brain's tool reads.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::tool_env::{ADDRESS_VARIABLE, ENV_FILE, TOKEN_VARIABLE};

/// A holder's set as its file declares it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HolderFile {
  pub held_by: String,
  #[serde(rename = "memory", default)]
  pub memories: Vec<HeldMemory>,
}

/// One row of a holder's set.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldMemory {
  pub key: String,
  #[serde(rename = "type")]
  pub kind: String,
  pub text: String,
  pub recall_condition: String,
  pub importance: Option<i64>,
}

impl HolderFile {
  /// The body `POST /declarations` takes.
  pub fn declaration(&self) -> Value {
    let memories: Vec<Value> = self
      .memories
      .iter()
      .map(|memory| {
        let mut row = json!({
          "key": memory.key,
          "type": memory.kind,
          "text": memory.text,
          "recallCondition": memory.recall_condition,
        });

        if let Some(importance) = memory.importance {
          row["importance"] = importance.into();
        }

        row
      })
      .collect();

    json!({ "heldBy": self.held_by, "memories": memories })
  }
}

/// Where the brain stands and the token it takes, as the data root's env file holds them; none where the file
/// does not stand.
pub struct Reach {
  pub address: String,
  pub token: String,
}

/// The data root: the one `BMO_DATA_DIR` names, else the machine's.
pub fn data_root() -> PathBuf {
  std::env::var_os("BMO_DATA_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".local/share/bmo/data"))
}

/// The brain's reach read off the env file under `root`, each line's key before its first `=` and its value
/// verbatim after it; none where the file does not stand or names either value nowhere.
pub fn reach(root: &Path) -> Option<Reach> {
  let text = std::fs::read_to_string(root.join(ENV_FILE)).ok()?;
  let value = |name: &str| {
    text.lines().find_map(|line| {
      line
        .split_once('=')
        .filter(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.to_owned())
    })
  };

  Some(Reach {
    address: value(ADDRESS_VARIABLE)?.trim_end_matches('/').to_owned(),
    token: value(TOKEN_VARIABLE)?,
  })
}

/// What a step came to: the line it prints and its exit code.
pub struct Outcome {
  pub line: String,
  pub code: u8,
}

fn said(line: impl Into<String>, code: u8) -> Outcome {
  Outcome {
    line: line.into(),
    code,
  }
}

// The brain's client at the reach, or the line saying why none stands.
fn client(reach: &Reach, verb: &str) -> Result<bmo_brain_client::BrainClient, Outcome> {
  bmo_brain_client::BrainClient::new(&reach.address, reach.token.clone())
    .map_err(|why| said(format!("holder {verb}: the brain's address does not read: {why}"), 1))
}

/// The holder's set the file declares, stood on the brain.
pub async fn declare(file: &Path) -> Outcome {
  let text = match std::fs::read_to_string(file) {
    Ok(text) => text,
    Err(why) => return said(format!("holder declare: {} could not be read: {why}", file.display()), 1),
  };
  let held: HolderFile = match toml::from_str(&text) {
    Ok(held) => held,
    Err(why) => return said(format!("holder declare: {} is no holder's set: {why}", file.display()), 1),
  };
  let body: bmo_brain_client::Declaration = match serde_json::from_value(held.declaration()) {
    Ok(body) => body,
    Err(why) => return said(format!("holder declare: {} is no holder's set: {why}", file.display()), 1),
  };
  let Some(reach) = reach(&data_root()) else {
    return said(
      format!(
        "holder declare: no brain is reached from this machine ({ENV_FILE} stands nowhere); nothing declared"
      ),
      0,
    );
  };
  let client = match client(&reach, "declare") {
    Ok(client) => client,
    Err(outcome) => return outcome,
  };

  match client.declare(&body).await {
    Ok(_) => said(
      format!("holder declare: {} stands with {} memories", held.held_by, held.memories.len()),
      0,
    ),
    Err(why) => said(format!("holder declare: {why}"), 1),
  }
}

/// The holder withdrawn from every brain.
pub async fn withdraw(key: &str) -> Outcome {
  let Some(reach) = reach(&data_root()) else {
    return said(
      format!(
        "holder withdraw: no brain is reached from this machine ({ENV_FILE} stands nowhere); nothing withdrawn"
      ),
      0,
    );
  };
  let client = match client(&reach, "withdraw") {
    Ok(client) => client,
    Err(outcome) => return outcome,
  };

  match client.holder_withdraw(key).await {
    Ok(_) => said(format!("holder withdraw: {key} withdrawn"), 0),
    Err(why) => said(format!("holder withdraw: {why}"), 1),
  }
}
