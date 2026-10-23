//! The client: one method for each route, behind the brain's token.

use reqwest::{Client, Method, RequestBuilder, StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::brains::{
  Brain, BrainsQuery, Confirmations, Consolidated, ExportQuery, Fold, Folded, Hits, Injected, Layer,
  LayerChange, LayersQuery, Learned, Learning, MemoriesQuery, Memory, MemoryMove, MemoryVersion, NewBrain,
  NewLayer, NewMemory, NewSubject, Recall, RecallAsk, Recorded, SimilarAsk, Subject, SubjectChange,
  SubjectsQuery, TypeCount, UnconsolidatedQuery,
};
use crate::fragments::{
  Fragment, FragmentHits, FragmentHitsRecorded, FragmentKind, FragmentMerge, FragmentSplit, FragmentSwitch,
  FragmentWrite, FragmentsQuery, Knowledge, KnowledgeQuery, Library, LibraryPut, LibrarySync,
  LibrarySyncAsk,
};
use crate::holdings::{Lock, LocksQuery, NewLock, ScopeChange, ScopeHolding};
use crate::memories::{
  NewNote, NewScopedMemory, NewScopedMemoryMatch, Note, NotesQuery, ScopedMemoriesQuery, ScopedMemory,
  ScopedMemoryChange, ScopedMemoryMatch,
};
use crate::records::{
  AgentAsk, Done, LoadsKeep, NoteKeep, RoomRules, RoomRulesAsk, RuleSet, SessionAsk, SessionLoads,
  SessionNote,
};
use crate::watches::{NewReminder, NewWatch, Reminder, RemindersQuery, Watch, WatchChange, WatchesQuery};
use crate::wire::{DryRun, Event, EventsQuery, Health, NameCursor, Page, WireError};

/// What a call came to short of the route's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFailure {
  /// The service answered with a failure: its status, and its error shape where it gave one.
  Answered {
    status: u16,
    answer: Option<WireError>,
  },
  /// Nothing answered.
  Unreachable(String),
  /// The service answered in a shape the route does not give.
  Unreadable(String),
}

impl std::fmt::Display for ClientFailure {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Answered {
        status,
        answer: Some(answer),
      } => write!(out, "the brain answered {status} {}: {}", answer.code, answer.error),
      Self::Answered {
        status,
        answer: None,
      } => write!(out, "the brain answered {status}"),
      Self::Unreachable(why) => write!(out, "the brain did not answer: {why}"),
      Self::Unreadable(why) => write!(out, "the brain's answer did not read: {why}"),
    }
  }
}

impl std::error::Error for ClientFailure {}

/// What a call answers.
pub type Answer<T> = Result<T, ClientFailure>;

/// The brain at one base address.
#[derive(Debug, Clone)]
pub struct BrainClient {
  http: Client,
  base: Url,
  token: String,
}

impl BrainClient {
  /// The brain whose routes stand at `base`, called with `token`; `None` where the address does not read.
  pub fn new(base: &str, token: impl Into<String>) -> Result<Self, String> {
    let base = Url::parse(base).map_err(|why| format!("{base} is no address: {why}"))?;
    if base.cannot_be_a_base() {
      return Err(format!("{base} is no base address"));
    }
    Ok(Self {
      http: Client::new(),
      base,
      token: token.into(),
    })
  }

  /// The same brain called through another HTTP client, such as one with a timeout of its own.
  pub fn with_http(mut self, http: Client) -> Self {
    self.http = http;
    self
  }

  fn url(&self, segments: &[&str]) -> Url {
    let mut url = self.base.clone();
    {
      let mut path = url.path_segments_mut().expect("the base address takes segments");
      path.pop_if_empty();
      path.extend(segments);
    }
    url
  }

  fn request(&self, method: Method, segments: &[&str]) -> RequestBuilder {
    self.http.request(method, self.url(segments)).bearer_auth(&self.token)
  }

  async fn answered<T: DeserializeOwned>(request: RequestBuilder) -> Answer<T> {
    let answer = Self::sent(request).await?;
    let bytes = answer.bytes().await.map_err(|why| ClientFailure::Unreadable(why.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|why| ClientFailure::Unreadable(why.to_string()))
  }

  async fn sent(request: RequestBuilder) -> Answer<reqwest::Response> {
    let answer = request.send().await.map_err(|why| ClientFailure::Unreachable(why.to_string()))?;
    let status = answer.status();
    if status.is_success() {
      return Ok(answer);
    }
    let body = answer.bytes().await.unwrap_or_default();
    Err(ClientFailure::Answered {
      status: status.as_u16(),
      answer: serde_json::from_slice(&body).ok(),
    })
  }

  async fn nothing(request: RequestBuilder) -> Answer<()> {
    let answer = Self::sent(request).await?;
    match answer.status() {
      StatusCode::NO_CONTENT => Ok(()),
      status => Err(ClientFailure::Unreadable(format!("{status} where 204 was due"))),
    }
  }

  fn get<Q: Serialize>(&self, segments: &[&str], query: &Q) -> RequestBuilder {
    self.request(Method::GET, segments).query(query)
  }

  fn send<B: Serialize>(&self, method: Method, segments: &[&str], body: &B) -> RequestBuilder {
    self.request(method, segments).json(body)
  }

  /// `GET /health`
  pub async fn health(&self) -> Answer<Health> {
    Self::answered(self.request(Method::GET, &["health"])).await
  }

  /// `GET /events`, as a page.
  pub async fn events(&self, query: &EventsQuery) -> Answer<Page<Event>> {
    let request = self.get(&["events"], query).header(reqwest::header::ACCEPT, "application/json");
    Self::answered(request).await
  }

  /// `GET /memories`
  pub async fn scoped_memories(&self, query: &ScopedMemoriesQuery) -> Answer<Page<ScopedMemory>> {
    Self::answered(self.get(&["memories"], query)).await
  }

  /// `POST /memories`
  pub async fn scoped_memory_add(&self, body: &NewScopedMemory) -> Answer<ScopedMemory> {
    Self::answered(self.send(Method::POST, &["memories"], body)).await
  }

  /// `GET /memories/{memoryId}`
  pub async fn scoped_memory(&self, memory_id: i64) -> Answer<ScopedMemory> {
    Self::answered(self.request(Method::GET, &["memories", &memory_id.to_string()])).await
  }

  /// `PATCH /memories/{memoryId}`
  pub async fn scoped_memory_update(
    &self,
    memory_id: i64,
    body: &ScopedMemoryChange,
  ) -> Answer<ScopedMemory> {
    Self::answered(self.send(Method::PATCH, &["memories", &memory_id.to_string()], body)).await
  }

  /// `POST /memory-matches`
  pub async fn scoped_memory_match_add(&self, body: &NewScopedMemoryMatch) -> Answer<ScopedMemoryMatch> {
    Self::answered(self.send(Method::POST, &["memory-matches"], body)).await
  }

  /// `GET /notes`
  pub async fn notes(&self, query: &NotesQuery) -> Answer<Page<Note>> {
    Self::answered(self.get(&["notes"], query)).await
  }

  /// `POST /notes`
  pub async fn note_add(&self, body: &NewNote) -> Answer<Note> {
    Self::answered(self.send(Method::POST, &["notes"], body)).await
  }

  /// `GET /watches`
  pub async fn watches(&self, query: &WatchesQuery) -> Answer<Page<Watch>> {
    Self::answered(self.get(&["watches"], query)).await
  }

  /// `POST /watches`
  pub async fn watch_add(&self, body: &NewWatch) -> Answer<Watch> {
    Self::answered(self.send(Method::POST, &["watches"], body)).await
  }

  /// `GET /watches/{watchId}`
  pub async fn watch(&self, watch_id: i64) -> Answer<Watch> {
    Self::answered(self.request(Method::GET, &["watches", &watch_id.to_string()])).await
  }

  /// `PATCH /watches/{watchId}`
  pub async fn watch_update(&self, watch_id: i64, body: &WatchChange) -> Answer<Watch> {
    Self::answered(self.send(Method::PATCH, &["watches", &watch_id.to_string()], body)).await
  }

  /// `GET /reminders`
  pub async fn reminders(&self, query: &RemindersQuery) -> Answer<Page<Reminder>> {
    Self::answered(self.get(&["reminders"], query)).await
  }

  /// `POST /reminders`
  pub async fn reminder_add(&self, body: &NewReminder) -> Answer<Reminder> {
    Self::answered(self.send(Method::POST, &["reminders"], body)).await
  }

  /// `DELETE /reminders/{reminderId}`
  pub async fn reminder_cancel(&self, reminder_id: i64) -> Answer<()> {
    Self::nothing(self.request(Method::DELETE, &["reminders", &reminder_id.to_string()])).await
  }

  /// `GET /fragments`
  pub async fn fragments(&self, query: &FragmentsQuery) -> Answer<Page<Fragment>> {
    Self::answered(self.get(&["fragments"], query)).await
  }

  /// `PUT /fragments/{fragmentKey}`
  pub async fn fragment_put(&self, fragment_key: &str, body: &FragmentWrite) -> Answer<Fragment> {
    Self::answered(self.send(Method::PUT, &["fragments", fragment_key], body)).await
  }

  /// `PATCH /fragments/{fragmentKey}`
  pub async fn fragment_switch(&self, fragment_key: &str, body: &FragmentSwitch) -> Answer<Fragment> {
    Self::answered(self.send(Method::PATCH, &["fragments", fragment_key], body)).await
  }

  /// `DELETE /fragments/{fragmentKey}`
  pub async fn fragment_remove(&self, fragment_key: &str) -> Answer<()> {
    Self::nothing(self.request(Method::DELETE, &["fragments", fragment_key])).await
  }

  /// `POST /fragment-merges`
  pub async fn fragment_merge(&self, body: &FragmentMerge) -> Answer<Fragment> {
    Self::answered(self.send(Method::POST, &["fragment-merges"], body)).await
  }

  /// `POST /fragment-splits`
  pub async fn fragment_split(&self, body: &FragmentSplit) -> Answer<Page<Fragment>> {
    Self::answered(self.send(Method::POST, &["fragment-splits"], body)).await
  }

  /// `POST /fragment-hits`
  pub async fn fragment_hits(&self, body: &FragmentHits) -> Answer<FragmentHitsRecorded> {
    Self::answered(self.send(Method::POST, &["fragment-hits"], body)).await
  }

  /// `GET /fragment-kinds`
  pub async fn fragment_kinds(&self, query: &NameCursor) -> Answer<Page<FragmentKind>> {
    Self::answered(self.get(&["fragment-kinds"], query)).await
  }

  /// `GET /libraries`
  pub async fn libraries(&self, query: &NameCursor) -> Answer<Page<Library>> {
    Self::answered(self.get(&["libraries"], query)).await
  }

  /// `GET /libraries/{libraryName}`
  pub async fn library(&self, library_name: &str) -> Answer<Library> {
    Self::answered(self.request(Method::GET, &["libraries", library_name])).await
  }

  /// `PUT /libraries/{libraryName}`
  pub async fn library_put(&self, library_name: &str, body: &LibraryPut) -> Answer<Library> {
    Self::answered(self.send(Method::PUT, &["libraries", library_name], body)).await
  }

  /// `POST /library-syncs`
  pub async fn library_sync(&self, body: &LibrarySyncAsk, dry_run: bool) -> Answer<LibrarySync> {
    let query = DryRun {
      dry_run: dry_run.then_some(true),
    };
    Self::answered(self.send(Method::POST, &["library-syncs"], body).query(&query)).await
  }

  /// `GET /knowledge`
  pub async fn knowledge(&self, query: &KnowledgeQuery) -> Answer<Knowledge> {
    Self::answered(self.get(&["knowledge"], query)).await
  }

  /// `GET /locks`
  pub async fn locks(&self, query: &LocksQuery) -> Answer<Page<Lock>> {
    Self::answered(self.get(&["locks"], query)).await
  }

  /// `POST /locks`
  pub async fn lock(&self, body: &NewLock) -> Answer<Lock> {
    Self::answered(self.send(Method::POST, &["locks"], body)).await
  }

  /// `DELETE /locks/{lockId}`
  pub async fn unlock(&self, lock_id: i64) -> Answer<()> {
    Self::nothing(self.request(Method::DELETE, &["locks", &lock_id.to_string()])).await
  }

  /// `GET /scopes/{scope}`
  pub async fn scope(&self, scope: &str) -> Answer<ScopeHolding> {
    Self::answered(self.request(Method::GET, &["scopes", scope])).await
  }

  /// `PATCH /scopes/{scope}`
  pub async fn scope_mark(&self, scope: &str, body: &ScopeChange) -> Answer<ScopeHolding> {
    Self::answered(self.send(Method::PATCH, &["scopes", scope], body)).await
  }

  /// `DELETE /scopes/{scope}`
  pub async fn scope_let_go(&self, scope: &str, dry_run: bool) -> Answer<ScopeHolding> {
    let query = DryRun {
      dry_run: dry_run.then_some(true),
    };
    Self::answered(self.request(Method::DELETE, &["scopes", scope]).query(&query)).await
  }

  /// `POST /room-rules/read`
  pub async fn room_rules(&self, ask: &RoomRulesAsk) -> Answer<RoomRules> {
    Self::answered(self.send(Method::POST, &["room-rules", "read"], ask)).await
  }

  /// `POST /room-rules/set`
  pub async fn room_rule_set(&self, set: &RuleSet) -> Answer<RoomRules> {
    Self::answered(self.send(Method::POST, &["room-rules", "set"], set)).await
  }

  /// `POST /sessions/loads`
  pub async fn session_loads(&self, ask: &SessionAsk) -> Answer<SessionLoads> {
    Self::answered(self.send(Method::POST, &["sessions", "loads"], ask)).await
  }

  /// `POST /sessions/loads/keep`
  pub async fn session_loads_keep(&self, keep: &LoadsKeep) -> Answer<Done> {
    Self::answered(self.send(Method::POST, &["sessions", "loads", "keep"], keep)).await
  }

  /// `POST /sessions/loads/forget`
  pub async fn session_loads_forget(&self, ask: &SessionAsk) -> Answer<Done> {
    Self::answered(self.send(Method::POST, &["sessions", "loads", "forget"], ask)).await
  }

  /// `POST /session-summary/read`
  pub async fn session_note(&self, ask: &AgentAsk) -> Answer<SessionNote> {
    Self::answered(self.send(Method::POST, &["session-summary", "read"], ask)).await
  }

  /// `POST /session-summary/keep`
  pub async fn session_note_keep(&self, keep: &NoteKeep) -> Answer<Done> {
    Self::answered(self.send(Method::POST, &["session-summary", "keep"], keep)).await
  }

  /// `POST /session-summary/clear`
  pub async fn session_note_clear(&self, ask: &AgentAsk) -> Answer<Done> {
    Self::answered(self.send(Method::POST, &["session-summary", "clear"], ask)).await
  }
}

impl BrainClient {
  async fn bytes(request: RequestBuilder) -> Answer<Vec<u8>> {
    let answer = Self::sent(request).await?;
    let bytes = answer.bytes().await.map_err(|why| ClientFailure::Unreadable(why.to_string()))?;
    Ok(bytes.to_vec())
  }

  async fn words(request: RequestBuilder) -> Answer<String> {
    let answer = Self::sent(request).await?;
    answer.text().await.map_err(|why| ClientFailure::Unreadable(why.to_string()))
  }

  /// `GET /brains`
  pub async fn brains(&self, query: &BrainsQuery) -> Answer<Page<Brain>> {
    Self::answered(self.get(&["brains"], query)).await
  }

  /// `POST /brains`
  pub async fn brain_add(&self, body: &NewBrain) -> Answer<Brain> {
    Self::answered(self.send(Method::POST, &["brains"], body)).await
  }

  /// `GET /brains/{brainId}`
  pub async fn brain(&self, brain_id: i64) -> Answer<Brain> {
    Self::answered(self.request(Method::GET, &["brains", &brain_id.to_string()])).await
  }

  /// `DELETE /brains/{brainId}`
  pub async fn brain_let_go(&self, brain_id: i64) -> Answer<Brain> {
    Self::answered(self.request(Method::DELETE, &["brains", &brain_id.to_string()])).await
  }

  /// `GET /brains/{brainId}/subjects`
  pub async fn subjects(&self, brain_id: i64, query: &SubjectsQuery) -> Answer<Page<Subject>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "subjects"], query)).await
  }

  /// `POST /brains/{brainId}/subjects`
  pub async fn subject_add(&self, brain_id: i64, body: &NewSubject) -> Answer<Subject> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "subjects"], body)).await
  }

  /// `GET /brains/{brainId}/subjects/{subjectId}`
  pub async fn subject(&self, brain_id: i64, subject_id: i64) -> Answer<Subject> {
    let path = ["brains", &brain_id.to_string(), "subjects", &subject_id.to_string()];
    Self::answered(self.request(Method::GET, &path)).await
  }

  /// `PATCH /brains/{brainId}/subjects/{subjectId}`
  pub async fn subject_change(
    &self,
    brain_id: i64,
    subject_id: i64,
    body: &SubjectChange,
  ) -> Answer<Subject> {
    let path = ["brains", &brain_id.to_string(), "subjects", &subject_id.to_string()];
    Self::answered(self.send(Method::PATCH, &path, body)).await
  }

  /// `GET /brains/{brainId}/layers`
  pub async fn layers(&self, brain_id: i64, query: &LayersQuery) -> Answer<Page<Layer>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "layers"], query)).await
  }

  /// `POST /brains/{brainId}/layers`
  pub async fn layer_add(&self, brain_id: i64, body: &NewLayer) -> Answer<Layer> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "layers"], body)).await
  }

  /// `GET /brains/{brainId}/layers/{layerId}`
  pub async fn layer(&self, brain_id: i64, layer_id: i64) -> Answer<Layer> {
    let path = ["brains", &brain_id.to_string(), "layers", &layer_id.to_string()];
    Self::answered(self.request(Method::GET, &path)).await
  }

  /// `PATCH /brains/{brainId}/layers/{layerId}`
  pub async fn layer_change(&self, brain_id: i64, layer_id: i64, body: &LayerChange) -> Answer<Layer> {
    let path = ["brains", &brain_id.to_string(), "layers", &layer_id.to_string()];
    Self::answered(self.send(Method::PATCH, &path, body)).await
  }

  /// `GET /brains/{brainId}/memories`
  pub async fn memories(&self, brain_id: i64, query: &MemoriesQuery) -> Answer<Page<Memory>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "memories"], query)).await
  }

  /// `POST /brains/{brainId}/memories`
  pub async fn memory_add(&self, brain_id: i64, body: &NewMemory) -> Answer<Memory> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "memories"], body)).await
  }

  /// `GET /brains/{brainId}/memories/{memoryId}`
  pub async fn memory(&self, brain_id: i64, memory_id: i64) -> Answer<Memory> {
    let path = ["brains", &brain_id.to_string(), "memories", &memory_id.to_string()];
    Self::answered(self.request(Method::GET, &path)).await
  }

  /// `POST /brains/{brainId}/memories/{memoryId}/versions`
  pub async fn memory_version(
    &self,
    brain_id: i64,
    memory_id: i64,
    body: &MemoryVersion,
  ) -> Answer<Memory> {
    let path = ["brains", &brain_id.to_string(), "memories", &memory_id.to_string(), "versions"];
    Self::answered(self.send(Method::POST, &path, body)).await
  }

  /// `PATCH /brains/{brainId}/memories/{memoryId}`
  pub async fn memory_move(&self, brain_id: i64, memory_id: i64, body: &MemoryMove) -> Answer<Memory> {
    let path = ["brains", &brain_id.to_string(), "memories", &memory_id.to_string()];
    Self::answered(self.send(Method::PATCH, &path, body)).await
  }

  /// `GET /brains/{brainId}/lineages/{lineageId}`
  pub async fn lineage(&self, brain_id: i64, lineage_id: i64) -> Answer<Vec<Memory>> {
    let path = ["brains", &brain_id.to_string(), "lineages", &lineage_id.to_string()];
    Self::answered(self.request(Method::GET, &path)).await
  }

  /// `POST /brains/{brainId}/hits`
  pub async fn hits_record(&self, brain_id: i64, body: &Hits) -> Answer<Recorded> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "hits"], body)).await
  }

  /// `POST /brains/{brainId}/confirmations`
  pub async fn confirmations_record(&self, brain_id: i64, body: &Confirmations) -> Answer<Recorded> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "confirmations"], body)).await
  }

  /// `GET /brains/{brainId}/types`
  pub async fn types(&self, brain_id: i64) -> Answer<Vec<TypeCount>> {
    Self::answered(self.request(Method::GET, &["brains", &brain_id.to_string(), "types"])).await
  }

  /// `POST /brains/{brainId}/recall`
  pub async fn recall(&self, brain_id: i64, body: &RecallAsk) -> Answer<Recall> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "recall"], body)).await
  }

  /// `POST /brains/{brainId}/folds`
  pub async fn fold(&self, brain_id: i64, body: &Fold) -> Answer<Folded> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "folds"], body)).await
  }

  /// `GET /brains/{brainId}/persona`
  pub async fn persona(&self, brain_id: i64) -> Answer<String> {
    Self::words(self.request(Method::GET, &["brains", &brain_id.to_string(), "persona"])).await
  }

  /// `GET /brains/{brainId}/export`
  pub async fn export(&self, brain_id: i64, query: &ExportQuery) -> Answer<Vec<u8>> {
    Self::bytes(self.get(&["brains", &brain_id.to_string(), "export"], query)).await
  }

  /// `POST /brains/{brainId}/injections`
  pub async fn inject(&self, brain_id: i64, file: Vec<u8>) -> Answer<Injected> {
    let request = self
      .request(Method::POST, &["brains", &brain_id.to_string(), "injections"])
      .header(reqwest::header::CONTENT_TYPE, "application/vnd.sqlite3")
      .body(file);
    Self::answered(request).await
  }

  /// `POST /brains/{brainId}/learnings`
  pub async fn learn(&self, brain_id: i64, body: &Learning) -> Answer<Learned> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "learnings"], body)).await
  }

  /// `POST /brains/{brainId}/similar`
  pub async fn similar(&self, brain_id: i64, body: &SimilarAsk) -> Answer<Vec<Memory>> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "similar"], body)).await
  }

  /// `GET /brains/{brainId}/unconsolidated`
  pub async fn unconsolidated(&self, brain_id: i64, query: &UnconsolidatedQuery) -> Answer<Vec<Memory>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "unconsolidated"], query)).await
  }

  /// `POST /brains/{brainId}/consolidated`
  pub async fn consolidated(&self, brain_id: i64, body: &Consolidated) -> Answer<Recorded> {
    Self::answered(self.send(Method::POST, &["brains", &brain_id.to_string(), "consolidated"], body)).await
  }

  /// `GET /discipline`
  pub async fn discipline(&self) -> Answer<String> {
    Self::words(self.request(Method::GET, &["discipline"])).await
  }
}
