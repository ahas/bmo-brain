//! Locks on the store, and what the brain holds under a scope.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Who is editing the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
  pub id: i64,
  pub holder_name: String,
  pub reason: String,
  pub created_at: i64,
  /// When it runs out, or null for one that stands until it is let go.
  pub expires_at: Option<i64>,
}

/// A lock to take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewLock {
  pub holder_name: String,
  pub reason: String,
  /// How long it stands; it stands until let go where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ttl_ms: Option<i64>,
}

/// Which locks to list, and how long to wait for none to stand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocksQuery {
  /// Answer once no lock stands, or once this many milliseconds have passed (at most ten minutes).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub wait_ms: Option<i64>,
  /// The last id seen.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// Whether a scope's rows wither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScopeState {
  Remembered,
  /// Its standing rows wither.
  Forgotten,
}

/// How many members of each resource stand under a scope.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCounts {
  pub memories: i64,
  pub memory_matches: i64,
  pub notes: i64,
  pub watches: i64,
  pub reminders: i64,
  pub fragments: i64,
  pub fragment_hits: i64,
}

impl ScopeCounts {
  /// Whether nothing stands under the scope.
  pub fn is_empty(&self) -> bool {
    *self == Self::default()
  }
}

/// What the brain holds under one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScopeHolding {
  pub scope: String,
  pub counts: ScopeCounts,
  pub state: ScopeState,
}

/// A scope marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScopeChange {
  pub state: ScopeState,
}
