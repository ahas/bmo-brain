//! The brains and their three tables (docs/design.md in the brain's repository): a brain, its subjects, its
//! layers, its memories in their versions, the hits and confirmations recorded on them, a turn's recall, a
//! session's fold, and what an injection comes to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::wire::Labels;

/// One agent's long-term memory, keyed by the account that owns it and the agent it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Brain {
  pub id: i64,
  /// The account's opaque key, as the caller gives it.
  pub account_key: String,
  /// The agent's opaque key, as the caller gives it.
  pub agent_key: String,
  pub name: String,
  pub created_at: i64,
}

/// A brain to make.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewBrain {
  pub account_key: String,
  pub agent_key: String,
  /// The agent's key where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
}

/// Which brains to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainsQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub account_key: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub agent_key: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// Something a memory can be about, found by its type and key alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
  pub id: i64,
  pub brain_id: i64,
  /// The caller's word for what it is: a person, a room, a document, a process.
  #[serde(rename = "type")]
  pub kind: String,
  /// The caller's identifier for it, unique within its brain and type.
  pub key: String,
  pub description: String,
  /// The text of its newest active `name` memory, or null where it has none.
  #[serde(default)]
  pub name: Option<String>,
  pub created_at: i64,
}

/// A subject named by its type and key, made where it does not stand yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubjectKey {
  #[serde(rename = "type")]
  pub kind: String,
  pub key: String,
}

/// A subject to add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewSubject {
  #[serde(rename = "type")]
  pub kind: String,
  pub key: String,
  #[serde(default)]
  pub description: String,
}

/// A change to a subject's description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubjectChange {
  pub description: String,
}

/// Which subjects to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubjectsQuery {
  #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  /// Words the description holds, in any case.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub q: Option<String>,
  /// The subjects these ids name, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ids: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// A heading in a brain's table of contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
  pub id: i64,
  pub brain_id: i64,
  pub name: String,
  pub description: String,
  /// Where it stands among the brain's layers; the persona is rendered in this order.
  pub position: i64,
  pub created_at: i64,
}

/// A layer to add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewLayer {
  pub name: String,
  #[serde(default)]
  pub description: String,
  /// After the last layer where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub position: Option<i64>,
}

/// A change to a layer: only the fields given change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayerChange {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub position: Option<i64>,
}

/// Which layers to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayersQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// The state of one version of a memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryStatus {
  /// The memory holds.
  Active,
  /// It waits for a person's approval.
  Pending,
  /// What it asked is finished.
  Done,
  /// A later version replaced it.
  Superseded,
  /// It stopped holding.
  Invalidated,
}

impl MemoryStatus {
  pub const ALL: [MemoryStatus; 5] =
    [Self::Active, Self::Pending, Self::Done, Self::Superseded, Self::Invalidated];

  /// The status as it crosses the wire.
  pub fn name(self) -> &'static str {
    match self {
      Self::Active => "active",
      Self::Pending => "pending",
      Self::Done => "done",
      Self::Superseded => "superseded",
      Self::Invalidated => "invalidated",
    }
  }

  /// The status a wire word names.
  pub fn named(name: &str) -> Option<Self> {
    Self::ALL.into_iter().find(|status| status.name() == name)
  }

  /// Whether a move may make this status a row's next one by a caller's word: `active` from `pending`,
  /// `done` and `invalidated` from `active` or `pending`. `superseded` is only ever set by a new version.
  pub fn may_move_to(self, next: Self) -> bool {
    matches!(
      (self, next),
      (Self::Pending, Self::Active)
        | (Self::Active | Self::Pending, Self::Done)
        | (Self::Active | Self::Pending, Self::Invalidated)
    )
  }

  /// Whether a row in this status is live: it may be handed out, and it holds its key.
  pub fn is_live(self) -> bool {
    matches!(self, Self::Active | Self::Pending)
  }
}

/// A subject a memory is about, as the memory lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubjectRef {
  pub id: i64,
  #[serde(rename = "type")]
  pub kind: String,
  pub key: String,
}

/// One memory in one version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
  pub id: i64,
  pub brain_id: i64,
  /// The caller's identifier, unique among the live rows of a brain.
  pub key: Option<String>,
  /// What the row is; the reserved types are stated in the design.
  #[serde(rename = "type")]
  pub kind: String,
  pub text: String,
  pub layer_id: Option<i64>,
  /// Where it stands within its layer.
  pub position: i64,
  pub subjects: Vec<SubjectRef>,
  pub status: MemoryStatus,
  /// The first version's id in this row's line.
  pub lineage_id: i64,
  pub superseded_by: Option<i64>,
  pub valid_from: i64,
  pub valid_to: Option<i64>,
  /// The situation in which it is needed, in words, or null where it carries none.
  #[serde(default)]
  pub recall_condition: Option<String>,
  /// The situation that ends it, in words, or null where it carries none.
  #[serde(default)]
  pub release_condition: Option<String>,
  /// The writer's weight at the time of writing, 1 to 10.
  pub importance: i64,
  /// How often it was used: handed out, reported used, confirmed or read explicitly.
  pub use_count: i64,
  /// Its latest use; null where it was never used.
  pub last_used_at: Option<i64>,
  pub last_confirmed_at: Option<i64>,
  pub consolidated_at: Option<i64>,
  /// Who wrote this version.
  pub actor_name: String,
  /// Why its status was moved, as the mover said; null where nothing was said.
  #[serde(default)]
  pub reason: Option<String>,
  pub labels: Labels,
  pub created_at: i64,
}

/// A memory to write as a first version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewMemory {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(rename = "type")]
  pub kind: String,
  pub text: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub layer_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub position: Option<i64>,
  /// The subjects it is about, by id.
  #[serde(default)]
  pub subject_ids: Vec<i64>,
  /// The subjects it is about, by type and key, each made where it does not stand yet.
  #[serde(default)]
  pub subjects: Vec<SubjectKey>,
  /// 1 to 10; 5 where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub importance: Option<i64>,
  /// When it began to hold; now where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub valid_from: Option<i64>,
  /// The situation in which it is needed, in words.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub recall_condition: Option<String>,
  /// The situation that ends it, in words.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub release_condition: Option<String>,
  /// `active` where none is given; `pending` is the other status a first version takes.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub status: Option<MemoryStatus>,
  pub actor_name: String,
  #[serde(default)]
  pub labels: Labels,
}

/// A new version of a memory, superseding the version it is written over: only the fields given differ.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryVersion {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub layer_id: Option<Option<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub position: Option<i64>,
  /// The subjects the new version is about, replacing the old version's whole where given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject_ids: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subjects: Option<Vec<SubjectKey>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub importance: Option<i64>,
  /// When the new version began to hold, and the old one stopped; now where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub valid_from: Option<i64>,
  /// The situation in which it is needed: the old version's where not given, and a null clears it.
  #[serde(
    default,
    deserialize_with = "crate::wire::nullable",
    skip_serializing_if = "Option::is_none"
  )]
  pub recall_condition: Option<Option<String>>,
  /// The situation that ends it: the old version's where not given, and a null clears it.
  #[serde(
    default,
    deserialize_with = "crate::wire::nullable",
    skip_serializing_if = "Option::is_none"
  )]
  pub release_condition: Option<Option<String>>,
  pub actor_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub labels: Option<Labels>,
}

/// One row written over several live rows about the same thing: the new row starts a lineage, and each
/// merged row is superseded by it. Where no subjects are given, the new row is about the union of theirs;
/// where no importance is given, the highest of theirs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMerge {
  /// The live rows merged, two or more.
  pub memory_ids: Vec<i64>,
  #[serde(rename = "type")]
  pub kind: String,
  pub text: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub layer_id: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub position: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject_ids: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subjects: Option<Vec<SubjectKey>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub importance: Option<i64>,
  /// When the merged row began to hold, and the merged ones stopped; now where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub valid_from: Option<i64>,
  /// The situation in which it is needed; the one every merged row shares where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub recall_condition: Option<String>,
  /// The situation that ends it; the one every merged row shares where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub release_condition: Option<String>,
  pub actor_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub labels: Option<Labels>,
}

/// A move of a memory's status by a caller's word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMove {
  pub status: MemoryStatus,
  /// When it stopped holding, for `done` and `invalidated`; now where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<i64>,
  pub actor_name: String,
  /// Why it moves, kept on the row moved.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
  /// For a move to `done`: the words it ended on, written as a new version that is then moved.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
}

/// A closed lineage restored: a new active version written after its newest, closed one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Restoration {
  /// The closed row's text where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  pub actor_name: String,
}

/// Which memories to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoriesQuery {
  /// One type, or several, comma-separated.
  #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  /// One status, or several, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub status: Option<String>,
  /// Types left out, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exclude_types: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub layer_id: Option<i64>,
  /// True for the rows in no layer.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub unfiled: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subject_id: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub lineage_id: Option<i64>,
  /// The rows these ids name, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ids: Option<String>,
  /// The rows superseded by these, comma-separated ids.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub superseded_by: Option<String>,
  /// `key` for rows whose label is set, `key=value` for rows whose label has that value.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  /// Words the text holds, in any case.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub q: Option<String>,
  /// Words the recall condition holds, in any case: the rows a situation calls for.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub situation: Option<String>,
  /// Whether a read by key, subject, type or words records a use of each live row it answers; true where none
  /// is given.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub record: Option<bool>,
  /// The field the list is ordered by: `id`, `position`, `validFrom`, `importance` or `useCount`, descending
  /// with a leading `-`; `id` where none is given.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub order: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// What memories are grouped by in a tally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TallyBy {
  Status,
  Type,
  Layer,
  Subject,
}

/// Which memories to tally, under the filters the memory list takes, and what to group them by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TalliesQuery {
  pub by: TallyBy,
  /// One type, or several, comma-separated.
  #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  /// One status, or several, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub status: Option<String>,
  /// Types left out, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exclude_types: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub layer_id: Option<i64>,
  /// True for the rows in no layer.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub unfiled: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subject_id: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub lineage_id: Option<i64>,
  /// The rows these ids name, comma-separated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ids: Option<String>,
  /// The rows superseded by these, comma-separated ids.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub superseded_by: Option<String>,
  /// `key` for rows whose label is set, `key=value` for rows whose label has that value.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub label: Option<String>,
  /// Words the text holds, in any case.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub q: Option<String>,
  /// Words the recall condition holds, in any case: the rows a situation calls for.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub situation: Option<String>,
}

/// One group of a tally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
  /// The status, the type, or the layer's or subject's id in decimal; null for the rows in no layer or about
  /// no subject.
  pub group: Option<String>,
  pub count: i64,
  /// The group's mean importance.
  pub importance: f64,
  /// The group's mean use count.
  pub use_count: f64,
  /// The latest use among the group's rows; null where none was used.
  pub last_used_at: Option<i64>,
  /// The latest `validFrom` among the group's rows.
  pub latest_valid_from: i64,
}

/// A reading that handed memories out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hits {
  pub memory_ids: Vec<i64>,
  /// The session the memories were handed to, which marks them for that session.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session_key: Option<String>,
  pub actor_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<i64>,
}

/// A source apart from a reading that confirmed memories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Confirmations {
  pub memory_ids: Vec<i64>,
  pub actor_name: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<i64>,
}

/// How many rows a recording reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Recorded {
  pub recorded_count: i64,
}

/// A type in use in a brain, and how many rows carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeCount {
  #[serde(rename = "type")]
  pub kind: String,
  /// Its live memories, `active` and `pending`.
  pub count: i64,
  /// Whether code depends on the type's meaning.
  pub reserved: bool,
}

/// A turn, as recall reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecallAsk {
  /// The room the turn is in.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room: Option<SubjectKey>,
  /// Everyone who spoke this turn.
  #[serde(default)]
  pub speakers: Vec<SubjectKey>,
  /// The words of the turn.
  #[serde(default)]
  pub words: String,
  /// The session the block goes to, whose marks and opening moment narrow it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session_key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session_opened_at: Option<i64>,
  /// The brain's own budget where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub budget_tokens: Option<i64>,
  /// Whether the hand-out is recorded as uses and marks; true where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub record: Option<bool>,
}

/// Which part of the block a memory was chosen for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RecallPart {
  /// Stands on every turn of the room.
  Core,
  /// Chosen by score.
  Searched,
}

/// One memory recall chose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecallChoice {
  pub memory_id: i64,
  #[serde(rename = "type")]
  pub kind: String,
  pub part: RecallPart,
  pub score: f64,
  /// Handed to this session before in this version, so the block carries its mark and not its text.
  pub marked: bool,
  pub token_count: i64,
}

/// What a turn is handed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Recall {
  pub block: String,
  pub chosen: Vec<RecallChoice>,
  pub token_count: i64,
  pub budget_tokens: i64,
  /// The sum of the importance of the brain's unconsolidated episodes.
  pub unconsolidated_importance: i64,
  /// Whether that sum has reached the brain's threshold, so a consolidation is due.
  pub consolidation_due: bool,
  /// The rows whose release condition this turn held, ended as invalidated by this recall.
  #[serde(default)]
  pub released: Vec<i64>,
}

/// A session folded or handed off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fold {
  pub session_key: String,
}

/// What a fold came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Folded {
  /// The marks cleared for the session.
  pub cleared_count: i64,
}

/// Which layers an export takes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportQuery {
  /// Comma-separated layer names; the whole brain where none is given.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub layers: Option<String>,
}

/// Which layers a persona rendering takes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonaQuery {
  /// Comma-separated layer names; every persona piece where none is given.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub layers: Option<String>,
}

/// Which room a subject's sheet takes in, and how many of its lines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SheetQuery {
  /// A subject of type `room`, whose facts about the subject the sheet takes in beside those about no room.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub room_id: Option<i64>,
  /// The first that many lines alone, at least one; the whole sheet where none is given.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// What an injection came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Injected {
  pub subjects_added: i64,
  pub layers_added: i64,
  pub memories_added: i64,
  pub memories_superseded: i64,
}

/// The rules, key by key as the configuration writes them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "snake_case")]
pub struct Rules {
  /// The tokens a recall block holds where the ask names no budget.
  pub budget_tokens: i64,
  /// The weight `λ` recall's selection gives a row's relevance against its overlap with the rows already in.
  pub mmr_lambda: f64,
  /// The most rows one subject fills in the searched part of a block.
  pub subject_cap: i64,
  /// The most facts of one speaker's sheet the core of a block carries; the rest reach a turn only through
  /// the searched part.
  pub core_fact_cap: i64,
  /// How many moments of use a row keeps, its newest.
  pub hit_times_kept: i64,
  /// The span of one session, in milliseconds; read by no rule.
  pub same_session_ms: i64,
  /// For how long after a version was superseded a reading says what the previous value was.
  pub previous_value_days: i64,
  /// The share of a condition's words the turn must hold for the condition to match it.
  pub condition_share: f64,
  /// The sum of importance of unconsolidated episodes that triggers a consolidation.
  pub consolidation_threshold: i64,
}

impl Default for Rules {
  fn default() -> Self {
    Self {
      budget_tokens: 1_800,
      mmr_lambda: 0.7,
      subject_cap: 4,
      core_fact_cap: 8,
      hit_times_kept: 64,
      same_session_ms: 60 * 60 * 1_000,
      previous_value_days: 30,
      condition_share: 0.5,
      consolidation_threshold: 150,
    }
  }
}

/// Where a learning came from, which decides whether it stands as a fact on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
  /// The person said it about themselves.
  Own,
  /// The owner stated or instructed it.
  Owner,
  /// Someone said it about someone else.
  Told,
  /// The agent concluded it.
  Inferred,
}

impl Source {
  /// Whether a learning from this source is heard rather than known: written as a rumor, closing nothing.
  pub fn is_heard(self) -> bool {
    matches!(self, Self::Told | Self::Inferred)
  }
}

/// One thing the agent learned, with its judgement against the live memories like it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Learning {
  #[serde(rename = "type")]
  pub kind: String,
  pub text: String,
  #[serde(default)]
  pub subjects: Vec<SubjectKey>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub importance: Option<i64>,
  pub source: Source,
  /// Live memories this merely repeats: confirmed, and nothing written where nothing else is judged.
  #[serde(default)]
  pub confirms: Vec<i64>,
  /// The live memory this says more fully: a new version of it is written.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub strengthens: Option<i64>,
  /// Live memories this contradicts: closed before the new row is written, unless the source is heard.
  #[serde(default)]
  pub replaces: Vec<i64>,
  /// The episodes this rests on, linked as subjects of type `episode`.
  #[serde(default)]
  pub grounds: Vec<i64>,
  /// When it began holding; now where none is given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub valid_from: Option<i64>,
  /// The situation in which it is needed, in words.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub recall_condition: Option<String>,
  /// The situation that ends it, in words.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub release_condition: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub layer_id: Option<i64>,
  pub actor_name: String,
  #[serde(default)]
  pub labels: Labels,
}

/// What a learning came to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Learned {
  pub confirmed_count: i64,
  pub invalidated_ids: Vec<i64>,
  /// The version written, as a first version or over the memory strengthened; null where only confirmed.
  pub written: Option<Memory>,
  /// Whether the row written is a new version of an existing memory.
  pub versioned: bool,
}

/// A candidate memory, whose likes the write judgement reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarAsk {
  pub text: String,
  #[serde(default)]
  pub subjects: Vec<SubjectKey>,
  /// Ten where none is given, at most fifty.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// Which episodes a consolidation reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnconsolidatedQuery {
  /// Sixty where none is given, at most five hundred.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
}

/// Episodes a consolidation read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Consolidated {
  pub episode_ids: Vec<i64>,
}
