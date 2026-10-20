//! Watches and reminders: what the brain's clock turns into events.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::wire::{Labels, Object};

/// Where a watch stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WatchState {
  /// Waiting on its condition.
  Armed,
  /// Its condition held.
  Fired,
  /// A holder is acting on it.
  Claimed,
  /// Acted on.
  Ended,
  /// Given up.
  Cancelled,
}

impl WatchState {
  /// The state as the wire writes it.
  pub fn name(self) -> &'static str {
    match self {
      Self::Armed => "armed",
      Self::Fired => "fired",
      Self::Claimed => "claimed",
      Self::Ended => "ended",
      Self::Cancelled => "cancelled",
    }
  }

  /// The state of that name.
  pub fn named(name: &str) -> Option<Self> {
    [Self::Armed, Self::Fired, Self::Claimed, Self::Ended, Self::Cancelled]
      .into_iter()
      .find(|state| state.name() == name)
  }

  /// Whether a watch in this state may move to `next`.
  pub fn may_move_to(self, next: Self) -> bool {
    use WatchState::*;
    matches!(
      (self, next),
      (Armed, Fired | Cancelled) | (Fired, Claimed | Ended | Cancelled) | (Claimed, Ended | Cancelled)
    )
  }
}

/// A condition waited on, and fired once it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Watch {
  pub id: i64,
  pub scope: String,
  /// `kind` is `at` (with `at`, a moment), `memory` (with `memoryId` and the row `state` it waits for) or
  /// `external` (with `key`, fired by a client); any other field is kept as given.
  pub condition: Object,
  pub state: WatchState,
  pub setter_name: String,
  pub holder_name: Option<String>,
  pub labels: Labels,
  pub fired_at: Option<i64>,
  pub created_at: i64,
  pub changed_at: i64,
}

/// Which watches to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WatchesQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub setter_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub holder_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub state: Option<WatchState>,
  /// The last id seen.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// A watch to set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewWatch {
  pub scope: String,
  /// `{"kind": "at", "at": <moment>}`, `{"kind": "memory", "memoryId": <row>, "state": <row state>}` or
  /// `{"kind": "external", "key": <string>}`.
  pub condition: Object,
  pub setter_name: String,
  pub labels: Labels,
}

/// A watch moved on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WatchChange {
  pub state: WatchState,
  /// The holder claiming it; required with `claimed`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub holder_name: Option<String>,
}

/// A moment to be told of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
  pub id: i64,
  pub scope: String,
  pub due_at: i64,
  pub text: String,
  pub labels: Labels,
  pub setter_name: String,
  pub created_at: i64,
  /// When `reminder.due` was published, or null while it waits.
  pub fired_at: Option<i64>,
}

/// Which reminders to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemindersQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  /// Reminders due at or before this moment.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub due_before: Option<i64>,
  /// True for the ones fired, false for the ones still waiting.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub fired: Option<bool>,
  /// The last id seen.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// A reminder to set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewReminder {
  pub scope: String,
  pub due_at: i64,
  pub text: String,
  pub setter_name: String,
  pub labels: Labels,
}
