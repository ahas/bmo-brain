//! The OpenAPI document: the committed one is what the code generates, and it states exactly the routes and
//! the tools docs/boundary-api.md lists.

use std::collections::BTreeSet;

use bmo_brain::openapi::{openapi, openapi_json};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The routes docs/boundary-api.md states, by method and path.
const ROUTES: [&str; 38] = [
  "get /health",
  "get /events",
  "get /discipline",
  "get /brains",
  "post /brains",
  "get /brains/{brainId}",
  "delete /brains/{brainId}",
  "get /brains/{brainId}/subjects",
  "post /brains/{brainId}/subjects",
  "get /brains/{brainId}/subjects/{subjectId}",
  "patch /brains/{brainId}/subjects/{subjectId}",
  "get /brains/{brainId}/layers",
  "post /brains/{brainId}/layers",
  "get /brains/{brainId}/layers/{layerId}",
  "patch /brains/{brainId}/layers/{layerId}",
  "get /brains/{brainId}/memories",
  "post /brains/{brainId}/memories",
  "get /brains/{brainId}/memories/{memoryId}",
  "patch /brains/{brainId}/memories/{memoryId}",
  "post /brains/{brainId}/memories/{memoryId}/versions",
  "get /brains/{brainId}/lineages/{lineageId}",
  "post /brains/{brainId}/hits",
  "post /brains/{brainId}/confirmations",
  "get /brains/{brainId}/types",
  "post /brains/{brainId}/recall",
  "post /brains/{brainId}/folds",
  "post /brains/{brainId}/learnings",
  "post /brains/{brainId}/similar",
  "get /brains/{brainId}/unconsolidated",
  "post /brains/{brainId}/consolidated",
  "get /brains/{brainId}/persona",
  "get /brains/{brainId}/export",
  "post /brains/{brainId}/injections",
  "get /memories",
  "patch /memories/{memoryId}",
  "get /fragments",
  "patch /fragments/{fragmentKey}",
  "get /fragment-kinds",
];

/// The harness plugin's tools.
pub const TOOLS: [&str; 21] = [
  "brain_brains",
  "brain_brain_add",
  "brain_subjects",
  "brain_subject_add",
  "brain_layers",
  "brain_layer_add",
  "brain_memories_list",
  "brain_memory_write",
  "brain_memory_read",
  "brain_memory_version",
  "brain_memory_move",
  "brain_hits",
  "brain_confirmations",
  "brain_types",
  "brain_recall",
  "brain_fold",
  "brain_learn",
  "brain_similar",
  "brain_unconsolidated",
  "brain_consolidated",
  "brain_persona",
];

#[test]
fn the_committed_document_is_the_generated_one() {
  let committed = std::fs::read_to_string(format!("{ROOT}/docs/openapi.json")).unwrap_or_default();
  assert!(
    committed == openapi_json(),
    "docs/openapi.json differs from what the code generates; write it again with `bmo-brain openapi`"
  );
}

#[test]
fn the_document_states_every_route_and_tool_and_nothing_else() {
  let document = openapi();
  let stated: BTreeSet<String> = document["paths"]
    .as_object()
    .unwrap()
    .iter()
    .flat_map(|(path, methods)| {
      methods.as_object().unwrap().keys().map(move |method| format!("{method} {path}"))
    })
    .collect();
  assert_eq!(stated, ROUTES.iter().map(|route| (*route).to_owned()).collect());
  let tools: Vec<&str> = document["x-tools"]
    .as_array()
    .unwrap()
    .iter()
    .map(|tool| tool["name"].as_str().unwrap())
    .collect();
  assert_eq!(tools, TOOLS);
  for tool in document["x-tools"].as_array().unwrap() {
    let (method, path) = tool["route"].as_str().unwrap().split_once(' ').unwrap();
    let route = format!("{} {path}", method.to_lowercase());
    assert!(ROUTES.contains(&route.as_str()), "{} calls no stated route", tool["name"]);
  }
  assert_eq!(document["x-calls"], serde_json::json!([]));
  assert_eq!(document["info"]["version"], env!("CARGO_PKG_VERSION"));
}
