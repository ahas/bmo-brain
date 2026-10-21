//! The brain service's client (docs/boundary-api.md in the brain's repository): every body and member of its
//! API as a Rust type, and `BrainClient`, one method for each route. The service's OpenAPI document is
//! generated from these types.

mod client;
mod fragments;
mod holdings;
mod memories;
mod records;
mod watches;
mod wire;

pub use client::{Answer, BrainClient, ClientFailure};
pub use fragments::{
  Drift, DriftChange, Fragment, FragmentHits, FragmentHitsRecorded, FragmentKind, FragmentMerge,
  FragmentPiece, FragmentSplit, FragmentState, FragmentSwitch, FragmentWrite, FragmentsQuery, KindWords,
  Knowledge, KnowledgeQuery, Library, LibraryDocument, LibraryDocumentPut, LibraryPut, LibrarySync,
  LibrarySyncAsk,
};
pub use holdings::{Lock, LocksQuery, NewLock, ScopeChange, ScopeCounts, ScopeHolding, ScopeState};
pub use memories::{
  MemoriesQuery, Memory, MemoryChange, MemoryMatch, MemoryState, NewMemory, NewMemoryMatch, NewNote, Note,
  NotesQuery,
};
pub use records::{
  AgentAsk, Done, Load, LoadsKeep, NoteKeep, RoomRule, RoomRules, RoomRulesAsk, RuleSet, RuleText,
  SessionAsk, SessionLoads, SessionNote,
};
/// The HTTP client a `BrainClient` calls through, for a caller that sets its own timeout or proxy.
pub use reqwest::Client as HttpClient;
pub use watches::{
  NewReminder, NewWatch, Reminder, RemindersQuery, Watch, WatchChange, WatchState, WatchesQuery,
};
pub use wire::{
  Cursor, DryRun, ErrorCode, Event, EventsQuery, Health, Labels, NameCursor, Object, Page, WireError,
  nullable,
};
