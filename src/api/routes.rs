//! The routes of every resource, each one call of the store (docs/boundary-api.md).

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post};
use bmo_brain_client::{
  AgentAsk, Done, DryRun, ErrorCode, Fragment, FragmentHits, FragmentHitsRecorded, FragmentKind,
  FragmentMerge, FragmentSplit, FragmentSwitch, FragmentWrite, FragmentsQuery, Knowledge, KnowledgeQuery,
  Library, LibraryPut, LibrarySync, LibrarySyncAsk, LoadsKeep, Lock, LocksQuery, NameCursor, NewLock,
  NewNote, NewReminder, NewScopedMemory, NewScopedMemoryMatch, NewWatch, Note, NoteKeep, NotesQuery, Page,
  Reminder, RemindersQuery, RoomRules, RoomRulesAsk, RuleSet, ScopeChange, ScopeHolding,
  ScopedMemoriesQuery, ScopedMemory, ScopedMemoryChange, ScopedMemoryMatch, SessionAsk, SessionLoads,
  SessionNote, Watch, WatchChange, WatchesQuery,
};
use tokio::time::Instant;

use super::{Answer, ApiError, Body, Brain, Key, Query};
use crate::store::locks::LONGEST_WAIT_MS;

type Created<T> = (StatusCode, Json<T>);

fn created<T>(member: T) -> Created<T> {
  (StatusCode::CREATED, Json(member))
}

/// 201 for a member made, 200 for one replaced.
fn written<T>((new, member): (bool, T)) -> Created<T> {
  let status = if new { StatusCode::CREATED } else { StatusCode::OK };
  (status, Json(member))
}

async fn memories(
  State(brain): State<Brain>,
  Query(query): Query<ScopedMemoriesQuery>,
) -> Answer<Json<Page<ScopedMemory>>> {
  Ok(Json(brain.store.scoped_memories(&query).await?))
}

async fn memory_add(
  State(brain): State<Brain>,
  Body(body): Body<NewScopedMemory>,
) -> Answer<Created<ScopedMemory>> {
  Ok(created(brain.store.scoped_memory_add(&body).await?))
}

async fn memory(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<Json<ScopedMemory>> {
  Ok(Json(brain.store.scoped_memory(id).await?))
}

async fn memory_change(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<ScopedMemoryChange>,
) -> Answer<Json<ScopedMemory>> {
  Ok(Json(brain.store.scoped_memory_change(id, &body).await?))
}

async fn memory_match_add(
  State(brain): State<Brain>,
  Body(body): Body<NewScopedMemoryMatch>,
) -> Answer<Created<ScopedMemoryMatch>> {
  Ok(created(brain.store.scoped_memory_match_add(&body).await?))
}

async fn notes(State(brain): State<Brain>, Query(query): Query<NotesQuery>) -> Answer<Json<Page<Note>>> {
  Ok(Json(brain.store.notes(&query).await?))
}

async fn note_add(State(brain): State<Brain>, Body(body): Body<NewNote>) -> Answer<Created<Note>> {
  Ok(created(brain.store.note_add(&body).await?))
}

async fn room_rules(State(brain): State<Brain>, Body(body): Body<RoomRulesAsk>) -> Answer<Json<RoomRules>> {
  Ok(Json(RoomRules {
    rules: brain.store.room_rules(body.chat_id).await?,
  }))
}

async fn room_rule_set(State(brain): State<Brain>, Body(body): Body<RuleSet>) -> Answer<Json<RoomRules>> {
  Ok(Json(RoomRules {
    rules: brain.store.room_rule_set(&body).await?,
  }))
}

async fn session_loads(
  State(brain): State<Brain>,
  Body(body): Body<SessionAsk>,
) -> Answer<Json<SessionLoads>> {
  Ok(Json(SessionLoads {
    loads: brain.store.session_loads(&body.session).await?,
  }))
}

async fn session_loads_keep(State(brain): State<Brain>, Body(body): Body<LoadsKeep>) -> Answer<Json<Done>> {
  brain.store.session_loads_keep(&body).await?;
  Ok(Json(Done { ok: true }))
}

async fn session_loads_forget(
  State(brain): State<Brain>,
  Body(body): Body<SessionAsk>,
) -> Answer<Json<Done>> {
  brain.store.session_loads_forget(&body.session).await?;
  Ok(Json(Done { ok: true }))
}

async fn session_note(State(brain): State<Brain>, Body(body): Body<AgentAsk>) -> Answer<Json<SessionNote>> {
  Ok(Json(SessionNote {
    note: brain.store.session_note(&body.agent).await?,
  }))
}

async fn session_note_keep(State(brain): State<Brain>, Body(body): Body<NoteKeep>) -> Answer<Json<Done>> {
  brain.store.session_note_keep(&body.agent, &body.note).await?;
  Ok(Json(Done { ok: true }))
}

async fn session_note_clear(State(brain): State<Brain>, Body(body): Body<AgentAsk>) -> Answer<Json<Done>> {
  brain.store.session_note_clear(&body.agent).await?;
  Ok(Json(Done { ok: true }))
}

async fn watches(
  State(brain): State<Brain>,
  Query(query): Query<WatchesQuery>,
) -> Answer<Json<Page<Watch>>> {
  Ok(Json(brain.store.watches(&query).await?))
}

async fn watch_add(State(brain): State<Brain>, Body(body): Body<NewWatch>) -> Answer<Created<Watch>> {
  Ok(created(brain.store.watch_add(&body).await?))
}

async fn watch(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<Json<Watch>> {
  Ok(Json(brain.store.watch(id).await?))
}

async fn watch_change(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<WatchChange>,
) -> Answer<Json<Watch>> {
  Ok(Json(brain.store.watch_change(id, &body).await?))
}

async fn reminders(
  State(brain): State<Brain>,
  Query(query): Query<RemindersQuery>,
) -> Answer<Json<Page<Reminder>>> {
  Ok(Json(brain.store.reminders(&query).await?))
}

async fn reminder_add(
  State(brain): State<Brain>,
  Body(body): Body<NewReminder>,
) -> Answer<Created<Reminder>> {
  Ok(created(brain.store.reminder_add(&body).await?))
}

async fn reminder_cancel(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<StatusCode> {
  brain.store.reminder_cancel(id).await?;
  Ok(StatusCode::NO_CONTENT)
}

async fn fragments(
  State(brain): State<Brain>,
  Query(query): Query<FragmentsQuery>,
) -> Answer<Json<Page<Fragment>>> {
  Ok(Json(brain.store.fragments(&query).await?))
}

async fn fragment_put(
  State(brain): State<Brain>,
  Key(key): Key<String>,
  Body(body): Body<FragmentWrite>,
) -> Answer<Created<Fragment>> {
  Ok(written(brain.store.fragment_put(&key, &body).await?))
}

async fn fragment_switch(
  State(brain): State<Brain>,
  Key(key): Key<String>,
  Body(body): Body<FragmentSwitch>,
) -> Answer<Json<Fragment>> {
  Ok(Json(brain.store.fragment_switch(&key, &body).await?))
}

async fn fragment_remove(State(brain): State<Brain>, Key(key): Key<String>) -> Answer<StatusCode> {
  brain.store.fragment_remove(&key).await?;
  Ok(StatusCode::NO_CONTENT)
}

async fn fragment_merge(
  State(brain): State<Brain>,
  Body(body): Body<FragmentMerge>,
) -> Answer<Created<Fragment>> {
  Ok(created(brain.store.fragment_merge(&body).await?))
}

async fn fragment_split(
  State(brain): State<Brain>,
  Body(body): Body<FragmentSplit>,
) -> Answer<Created<Page<Fragment>>> {
  Ok(created(brain.store.fragment_split(&body).await?))
}

async fn fragment_hits(
  State(brain): State<Brain>,
  Body(body): Body<FragmentHits>,
) -> Answer<Created<FragmentHitsRecorded>> {
  Ok(created(brain.store.fragment_hits(&body).await?))
}

async fn fragment_kinds(
  State(brain): State<Brain>,
  Query(query): Query<NameCursor>,
) -> Answer<Json<Page<FragmentKind>>> {
  Ok(Json(brain.store.fragment_kinds(&query).await?))
}

async fn libraries(
  State(brain): State<Brain>,
  Query(query): Query<NameCursor>,
) -> Answer<Json<Page<Library>>> {
  Ok(Json(brain.store.libraries(&query).await?))
}

async fn library(State(brain): State<Brain>, Key(name): Key<String>) -> Answer<Json<Library>> {
  Ok(Json(brain.store.library(&name).await?))
}

async fn library_put(
  State(brain): State<Brain>,
  Key(name): Key<String>,
  Body(body): Body<LibraryPut>,
) -> Answer<Created<Library>> {
  Ok(written(brain.store.library_put(&name, &body).await?))
}

async fn library_sync(
  State(brain): State<Brain>,
  Query(dry): Query<DryRun>,
  Body(body): Body<LibrarySyncAsk>,
) -> Answer<Json<LibrarySync>> {
  Ok(Json(brain.store.library_sync(&body.library_name, dry.dry_run.unwrap_or(false)).await?))
}

async fn knowledge(
  State(brain): State<Brain>,
  Query(query): Query<KnowledgeQuery>,
) -> Answer<Json<Knowledge>> {
  Ok(Json(brain.store.knowledge(&query).await?))
}

async fn locks(State(brain): State<Brain>, Query(query): Query<LocksQuery>) -> Answer<Json<Page<Lock>>> {
  let wait = query.wait_ms.unwrap_or(0);
  if !(0..=LONGEST_WAIT_MS).contains(&wait) {
    return Err(ApiError::new(
      ErrorCode::BadRequest,
      format!("waitMs is {wait}; it is 0 to {LONGEST_WAIT_MS}."),
    ));
  }
  let until = Instant::now() + Duration::from_millis(wait.unsigned_abs());
  let mut moved = brain.store.moved();
  loop {
    let page = brain.store.locks(&query).await?;
    let left = until.saturating_duration_since(Instant::now());
    if page.items.is_empty() || left.is_zero() {
      return Ok(Json(page));
    }
    let _ = tokio::time::timeout(left.min(Duration::from_secs(1)), moved.changed()).await;
  }
}

async fn lock(State(brain): State<Brain>, Body(body): Body<NewLock>) -> Answer<Created<Lock>> {
  Ok(written(brain.store.lock(&body).await?))
}

async fn unlock(State(brain): State<Brain>, Key(id): Key<i64>) -> Answer<StatusCode> {
  brain.store.unlock(id).await?;
  Ok(StatusCode::NO_CONTENT)
}

async fn scope(State(brain): State<Brain>, Key(name): Key<String>) -> Answer<Json<ScopeHolding>> {
  Ok(Json(brain.store.scope(&name).await?))
}

async fn scope_mark(
  State(brain): State<Brain>,
  Key(name): Key<String>,
  Body(body): Body<ScopeChange>,
) -> Answer<Json<ScopeHolding>> {
  Ok(Json(brain.store.scope_mark(&name, body.state).await?))
}

async fn scope_let_go(
  State(brain): State<Brain>,
  Key(name): Key<String>,
  Query(dry): Query<DryRun>,
) -> Answer<Json<ScopeHolding>> {
  Ok(Json(brain.store.scope_let_go(&name, dry.dry_run.unwrap_or(false)).await?))
}

pub(super) fn routes() -> Router<Brain> {
  Router::new()
    .route("/memories", get(memories).post(memory_add))
    .route("/memories/{memoryId}", get(memory).patch(memory_change))
    .route("/memory-matches", post(memory_match_add))
    .route("/notes", get(notes).post(note_add))
    .route("/room-rules/read", post(room_rules))
    .route("/room-rules/set", post(room_rule_set))
    .route("/sessions/loads", post(session_loads))
    .route("/sessions/loads/keep", post(session_loads_keep))
    .route("/sessions/loads/forget", post(session_loads_forget))
    .route("/session-summary/read", post(session_note))
    .route("/session-summary/keep", post(session_note_keep))
    .route("/session-summary/clear", post(session_note_clear))
    .route("/watches", get(watches).post(watch_add))
    .route("/watches/{watchId}", get(watch).patch(watch_change))
    .route("/reminders", get(reminders).post(reminder_add))
    .route("/reminders/{reminderId}", delete(reminder_cancel))
    .route("/fragments", get(fragments))
    .route(
      "/fragments/{fragmentKey}",
      axum::routing::put(fragment_put).patch(fragment_switch).delete(fragment_remove),
    )
    .route("/fragment-merges", post(fragment_merge))
    .route("/fragment-splits", post(fragment_split))
    .route("/fragment-hits", post(fragment_hits))
    .route("/fragment-kinds", get(fragment_kinds))
    .route("/libraries", get(libraries))
    .route("/libraries/{libraryName}", get(library).put(library_put))
    .route("/library-syncs", post(library_sync))
    .route("/knowledge", get(knowledge))
    .route("/locks", get(locks).post(lock))
    .route("/locks/{lockId}", delete(unlock))
    .route("/scopes/{scope}", get(scope).patch(scope_mark).delete(scope_let_go))
}
