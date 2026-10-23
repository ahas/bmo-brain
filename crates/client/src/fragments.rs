//! Fragments, held for the console's screens, and their kinds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::wire::Labels;

/// Whether a fragment is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FragmentState {
  Active,
  Off,
}

impl FragmentState {
  /// The state as the wire writes it.
  pub fn name(self) -> &'static str {
    match self {
      Self::Active => "active",
      Self::Off => "off",
    }
  }

  /// The state of that name.
  pub fn named(name: &str) -> Option<Self> {
    [Self::Active, Self::Off].into_iter().find(|state| state.name() == name)
  }
}

/// One piece of reference text, named by its key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
  pub id: i64,
  pub key: String,
  pub kind: String,
  /// The scope it is held under, or null for one shared by every scope.
  pub scope: Option<String>,
  pub subject_key: Option<String>,
  pub title: Option<String>,
  /// Where the caller keeps it.
  pub path: Option<String>,
  pub text: String,
  /// The SHA-256 of the text, in hex.
  pub version: String,
  /// The caller's name for the set it came in.
  pub library_name: Option<String>,
  pub state: FragmentState,
  /// The scopes it is switched off for.
  pub switched_off: Vec<String>,
  pub labels: Labels,
  /// How many readings have attached it.
  pub hit_count: i64,
  pub last_hit_at: Option<i64>,
  pub created_at: i64,
  pub changed_at: i64,
}

/// Which fragments to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentsQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subject_key: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub library_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub state: Option<FragmentState>,
  /// Words the title or the text holds, in any case.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub q: Option<String>,
  /// Comma-separated ids.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ids: Option<String>,
  /// Comma-separated keys.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub keys: Option<String>,
  /// The last id seen.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// A fragment switched: only the fields given change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentSwitch {
  /// The scopes it is switched off for, replaced whole.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub switched_off: Option<Vec<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<FragmentState>,
}

/// A kind of fragment and the words a reader calls it by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentKind {
  pub name: String,
  /// Empty where the store holds none.
  pub description: String,
  pub fragment_count: i64,
}
