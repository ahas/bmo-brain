//! The OpenAPI document: the committed one is what the code generates, and it states exactly the routes and
//! the tools docs/boundary-api.md lists, with no route that names a caller.

use std::collections::BTreeSet;

use bmo_brain::openapi::{openapi, openapi_json};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The routes docs/boundary-api.md states, by method and path.
const ROUTES: [&str; 35] = [
  "get /health",
  "get /events",
  "get /memories",
  "post /memories",
  "get /memories/{memoryId}",
  "patch /memories/{memoryId}",
  "post /memory-matches",
  "get /notes",
  "post /notes",
  "get /watches",
  "post /watches",
  "get /watches/{watchId}",
  "patch /watches/{watchId}",
  "get /reminders",
  "post /reminders",
  "delete /reminders/{reminderId}",
  "get /fragments",
  "put /fragments/{fragmentKey}",
  "patch /fragments/{fragmentKey}",
  "delete /fragments/{fragmentKey}",
  "post /fragment-merges",
  "post /fragment-splits",
  "post /fragment-hits",
  "get /fragment-kinds",
  "get /libraries",
  "get /libraries/{libraryName}",
  "put /libraries/{libraryName}",
  "post /library-syncs",
  "get /knowledge",
  "get /locks",
  "post /locks",
  "delete /locks/{lockId}",
  "get /scopes/{scope}",
  "patch /scopes/{scope}",
  "delete /scopes/{scope}",
];

/// The harness plugin's tools.
pub const TOOLS: [&str; 17] = [
  "brain_memories",
  "brain_memory_add",
  "brain_memory",
  "brain_memory_update",
  "brain_notes",
  "brain_note_add",
  "brain_watches",
  "brain_watch_add",
  "brain_watch_update",
  "brain_reminders",
  "brain_reminder_add",
  "brain_reminder_cancel",
  "brain_fragments",
  "brain_fragment_put",
  "brain_knowledge",
  "brain_lock",
  "brain_unlock",
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
