//! The parts of a request's session record the brain's tools read, and the turn the request is made in, read
//! afresh from it (docs/boundary-api.md, 「Its tools」). The record is the launcher's; a field it holds that no tool
//! of the brain's reads is passed over.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

/// The turn variable naming the agent the turn is.
pub const AGENT_VARIABLE: &str = "BMO_TURN_AGENT";

/// The turn variable naming the room the turn answers.
pub const CHAT_VARIABLE: &str = "BMO_TURN_CHAT";

/// One session's record, its field names camel case as the launcher writes them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
  /// The data root the run reads.
  #[serde(default)]
  pub data_root: PathBuf,
  /// A resident program's place, where each request reads which run is going.
  #[serde(default)]
  pub place: Option<PathBuf>,
  /// The run's own variables, for a run spawned for itself.
  #[serde(default)]
  pub run_variables: BTreeMap<String, String>,
  /// The fragment desk an editor's batch opened.
  #[serde(default)]
  pub fragment_desk: Option<PathBuf>,
}

impl Session {
  /// One variable of the turn the request is made in: the run the resident program's place names — none for a
  /// person's turn — standing over the run's own variables.
  pub fn turn_variable(&self, name: &str) -> Option<String> {
    let from_place = self.place.as_ref().and_then(|place| {
      let run: Value = serde_json::from_str(&std::fs::read_to_string(place.join("run.json")).ok()?).ok()?;

      if run.get("person").and_then(Value::as_bool) == Some(true) {
        return None;
      }

      run.get("env")?.get(name)?.as_str().map(str::to_owned)
    });

    from_place.or_else(|| self.run_variables.get(name).cloned())
  }
}

/// What a request reads of its turn: the agent it is and the room it answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Turn {
  pub agent: Option<String>,
  pub chat: Option<i64>,
}

impl Turn {
  /// The turn the record's request is made in.
  pub fn of(session: &Session) -> Self {
    let named = |name: &str| {
      session
        .turn_variable(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    };

    Self {
      agent: named(AGENT_VARIABLE),
      chat: named(CHAT_VARIABLE).and_then(|chat| chat.parse().ok()),
    }
  }
}
