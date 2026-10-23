//! What every route shares: the error shape and its codes, a page, a cursor, health and an event.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The caller's own metadata: names to values, neither read by the brain.
pub type Labels = std::collections::BTreeMap<String, String>;

/// A JSON object kept as the caller gave it.
pub type Object = serde_json::Map<String, Value>;

/// Every failure: its code and an English sentence saying what went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WireError {
  /// The code, kebab-case.
  pub code: String,
  /// What went wrong.
  pub error: String,
}

/// The codes the brain answers with, each with its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
  BadRequest,
  Unauthorized,
  NoRoute,
  NotFound,
  Conflict,
  Gone,
  Failed,
}

impl ErrorCode {
  /// Every code, in the order of their statuses.
  pub const ALL: [ErrorCode; 7] = [
    Self::BadRequest,
    Self::Unauthorized,
    Self::NoRoute,
    Self::NotFound,
    Self::Conflict,
    Self::Gone,
    Self::Failed,
  ];

  /// The code as it crosses the wire.
  pub fn name(self) -> &'static str {
    match self {
      Self::BadRequest => "bad-request",
      Self::Unauthorized => "unauthorized",
      Self::NoRoute => "no-route",
      Self::NotFound => "not-found",
      Self::Conflict => "conflict",
      Self::Gone => "gone",
      Self::Failed => "failed",
    }
  }

  /// The status it is answered with.
  pub fn status(self) -> u16 {
    match self {
      Self::BadRequest => 400,
      Self::Unauthorized => 401,
      Self::NoRoute | Self::NotFound => 404,
      Self::Conflict => 409,
      Self::Gone => 410,
      Self::Failed => 500,
    }
  }

  /// When it is answered.
  pub fn meaning(self) -> &'static str {
    match self {
      Self::BadRequest => "the body, the query or a key is not the shape the route takes",
      Self::Unauthorized => "the token is missing or wrong",
      Self::NoRoute => "no route answers this method and path",
      Self::NotFound => "nothing stands at the key the path names",
      Self::Conflict => "the member's state does not allow the change, or another holder holds the lock",
      Self::Gone => "the stream was resumed from before the oldest event kept",
      Self::Failed => "the service failed",
    }
  }
}

/// One page of a list, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "{T}Page")]
pub struct Page<T> {
  pub items: Vec<T>,
  /// Whether anything stands past the last item.
  pub more: bool,
}

/// A page asked for by the last id seen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
  /// The last id seen; the page begins after it.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  /// How many items at most: 50 where none is given, at most 500.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// A page asked for by the last name seen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NameCursor {
  /// The last name seen; the page begins after it.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<String>,
  /// How many items at most: 50 where none is given, at most 500.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// Whether the service can answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Health {
  pub ready: bool,
  /// Why it is not ready, or null.
  pub reason: Option<String>,
  pub version: String,
  /// The numbers the brain reads by.
  pub rules: crate::brains::Rules,
}

/// One event: its number, its kind, when it happened and the member it is about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
  pub id: i64,
  /// `<resource>.<change>`.
  pub kind: String,
  pub at: i64,
  /// The member as a GET answers it, or its key alone for a deletion.
  pub data: Value,
}

/// Which events to read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
  /// Comma-separated kinds.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub kinds: Option<String>,
  /// The last event number seen.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
  /// Only the events of members held under this scope.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
}

/// A request previewed rather than carried out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DryRun {
  /// True answers what would happen and changes nothing.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dry_run: Option<bool>,
}

/// A field a merge patch may set, clear with null or leave out: absent is `None`, null `Some(None)`.
pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  Option::<T>::deserialize(deserializer).map(Some)
}
