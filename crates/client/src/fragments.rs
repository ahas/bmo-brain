//! Fragments, the libraries they are synced from, and the knowledge read over them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::memories::ScopedMemory;
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
  /// The library it was synced from.
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

/// A fragment written whole. `state` and `switchedOff`, where not given, keep what the fragment had.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentWrite {
  pub kind: String,
  pub text: String,
  /// Null or left out for a fragment shared by every scope.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub library_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<FragmentState>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub switched_off: Option<Vec<String>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub labels: Option<Labels>,
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

/// Fragments folded into one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentMerge {
  /// The fragments folded, in the order their texts join.
  pub keys: Vec<String>,
  /// The fragment kept: one of `keys`, or a new key.
  pub into: String,
  /// The kept fragment's text; the texts joined by a blank line where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
}

/// One piece a fragment is cut into; every field left out is the fragment's that is cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentPiece {
  pub key: String,
  pub text: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub labels: Option<Labels>,
}

/// A fragment cut into pieces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentSplit {
  pub key: String,
  pub pieces: Vec<FragmentPiece>,
}

/// That a reading attached fragments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentHits {
  pub keys: Vec<String>,
  pub actor_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
}

/// How many of the hits were recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentHitsRecorded {
  pub recorded_count: i64,
}

/// A kind of fragment and the words a reader calls it by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FragmentKind {
  pub name: String,
  /// Empty where none was given.
  pub description: String,
  pub fragment_count: i64,
}

/// A kind's words, as a library gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KindWords {
  pub name: String,
  pub description: String,
}

/// One document of a library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDocument {
  /// The key of the fragment a sync makes of it.
  pub key: String,
  pub kind: String,
  pub scope: Option<String>,
  pub subject_key: Option<String>,
  pub title: Option<String>,
  pub path: Option<String>,
  pub text: String,
  /// The SHA-256 of the text, in hex.
  pub version: String,
}

/// A named set of documents the caller keeps elsewhere, handed in whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Library {
  pub name: String,
  pub documents: Vec<LibraryDocument>,
  pub kinds: Vec<KindWords>,
  pub changed_at: i64,
}

/// One document as a library is put; the brain reckons its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDocumentPut {
  pub key: String,
  pub kind: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  pub text: String,
}

/// A library put whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPut {
  pub documents: Vec<LibraryDocumentPut>,
  #[serde(default)]
  pub kinds: Vec<KindWords>,
}

/// The library whose fragments to bring in line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySyncAsk {
  pub library_name: String,
}

/// How a fragment differs from its library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DriftChange {
  /// A document with no fragment.
  Added,
  /// A fragment whose text, version or place differs from its document.
  Changed,
  /// A fragment of the library whose document is gone.
  Removed,
  /// A fragment of the document's key that belongs to no library or to another.
  Claimed,
}

/// One fragment out of line with its library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Drift {
  pub key: String,
  pub change: DriftChange,
}

/// What a sync found, and whether it was applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySync {
  pub library_name: String,
  pub drift: Vec<Drift>,
  pub applied: bool,
}

/// What to read knowledge for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQuery {
  /// Comma-separated scopes.
  pub scopes: String,
  /// Comma-separated fragment keys.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub slugs: Option<String>,
  /// Comma-separated fragment kinds.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kinds: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject_key: Option<String>,
}

/// What a reader should know for a set of scopes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Knowledge {
  pub scopes: Vec<String>,
  /// The active fragments held under one of the scopes or shared, switched off for none of them.
  pub fragments: Vec<Fragment>,
  /// The standing rows held under one of the scopes.
  pub memories: Vec<ScopedMemory>,
}
