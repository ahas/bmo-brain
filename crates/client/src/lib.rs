//! The brain service's client (docs/boundary-api.md in the brain's repository): every body and member of its
//! API as a Rust type, and `BrainClient`, one method for each route. The service's OpenAPI document is
//! generated from these types.

mod brains;
mod client;
mod fragments;
mod memories;
mod wire;

pub use brains::{
  Brain, BrainsQuery, Confirmations, Consolidated, ExportQuery, Fold, Folded, Hits, Injected, Layer,
  LayerChange, LayersQuery, Learned, Learning, MemoriesQuery, Memory, MemoryMerge, MemoryMove,
  MemoryStatus, MemoryVersion, NewBrain, NewLayer, NewMemory, NewSubject, PersonaQuery, Recall, RecallAsk,
  RecallChoice, RecallPart, Recorded, Restoration, Rules, Subject, SubjectChange, SubjectKey, SubjectRef,
  SubjectsQuery, TalliesQuery, Tally, TallyBy, TypeCount,
};
pub use brains::{SimilarAsk, Source, UnconsolidatedQuery};
pub use client::{Answer, BrainClient, ClientFailure};
pub use fragments::{Fragment, FragmentKind, FragmentState, FragmentSwitch, FragmentsQuery};
pub use memories::{ScopedMemoriesQuery, ScopedMemory, ScopedMemoryChange, ScopedMemoryState};
/// The HTTP client a `BrainClient` calls through, for a caller that sets its own timeout or proxy.
pub use reqwest::Client as HttpClient;
pub use wire::{
  ConsoleManifest, ConsoleWords, Cursor, ErrorCode, Event, EventsQuery, Health, Labels, NameCursor, Object,
  Page, Role, WireError, nullable,
};
