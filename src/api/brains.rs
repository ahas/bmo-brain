//! The routes of the brains and their three tables (docs/boundary-api.md, 「Brains」), each one call of the
//! store.

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use bmo_brain_client::{
  Brain as BrainMember, BrainsQuery, Confirmations, Consolidated, ConsolidationQuery, ConsolidationState,
  DuplicateCandidate, DuplicatesQuery, ExportQuery, Fold, Folded, Hits, Injected, Layer, LayerChange,
  LayersQuery, Learned, Learning, MemoriesQuery, Memory, MemoryMerge, MemoryMove, MemoryVersion,
  MissesQuery, NewBrain, NewLayer, NewMemory, NewSubject, OrgTopic, OrgTopicsQuery, Page, PersonaQuery,
  Recall, RecallAsk, Recorded, Restoration, SearchMiss, SheetQuery, SimilarAsk, Subject, SubjectChange,
  SubjectsQuery, TalliesQuery, Tally, TypeCount, UnconsolidatedEpisode, UnconsolidatedQuery,
};

use super::{Answer, Body, Brain, Key, Query};

type Created<T> = (StatusCode, Json<T>);

fn created<T>(member: T) -> Created<T> {
  (StatusCode::CREATED, Json(member))
}

fn written<T>((new, member): (bool, T)) -> Created<T> {
  let status = if new { StatusCode::CREATED } else { StatusCode::OK };
  (status, Json(member))
}

async fn brains(
  State(brain): State<Brain>,
  Query(query): Query<BrainsQuery>,
) -> Answer<Json<Page<BrainMember>>> {
  Ok(Json(brain.store.brains(&query).await?))
}

async fn org_topics(
  State(brain): State<Brain>,
  Query(query): Query<OrgTopicsQuery>,
) -> Answer<Json<Vec<OrgTopic>>> {
  Ok(Json(brain.store.org_topics(&query).await?))
}

async fn brain_add(State(brain): State<Brain>, Body(body): Body<NewBrain>) -> Answer<Created<BrainMember>> {
  Ok(created(brain.store.brain_add(&body).await?))
}

async fn brain_read(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<Json<BrainMember>> {
  Ok(Json(brain.store.brain(id).await?))
}

async fn brain_let_go(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<Json<BrainMember>> {
  Ok(Json(brain.store.brain_let_go(id).await?))
}

async fn subjects(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<SubjectsQuery>,
) -> Answer<Json<Page<Subject>>> {
  Ok(Json(brain.store.subjects(id, &query).await?))
}

async fn subject_add(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<NewSubject>,
) -> Answer<Created<Subject>> {
  Ok(written(brain.store.subject_add(id, &body).await?))
}

async fn subject(State(brain): State<Brain>, Key((id, subject)): Key<(i64, i64)>) -> Answer<Json<Subject>> {
  Ok(Json(brain.store.subject(id, subject).await?))
}

async fn subject_change(
  State(brain): State<Brain>,
  Key((id, subject)): Key<(i64, i64)>,
  Body(body): Body<SubjectChange>,
) -> Answer<Json<Subject>> {
  Ok(Json(brain.store.subject_change(id, subject, &body).await?))
}

async fn layers(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<LayersQuery>,
) -> Answer<Json<Page<Layer>>> {
  Ok(Json(brain.store.layers(id, &query).await?))
}

async fn layer_add(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<NewLayer>,
) -> Answer<Created<Layer>> {
  Ok(written(brain.store.layer_add(id, &body).await?))
}

async fn layer(State(brain): State<Brain>, Key((id, layer)): Key<(i64, i64)>) -> Answer<Json<Layer>> {
  Ok(Json(brain.store.layer(id, layer).await?))
}

async fn layer_change(
  State(brain): State<Brain>,
  Key((id, layer)): Key<(i64, i64)>,
  Body(body): Body<LayerChange>,
) -> Answer<Json<Layer>> {
  Ok(Json(brain.store.layer_change(id, layer, &body).await?))
}

async fn memories(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<MemoriesQuery>,
) -> Answer<Json<Page<Memory>>> {
  Ok(Json(brain.store.memories(id, &query).await?))
}

async fn memory_add(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<NewMemory>,
) -> Answer<Created<Memory>> {
  Ok(created(brain.store.memory_add(id, &body).await?))
}

async fn memory(State(brain): State<Brain>, Key((id, memory)): Key<(i64, i64)>) -> Answer<Json<Memory>> {
  Ok(Json(brain.store.memory(id, memory).await?))
}

async fn memory_version(
  State(brain): State<Brain>,
  Key((id, memory)): Key<(i64, i64)>,
  Body(body): Body<MemoryVersion>,
) -> Answer<Created<Memory>> {
  Ok(created(brain.store.memory_version(id, memory, &body).await?))
}

async fn memory_merge(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<MemoryMerge>,
) -> Answer<Created<Memory>> {
  Ok(created(brain.store.memory_merge(id, &body).await?))
}

async fn memory_move(
  State(brain): State<Brain>,
  Key((id, memory)): Key<(i64, i64)>,
  Body(body): Body<MemoryMove>,
) -> Answer<Json<Memory>> {
  Ok(Json(brain.store.memory_move(id, memory, &body).await?))
}

async fn memory_restore(
  State(brain): State<Brain>,
  Key((id, memory)): Key<(i64, i64)>,
  Body(body): Body<Restoration>,
) -> Answer<Created<Memory>> {
  Ok(created(brain.store.memory_restore(id, memory, &body).await?))
}

async fn lineage(
  State(brain): State<Brain>,
  Key((id, lineage)): Key<(i64, i64)>,
) -> Answer<Json<Vec<Memory>>> {
  Ok(Json(brain.store.lineage(id, lineage).await?))
}

async fn hits(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<Hits>,
) -> Answer<Created<Recorded>> {
  let recorded_count = brain.store.hits_record(id, &body).await?;
  Ok(created(Recorded { recorded_count }))
}

async fn confirmations(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<Confirmations>,
) -> Answer<Created<Recorded>> {
  let recorded_count = brain.store.confirmations_record(id, &body).await?;
  Ok(created(Recorded { recorded_count }))
}

async fn types(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<Json<Vec<TypeCount>>> {
  Ok(Json(brain.store.types(id).await?))
}

async fn tallies(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<TalliesQuery>,
) -> Answer<Json<Vec<Tally>>> {
  Ok(Json(brain.store.tallies(id, &query).await?))
}

async fn recall(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<RecallAsk>,
) -> Answer<Json<Recall>> {
  Ok(Json(brain.store.recall(id, &body).await?))
}

async fn fold(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<Fold>,
) -> Answer<Json<Folded>> {
  let cleared_count = brain.store.fold(id, &body).await?;
  Ok(Json(Folded { cleared_count }))
}

async fn learn(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<Learning>,
) -> Answer<Created<Learned>> {
  Ok(created(brain.store.learn(id, &body).await?))
}

async fn similar(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<SimilarAsk>,
) -> Answer<Json<Vec<Memory>>> {
  Ok(Json(brain.store.similar(id, &body).await?))
}

async fn unconsolidated(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<UnconsolidatedQuery>,
) -> Answer<Json<Vec<UnconsolidatedEpisode>>> {
  Ok(Json(brain.store.unconsolidated(id, query.limit).await?))
}

async fn consolidation(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<ConsolidationQuery>,
) -> Answer<Json<ConsolidationState>> {
  Ok(Json(brain.store.consolidation(id, query.since).await?))
}

async fn duplicates(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<DuplicatesQuery>,
) -> Answer<Json<Vec<DuplicateCandidate>>> {
  Ok(Json(brain.store.duplicates(id, query.limit).await?))
}

async fn misses(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<MissesQuery>,
) -> Answer<Json<Page<SearchMiss>>> {
  Ok(Json(brain.store.misses(id, &query).await?))
}

async fn consolidated(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<Consolidated>,
) -> Answer<Json<Recorded>> {
  let recorded_count = brain.store.consolidated(id, &body.episode_ids).await?;
  Ok(Json(Recorded { recorded_count }))
}

async fn persona(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<PersonaQuery>,
) -> Answer<Response> {
  let text = brain.store.persona(id, &layer_names(query.layers.as_deref())).await?;
  Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

async fn sheet(
  State(brain): State<Brain>,
  Key((id, subject_id)): Key<(i64, i64)>,
  Query(query): Query<SheetQuery>,
) -> Answer<Response> {
  let text = brain.store.sheet(id, subject_id, query.room_id, query.limit).await?;
  Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

/// The layer names a comma-separated query names, trimmed, the empty ones left out.
fn layer_names(query: Option<&str>) -> Vec<String> {
  query
    .unwrap_or_default()
    .split(',')
    .map(str::trim)
    .filter(|name| !name.is_empty())
    .map(str::to_owned)
    .collect()
}

async fn export(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Query(query): Query<ExportQuery>,
) -> Answer<Response> {
  let bytes = brain.store.export(id, &layer_names(query.layers.as_deref())).await?;
  Ok(([(header::CONTENT_TYPE, "application/vnd.sqlite3")], bytes).into_response())
}

async fn inject(State(brain): State<Brain>, Key(id): Key<i64>, body: Bytes) -> Answer<Created<Injected>> {
  Ok(created(brain.store.inject(id, body.to_vec()).await?))
}

/// The brain routes.
pub fn routes() -> Router<Brain> {
  Router::new()
    .route("/brains", get(brains).post(brain_add))
    .route("/org-topics", get(org_topics))
    .route("/brains/{brainId}", get(brain_read).delete(brain_let_go))
    .route("/brains/{brainId}/subjects", get(subjects).post(subject_add))
    .route("/brains/{brainId}/subjects/{subjectId}", get(subject).patch(subject_change))
    .route("/brains/{brainId}/subjects/{subjectId}/sheet", get(sheet))
    .route("/brains/{brainId}/layers", get(layers).post(layer_add))
    .route("/brains/{brainId}/layers/{layerId}", get(layer).patch(layer_change))
    .route("/brains/{brainId}/memories", get(memories).post(memory_add))
    .route("/brains/{brainId}/memories/{memoryId}", get(memory).patch(memory_move))
    .route("/brains/{brainId}/memories/{memoryId}/versions", post(memory_version))
    .route("/brains/{brainId}/memories/{memoryId}/restorations", post(memory_restore))
    .route("/brains/{brainId}/merges", post(memory_merge))
    .route("/brains/{brainId}/lineages/{lineageId}", get(lineage))
    .route("/brains/{brainId}/hits", post(hits))
    .route("/brains/{brainId}/confirmations", post(confirmations))
    .route("/brains/{brainId}/types", get(types))
    .route("/brains/{brainId}/tallies", get(tallies))
    .route("/brains/{brainId}/recall", post(recall))
    .route("/brains/{brainId}/folds", post(fold))
    .route("/brains/{brainId}/learnings", post(learn))
    .route("/brains/{brainId}/similar", post(similar))
    .route("/brains/{brainId}/unconsolidated", get(unconsolidated))
    .route("/brains/{brainId}/consolidation", get(consolidation))
    .route("/brains/{brainId}/consolidated", post(consolidated))
    .route("/brains/{brainId}/duplicates", get(duplicates))
    .route("/brains/{brainId}/misses", get(misses))
    .route("/brains/{brainId}/persona", get(persona))
    .route("/brains/{brainId}/export", get(export))
    .route("/brains/{brainId}/injections", post(inject))
}
