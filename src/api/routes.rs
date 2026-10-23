//! The routes of the memory rows and the fragments the console reads, each one call of the store
//! (docs/boundary-api.md, 「Memory rows」 and 「Fragments」).

use axum::Router;
use axum::extract::State;
use axum::response::Json;
use axum::routing::{get, patch};
use bmo_brain_client::{
  Fragment, FragmentKind, FragmentSwitch, FragmentsQuery, NameCursor, Page, ScopedMemoriesQuery,
  ScopedMemory, ScopedMemoryChange,
};

use super::{Answer, Body, Brain, Key, Query};

async fn memories(
  State(brain): State<Brain>,
  Query(query): Query<ScopedMemoriesQuery>,
) -> Answer<Json<Page<ScopedMemory>>> {
  Ok(Json(brain.store.scoped_memories(&query).await?))
}

async fn memory_change(
  State(brain): State<Brain>,
  Key(id): Key<i64>,
  Body(body): Body<ScopedMemoryChange>,
) -> Answer<Json<ScopedMemory>> {
  Ok(Json(brain.store.scoped_memory_change(id, &body).await?))
}

async fn fragments(
  State(brain): State<Brain>,
  Query(query): Query<FragmentsQuery>,
) -> Answer<Json<Page<Fragment>>> {
  Ok(Json(brain.store.fragments(&query).await?))
}

async fn fragment_switch(
  State(brain): State<Brain>,
  Key(key): Key<String>,
  Body(body): Body<FragmentSwitch>,
) -> Answer<Json<Fragment>> {
  Ok(Json(brain.store.fragment_switch(&key, &body).await?))
}

async fn fragment_kinds(
  State(brain): State<Brain>,
  Query(query): Query<NameCursor>,
) -> Answer<Json<Page<FragmentKind>>> {
  Ok(Json(brain.store.fragment_kinds(&query).await?))
}

pub(super) fn routes() -> Router<Brain> {
  Router::new()
    .route("/memories", get(memories))
    .route("/memories/{memoryId}", patch(memory_change))
    .route("/fragments", get(fragments))
    .route("/fragments/{fragmentKey}", patch(fragment_switch))
    .route("/fragment-kinds", get(fragment_kinds))
}
