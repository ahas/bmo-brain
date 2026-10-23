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
  Forbidden,
  NoRoute,
  NotFound,
  Conflict,
  Gone,
  Failed,
}

impl ErrorCode {
  /// Every code, in the order of their statuses.
  pub const ALL: [ErrorCode; 8] = [
    Self::BadRequest,
    Self::Unauthorized,
    Self::Forbidden,
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
      Self::Forbidden => "forbidden",
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
      Self::Forbidden => 403,
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
      Self::Forbidden => {
        "the console's token on a route the screen does not use, or without the role it needs"
      }
      Self::NoRoute => "no route answers this method and path",
      Self::NotFound => "nothing stands at the key the path names",
      Self::Conflict => "the member's state does not allow the change",
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

/// A field a merge patch may set, clear with null or leave out: absent is `None`, null `Some(None)`.
pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  Option::<T>::deserialize(deserializer).map(Some)
}

/// A role a reader holds, as bmo-ui's console bundle contract names it: an administrator holds every role a
/// member holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
  Administrator,
  Member,
}

impl Role {
  /// The role as the `Owner-Role` header and the manifest write it.
  pub fn name(self) -> &'static str {
    match self {
      Self::Administrator => "administrator",
      Self::Member => "member",
    }
  }

  /// The role a header names, or none where it names neither.
  pub fn named(name: &str) -> Option<Self> {
    [Self::Administrator, Self::Member].into_iter().find(|role| role.name() == name)
  }

  /// Whether a reader holding this role may open what needs `needed`.
  pub fn allows(self, needed: Self) -> bool {
    self == Self::Administrator || self == needed
  }
}

/// The console bundle's manifest, in the shape bmo-ui's `console-manifest` module states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleManifest {
  /// The bundle's name, in kebab-case: its words' namespace.
  pub name: String,
  /// The key, in the bundle's words, of the name a console's menu gives the screen.
  pub title: String,
  /// The icon drawn beside that name.
  pub icon: String,
  /// The path the bundle's screens stand under.
  pub screen_path: String,
  /// The service's name, which a console relays it under.
  pub service_name: String,
  /// The path the module is answered at.
  pub bundle: String,
  /// The path the stylesheet is answered at, where the bundle has one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub stylesheet: Option<String>,
  /// The path the words are answered at.
  pub words: String,
  /// The path every call the screen makes stands under.
  pub api_prefix: String,
  /// The least role a reader holds to open the screen.
  pub role: Role,
  /// Whether every call the screen makes needs the reader's owner key.
  pub owner_key: bool,
  /// Whether the screen reads the event stream through the console.
  pub events: bool,
  /// Whether the screen opens a WebSocket through the console.
  pub websocket: bool,
  /// The event kinds a console may raise as a notice to a reader not on the screen.
  pub push_kinds: Vec<String>,
  /// The other services the screen calls through the console, each by the service name its own manifest gives it.
  #[serde(default)]
  pub calls: Vec<String>,
}

/// The console bundle's words: each locale's tree under the locale's code.
pub type ConsoleWords = std::collections::BTreeMap<String, Value>;
