//! Memory rows, held under a caller's scope for the console's screens.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::wire::{Labels, Object, nullable};

/// Where a row stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScopedMemoryState {
  /// The row holds.
  Standing,
  /// What it asked is finished.
  Done,
  /// It is no longer of use.
  Retired,
  /// It waits for someone's approval.
  Candidate,
  /// It is set aside.
  Off,
}

impl ScopedMemoryState {
  /// The state as the wire writes it.
  pub fn name(self) -> &'static str {
    match self {
      Self::Standing => "standing",
      Self::Done => "done",
      Self::Retired => "retired",
      Self::Candidate => "candidate",
      Self::Off => "off",
    }
  }

  /// The state of that name.
  pub fn named(name: &str) -> Option<Self> {
    [Self::Standing, Self::Done, Self::Retired, Self::Candidate, Self::Off]
      .into_iter()
      .find(|state| state.name() == name)
  }

  /// Whether a row in this state may move to `next`; moving to the state it is in changes nothing.
  pub fn may_move_to(self, next: Self) -> bool {
    use ScopedMemoryState::*;
    self == next
      || matches!(
        (self, next),
        (Standing, Done | Retired | Candidate | Off)
          | (Done, Standing | Retired)
          | (Retired, Standing)
          | (Candidate, Standing | Retired)
          | (Off, Standing | Retired)
      )
  }
}

/// One thing to remember.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScopedMemory {
  pub id: i64,
  pub scope: String,
  pub subject_key: Option<String>,
  /// The caller's word for what the row is.
  pub kind: String,
  pub text: String,
  pub state: ScopedMemoryState,
  /// The caller's objects, kept as given.
  pub conditions: Vec<Object>,
  pub labels: Labels,
  pub due_at: Option<i64>,
  /// How many readings have matched it.
  pub match_count: i64,
  pub last_matched_at: Option<i64>,
  pub created_at: i64,
  pub changed_at: i64,
}

/// Which rows to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScopedMemoriesQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  /// Rows of every scope beginning with this.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope_prefix: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subject_key: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub state: Option<ScopedMemoryState>,
  /// `key` for rows whose label is set, `key=value` for rows whose label has that value.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  /// Rows due at or before this moment.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub due_before: Option<i64>,
  /// Words the text holds, in any case.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub q: Option<String>,
  /// The last id seen.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// A change to a row: only the fields given change.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScopedMemoryChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<ScopedMemoryState>,
  /// The labels, replaced whole.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub labels: Option<Labels>,
  /// The moment it is due; null clears it.
  #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
  pub due_at: Option<Option<i64>>,
}
