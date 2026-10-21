//! The caller's own records (docs/boundary-api.md, 「A caller's records」): a room's rules, a session's loaded
//! documents and a chat session's handoff note.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One rule a caller grants in a room: its id, the subject it is about and its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomRule {
  pub id: String,
  pub subject: i64,
  pub text: String,
}

/// A rule's words, as it is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuleText {
  pub subject: i64,
  pub text: String,
}

/// The room whose rules are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomRulesAsk {
  pub chat_id: i64,
}

/// A room's rules, in the order they were set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomRules {
  pub rules: Vec<RoomRule>,
}

/// A rule set or revoked on a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuleSet {
  pub chat_id: i64,
  pub message_id: i64,
  pub id: String,
  /// The rule's words, or null to revoke it.
  pub rule: Option<RuleText>,
}

/// The session whose loads are read or let go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionAsk {
  pub session: String,
}

/// One document a session was handed, at the version it read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Load {
  pub path: String,
  pub version: String,
}

/// A session's loads, by path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionLoads {
  pub loads: Vec<Load>,
}

/// The loads a session keeps, read at `at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LoadsKeep {
  pub session: String,
  pub loads: Vec<Load>,
  /// When they were read, in seconds; now where none is given.
  #[serde(default)]
  pub at: Option<i64>,
}

/// The agent whose handoff note is read, kept or cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentAsk {
  pub agent: String,
}

/// The handoff note an agent's chat session left, or null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionNote {
  pub note: Option<String>,
}

/// A handoff note kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NoteKeep {
  pub agent: String,
  pub note: String,
}

/// That the call was done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Done {
  pub ok: bool,
}
