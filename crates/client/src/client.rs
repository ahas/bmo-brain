//! The client: one method for each route, behind the brain's token.

use reqwest::{Client, Method, RequestBuilder, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::brains::{
  Brain, BrainsQuery, Confirmations, Consolidated, ConsolidationState, ExportQuery, Fold, Folded, Hits,
  Injected, Layer, LayerChange, LayersQuery, Learned, Learning, MemoriesQuery, Memory, MemoryMerge,
  MemoryMove, MemoryVersion, MissesQuery, NewBrain, NewLayer, NewMemory, NewSubject, OrgTopic,
  OrgTopicsQuery, PersonaQuery, Recall, RecallAsk, Recorded, Restoration, SearchMiss, SheetQuery,
  SimilarAsk, Subject, SubjectChange, SubjectsQuery, TalliesQuery, Tally, TypeCount, UnconsolidatedEpisode,
  UnconsolidatedQuery,
};
use crate::wire::{Event, EventsQuery, Health, Page, WireError};

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

  /// `GET /org-topics`
  pub async fn org_topics(&self, query: &OrgTopicsQuery) -> Answer<Vec<OrgTopic>> {
    Self::answered(self.get(&["org-topics"], query)).await
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

  /// `GET /brains/{brainId}/subjects/{subjectId}/sheet`
  pub async fn sheet(&self, brain_id: i64, subject_id: i64, query: &SheetQuery) -> Answer<String> {
    let path = ["brains", &brain_id.to_string(), "subjects", &subject_id.to_string(), "sheet"];
    Self::words(self.get(&path, query)).await
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

  /// `POST /brains/{brainId}/merges`
  pub async fn memory_merge(&self, brain_id: i64, body: &MemoryMerge) -> Answer<Memory> {
    let path = ["brains", &brain_id.to_string(), "merges"];
    Self::answered(self.send(Method::POST, &path, body)).await
  }

  /// `PATCH /brains/{brainId}/memories/{memoryId}`
  pub async fn memory_move(&self, brain_id: i64, memory_id: i64, body: &MemoryMove) -> Answer<Memory> {
    let path = ["brains", &brain_id.to_string(), "memories", &memory_id.to_string()];
    Self::answered(self.send(Method::PATCH, &path, body)).await
  }

  /// `POST /brains/{brainId}/memories/{memoryId}/restorations`
  pub async fn memory_restore(&self, brain_id: i64, memory_id: i64, body: &Restoration) -> Answer<Memory> {
    let path = ["brains", &brain_id.to_string(), "memories", &memory_id.to_string(), "restorations"];
    Self::answered(self.send(Method::POST, &path, body)).await
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

  /// `GET /brains/{brainId}/tallies`
  pub async fn tallies(&self, brain_id: i64, query: &TalliesQuery) -> Answer<Vec<Tally>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "tallies"], query)).await
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
  pub async fn persona(&self, brain_id: i64, query: &PersonaQuery) -> Answer<String> {
    Self::words(self.get(&["brains", &brain_id.to_string(), "persona"], query)).await
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
  pub async fn unconsolidated(
    &self,
    brain_id: i64,
    query: &UnconsolidatedQuery,
  ) -> Answer<Vec<UnconsolidatedEpisode>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "unconsolidated"], query)).await
  }

  /// `GET /brains/{brainId}/consolidation`
  pub async fn consolidation(&self, brain_id: i64) -> Answer<ConsolidationState> {
    Self::answered(self.request(Method::GET, &["brains", &brain_id.to_string(), "consolidation"])).await
  }

  /// `GET /brains/{brainId}/misses`
  pub async fn misses(&self, brain_id: i64, query: &MissesQuery) -> Answer<Page<SearchMiss>> {
    Self::answered(self.get(&["brains", &brain_id.to_string(), "misses"], query)).await
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
