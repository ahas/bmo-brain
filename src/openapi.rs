//! The brain's OpenAPI document, `docs/openapi.json`: every route docs/boundary-api.md states, with its
//! parameters, its body, its answers and the codes it can give, the bodies' schemas generated from the
//! client's own types, the events it publishes, and the harness plugin's tools from the table the MCP server
//! lists.

use std::collections::BTreeMap;

use bmo_brain_client::{
  AgentAsk, Brain, BrainsQuery, Confirmations, Done, DryRun, ErrorCode, Event, EventsQuery, ExportQuery,
  Fold, Folded, Fragment, FragmentHits, FragmentHitsRecorded, FragmentKind, FragmentMerge, FragmentSplit,
  FragmentSwitch, FragmentWrite, FragmentsQuery, Health, Hits, Injected, Knowledge, KnowledgeQuery, Layer,
  LayerChange, LayersQuery, Library, LibraryPut, LibrarySync, LibrarySyncAsk, LoadsKeep, Lock, LocksQuery,
  MemoriesQuery, Memory, MemoryMove, MemoryVersion, NameCursor, NewBrain, NewLayer, NewLock, NewMemory,
  NewNote, NewReminder, NewScopedMemory, NewScopedMemoryMatch, NewSubject, NewWatch, Note, NoteKeep,
  NotesQuery, Page, Recall, RecallAsk, Recorded, Reminder, RemindersQuery, RoomRules, RoomRulesAsk,
  RuleSet, ScopeChange, ScopeHolding, ScopedMemoriesQuery, ScopedMemory, ScopedMemoryChange,
  ScopedMemoryMatch, SessionAsk, SessionLoads, SessionNote, Subject, SubjectChange, SubjectsQuery,
  TypeCount, Watch, WatchChange, WatchesQuery, WireError,
};
use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::{JsonSchema, Schema};
use serde_json::{Map, Value, json};

use crate::mcp::tools;

/// Where the schemas the routes refer to stand in the document.
const SCHEMAS: &str = "/components/schemas";

const DESCRIPTION: &str = "An agent's long-term memory as a service. The service keeps many brains, each an \
  account's and one agent's: subjects (what a memory is about, found by type and key), layers (the table of \
  contents) and memories, one row a version, superseded and invalidated but never deleted. A turn asks recall for \
  a block under a token budget; a persona is rendered as the same bytes until a piece changes; a brain moves in and \
  out as a brain file. The rows held under a caller's opaque scope (memory rows, notes, watches, reminders, \
  fragments, libraries, locks and the caller's own records) keep answering until their readers move to the brains. \
  Bodies are JSON with camelCase keys, times are epoch milliseconds, lists are pages of items and more, and every \
  failure is the error shape with its status. The harness plugin's tools, each a thin call of one route, are \
  stated under x-tools.";

const ENTRY: &str = "A harness finds or makes its brain with brain_brains and brain_brain_add, reads what a turn \
  should know with brain_recall, writes what it learns with brain_memory_write and brain_memory_version, and keeps \
  the scope routes' tools for rows its callers still hold under scopes.";

/// The events the brain publishes.
pub const EVENTS: [&str; 21] = [
  "brain.created",
  "brain.deleted",
  "brain.injected",
  "subject.created",
  "subject.changed",
  "layer.changed",
  "memory.written",
  "memory.moved",
  "memory.created",
  "memory.changed",
  "note.created",
  "watch.changed",
  "watch.fired",
  "reminder.created",
  "reminder.due",
  "reminder.deleted",
  "fragment.changed",
  "fragment.deleted",
  "library.changed",
  "lock.changed",
  "scope.changed",
];

/// One route as the document states it.
struct Route {
  method: &'static str,
  path: &'static str,
  id: &'static str,
  tag: &'static str,
  summary: &'static str,
  parameters: Vec<Value>,
  body: Option<Schema>,
  /// A request body that is not JSON: its media type.
  raw_body: Option<&'static str>,
  /// The success answers, each by its status: the content by media type, or none.
  answers: Vec<(u16, Option<Value>)>,
  /// The codes beside `unauthorized` the route can answer.
  codes: Vec<ErrorCode>,
}

impl Route {
  fn operation(self, error: &Schema) -> Value {
    let mut codes = vec![ErrorCode::Unauthorized];
    codes.extend(self.codes);
    codes.sort_by_key(|code| ErrorCode::ALL.iter().position(|all| all == code));
    codes.dedup();
    let mut statuses: BTreeMap<u16, Vec<ErrorCode>> = BTreeMap::new();
    for &code in &codes {
      statuses.entry(code.status()).or_default().push(code);
    }
    let mut responses = Map::new();
    for (status, content) in self.answers {
      let description = match status {
        201 => "Created.",
        204 => "Done; no content.",
        _ => "Answered.",
      };
      let mut answer = json!({ "description": description });
      if let Some(content) = content {
        answer["content"] = content;
      }
      responses.insert(status.to_string(), answer);
    }
    for (status, codes) in statuses {
      let answer = match codes.as_slice() {
        [code] => json!({ "$ref": format!("#/components/responses/{}", response_name(*code)) }),
        _ => answered(error, &codes),
      };
      responses.insert(status.to_string(), answer);
    }
    let mut operation = json!({
      "operationId": self.id,
      "tags": [self.tag],
      "summary": self.summary,
      "responses": responses,
      "x-codes": codes.iter().map(|code| code.name()).collect::<Vec<_>>(),
    });
    if !self.parameters.is_empty() {
      operation["parameters"] = self.parameters.into();
    }
    if let Some(body) = self.body {
      operation["requestBody"] = json!({
        "required": true,
        "content": { "application/json": { "schema": body } },
      });
    }
    if let Some(media) = self.raw_body {
      operation["requestBody"] = json!({
        "required": true,
        "content": { media: { "schema": { "type": "string", "format": "binary" } } },
      });
    }
    operation
  }
}

/// An answer in the error shape with one of these codes, all answered with one status.
fn answered(error: &Schema, codes: &[ErrorCode]) -> Value {
  let meaning = |code: ErrorCode| format!("`{}`: {}", code.name(), code.meaning());
  let description = match codes {
    [code] => meaning(*code),
    _ => codes.iter().map(|&code| format!("- {}", meaning(code))).collect::<Vec<_>>().join("\n"),
  };
  json!({
    "description": description,
    "content": { "application/json": { "schema": error } },
  })
}

fn response_name(code: ErrorCode) -> String {
  format!("{code:?}")
}

fn json_answer<Answer: JsonSchema>(generator: &mut SchemaGenerator) -> Option<Value> {
  Some(json!({ "application/json": { "schema": generator.subschema_for::<Answer>() } }))
}

/// One path parameter: a member's id.
fn id(generator: &mut SchemaGenerator, name: &str) -> Value {
  json!({ "name": name, "in": "path", "required": true, "schema": generator.subschema_for::<i64>() })
}

/// One path parameter: a key the caller names.
fn named(generator: &mut SchemaGenerator, name: &str) -> Value {
  json!({ "name": name, "in": "path", "required": true, "schema": generator.subschema_for::<String>() })
}

/// The query parameters of a query type, each property one parameter, required as the type requires it.
fn query<Query: JsonSchema>(generator: &mut SchemaGenerator) -> Vec<Value> {
  let schema = Query::json_schema(generator).to_value();
  let required: Vec<&str> = schema["required"]
    .as_array()
    .map(|names| names.iter().filter_map(Value::as_str).collect())
    .unwrap_or_default();
  let Some(properties) = schema["properties"].as_object() else {
    return Vec::new();
  };
  properties
    .iter()
    .map(|(name, property)| {
      let mut parameter = json!({
        "name": name,
        "in": "query",
        "required": required.contains(&name.as_str()),
        "schema": property,
      });
      if let Some(description) = property.get("description") {
        parameter["description"] = description.clone();
      }
      parameter
    })
    .collect()
}

fn body<Body: JsonSchema>(generator: &mut SchemaGenerator) -> Option<Schema> {
  Some(generator.subschema_for::<Body>())
}

use ErrorCode::{BadRequest, Conflict, Gone, NotFound};

fn brain_routes(g: &mut SchemaGenerator) -> Vec<Route> {
  let brain = |g: &mut SchemaGenerator| vec![id(g, "brainId")];
  let under = |g: &mut SchemaGenerator, name: &str| vec![id(g, "brainId"), id(g, name)];
  vec![
    Route {
      method: "get",
      path: "/brains",
      id: "listBrains",
      tag: "brains",
      summary: "The brains, by account and agent.",
      parameters: query::<BrainsQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Brain>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/brains",
      id: "createBrain",
      tag: "brains",
      summary: "Make a brain for an account and an agent that have none.",
      parameters: vec![],
      body: body::<NewBrain>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Brain>(g))],
      codes: vec![BadRequest, Conflict],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}",
      id: "readBrain",
      tag: "brains",
      summary: "One brain.",
      parameters: brain(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Brain>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "delete",
      path: "/brains/{brainId}",
      id: "deleteBrain",
      tag: "brains",
      summary: "Let a brain go whole, with everything it held.",
      parameters: brain(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Brain>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/subjects",
      id: "listSubjects",
      tag: "subjects",
      summary: "A brain's subjects, by type, key and words of the description.",
      parameters: {
        let mut parameters = brain(g);
        parameters.extend(query::<SubjectsQuery>(g));
        parameters
      },
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Subject>>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/subjects",
      id: "createSubject",
      tag: "subjects",
      summary: "Add a subject, or the one standing at its type and key with its description set.",
      parameters: brain(g),
      body: body::<NewSubject>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Subject>(g)), (200, json_answer::<Subject>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/subjects/{subjectId}",
      id: "readSubject",
      tag: "subjects",
      summary: "One subject.",
      parameters: under(g, "subjectId"),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Subject>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "patch",
      path: "/brains/{brainId}/subjects/{subjectId}",
      id: "changeSubject",
      tag: "subjects",
      summary: "Change a subject's description.",
      parameters: under(g, "subjectId"),
      body: body::<SubjectChange>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Subject>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/layers",
      id: "listLayers",
      tag: "layers",
      summary: "A brain's layers, by position.",
      parameters: {
        let mut parameters = brain(g);
        parameters.extend(query::<LayersQuery>(g));
        parameters
      },
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Layer>>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/layers",
      id: "createLayer",
      tag: "layers",
      summary: "Add a layer, or the one standing at its name.",
      parameters: brain(g),
      body: body::<NewLayer>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Layer>(g)), (200, json_answer::<Layer>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/layers/{layerId}",
      id: "readLayer",
      tag: "layers",
      summary: "One layer.",
      parameters: under(g, "layerId"),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Layer>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "patch",
      path: "/brains/{brainId}/layers/{layerId}",
      id: "changeLayer",
      tag: "layers",
      summary: "Change a layer's description or position.",
      parameters: under(g, "layerId"),
      body: body::<LayerChange>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Layer>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/memories",
      id: "listBrainMemories",
      tag: "brain-memories",
      summary: "A brain's memories, by type, status, layer, subject, key, lineage, label and words.",
      parameters: {
        let mut parameters = brain(g);
        parameters.extend(query::<MemoriesQuery>(g));
        parameters
      },
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Memory>>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/memories",
      id: "writeMemory",
      tag: "brain-memories",
      summary: "Write a memory as a first version.",
      parameters: brain(g),
      body: body::<NewMemory>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Memory>(g))],
      codes: vec![BadRequest, NotFound, Conflict],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/memories/{memoryId}",
      id: "readBrainMemory",
      tag: "brain-memories",
      summary: "One memory in one version, with its subjects and strengths.",
      parameters: under(g, "memoryId"),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Memory>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "patch",
      path: "/brains/{brainId}/memories/{memoryId}",
      id: "moveMemory",
      tag: "brain-memories",
      summary: "Move a memory's status: approved, done or invalidated.",
      parameters: under(g, "memoryId"),
      body: body::<MemoryMove>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Memory>(g))],
      codes: vec![BadRequest, NotFound, Conflict],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/memories/{memoryId}/versions",
      id: "writeMemoryVersion",
      tag: "brain-memories",
      summary: "Write a new version of a memory, superseding the one named.",
      parameters: under(g, "memoryId"),
      body: body::<MemoryVersion>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Memory>(g))],
      codes: vec![BadRequest, NotFound, Conflict],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/lineages/{lineageId}",
      id: "readLineage",
      tag: "brain-memories",
      summary: "Every version of a lineage, oldest first.",
      parameters: under(g, "lineageId"),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Vec<Memory>>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/hits",
      id: "recordHits",
      tag: "brain-memories",
      summary: "Record a hand-out: each row hit, and marked for the session named.",
      parameters: brain(g),
      body: body::<Hits>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Recorded>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/confirmations",
      id: "recordConfirmations",
      tag: "brain-memories",
      summary: "Record confirmations: each row's storage strength raised by the gap since its last.",
      parameters: brain(g),
      body: body::<Confirmations>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Recorded>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/types",
      id: "listTypes",
      tag: "brain-memories",
      summary: "The types in use, counted, the reserved ones first.",
      parameters: brain(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Vec<TypeCount>>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/recall",
      id: "recall",
      tag: "recall",
      summary: "What a turn is handed: the room's core and the searched part, under the budget.",
      parameters: brain(g),
      body: body::<RecallAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Recall>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/folds",
      id: "fold",
      tag: "recall",
      summary: "A session folded or handed off: its marks cleared.",
      parameters: brain(g),
      body: body::<Fold>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Folded>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/persona",
      id: "renderPersona",
      tag: "persona",
      summary: "The persona rendered as text, the same bytes until a piece changes.",
      parameters: brain(g),
      body: None,
      raw_body: None,
      answers: vec![(200, Some(json!({ "text/plain": { "schema": { "type": "string" } } })))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/brains/{brainId}/export",
      id: "exportBrain",
      tag: "brain-files",
      summary: "The brain, or the layers named, as a brain file.",
      parameters: {
        let mut parameters = brain(g);
        parameters.extend(query::<ExportQuery>(g));
        parameters
      },
      body: None,
      raw_body: None,
      answers: vec![(
        200,
        Some(json!({ "application/vnd.sqlite3": { "schema": { "type": "string", "format": "binary" } } })),
      )],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "post",
      path: "/brains/{brainId}/injections",
      id: "injectBrain",
      tag: "brain-files",
      summary: "Read a brain file into the brain: keyed rows supersede, the rest are added, nothing overwritten.",
      parameters: brain(g),
      body: None,
      raw_body: Some("application/vnd.sqlite3"),
      answers: vec![(201, json_answer::<Injected>(g))],
      codes: vec![BadRequest, NotFound],
    },
  ]
}

/// Every route, in the order docs/boundary-api.md states them.
fn routes(g: &mut SchemaGenerator) -> Vec<Route> {
  let mut all = brain_routes(g);
  all.extend(scope_routes(g));
  all
}

fn scope_routes(g: &mut SchemaGenerator) -> Vec<Route> {
  vec![
    Route {
      method: "get",
      path: "/health",
      id: "readHealth",
      tag: "health",
      summary: "Readiness and version.",
      parameters: vec![],
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Health>(g))],
      codes: vec![],
    },
    Route {
      method: "get",
      path: "/events",
      id: "listEvents",
      tag: "events",
      summary: "The brain's one stream: server-sent events, or a page to a reader asking for JSON.",
      parameters: {
        let mut parameters = query::<EventsQuery>(g);
        parameters.push(json!({
          "name": "Last-Event-ID",
          "in": "header",
          "required": false,
          "description": "The last event number seen, as a reconnecting stream sends it.",
          "schema": g.subschema_for::<i64>(),
        }));
        parameters
      },
      body: None,
      raw_body: None,
      answers: vec![(
        200,
        Some(json!({
          "text/event-stream": { "schema": { "type": "string", "description": "Each event's data is an Event." } },
          "application/json": { "schema": g.subschema_for::<Page<Event>>() },
        })),
      )],
      codes: vec![BadRequest, Gone],
    },
    Route {
      method: "get",
      path: "/memories",
      id: "listMemories",
      tag: "memories",
      summary: "Memory rows by scope, scope prefix, subject, kind, state, label; standing rows, rows due.",
      parameters: query::<ScopedMemoriesQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<ScopedMemory>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/memories",
      id: "createMemory",
      tag: "memories",
      summary: "Add a memory row.",
      parameters: vec![],
      body: body::<NewScopedMemory>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<ScopedMemory>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/memories/{memoryId}",
      id: "readMemory",
      tag: "memories",
      summary: "One memory row and the watches on it.",
      parameters: vec![id(g, "memoryId")],
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<ScopedMemory>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "patch",
      path: "/memories/{memoryId}",
      id: "changeMemory",
      tag: "memories",
      summary: "Change a row: its words, or its state (done, retired, candidate, off, standing again).",
      parameters: vec![id(g, "memoryId")],
      body: body::<ScopedMemoryChange>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<ScopedMemory>(g))],
      codes: vec![BadRequest, NotFound, Conflict],
    },
    Route {
      method: "post",
      path: "/memory-matches",
      id: "createMemoryMatch",
      tag: "memory-matches",
      summary: "Record which rows a reading matched and what it spent.",
      parameters: vec![],
      body: body::<NewScopedMemoryMatch>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<ScopedMemoryMatch>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/notes",
      id: "listNotes",
      tag: "notes",
      summary: "Notes by scope and subject.",
      parameters: query::<NotesQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Note>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/notes",
      id: "createNote",
      tag: "notes",
      summary: "Add a note.",
      parameters: vec![],
      body: body::<NewNote>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Note>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/room-rules/read",
      id: "readRoomRules",
      tag: "room-rules",
      summary: "A room's rules, in the order they were set.",
      parameters: vec![],
      body: body::<RoomRulesAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<RoomRules>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/room-rules/set",
      id: "setRoomRule",
      tag: "room-rules",
      summary: "Set or revoke one rule on a message; the room's rules as they now stand.",
      parameters: vec![],
      body: body::<RuleSet>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<RoomRules>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/sessions/loads",
      id: "readSessionLoads",
      tag: "sessions",
      summary: "The documents a session was handed, by path and version.",
      parameters: vec![],
      body: body::<SessionAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<SessionLoads>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/sessions/loads/keep",
      id: "keepSessionLoads",
      tag: "sessions",
      summary: "Keep the documents a session was handed; sessions untouched for a week are let go.",
      parameters: vec![],
      body: body::<LoadsKeep>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Done>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/sessions/loads/forget",
      id: "forgetSessionLoads",
      tag: "sessions",
      summary: "Let a session's loads go.",
      parameters: vec![],
      body: body::<SessionAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Done>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/session-summary/read",
      id: "readSessionNote",
      tag: "session-summary",
      summary: "An agent's handoff note, or null where it left none.",
      parameters: vec![],
      body: body::<AgentAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<SessionNote>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/session-summary/keep",
      id: "keepSessionNote",
      tag: "session-summary",
      summary: "Keep an agent's handoff note, overwriting the one it had.",
      parameters: vec![],
      body: body::<NoteKeep>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Done>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/session-summary/clear",
      id: "clearSessionNote",
      tag: "session-summary",
      summary: "Clear an agent's handoff note.",
      parameters: vec![],
      body: body::<AgentAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Done>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/watches",
      id: "listWatches",
      tag: "watches",
      summary: "Watches by scope, setter, holder or state.",
      parameters: query::<WatchesQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Watch>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/watches",
      id: "createWatch",
      tag: "watches",
      summary: "Set a watch.",
      parameters: vec![],
      body: body::<NewWatch>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Watch>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/watches/{watchId}",
      id: "readWatch",
      tag: "watches",
      summary: "One watch.",
      parameters: vec![id(g, "watchId")],
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Watch>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "patch",
      path: "/watches/{watchId}",
      id: "changeWatch",
      tag: "watches",
      summary: "Move a watch: fired (an external condition held), claimed by a holder (409 when held), ended, \
                cancelled.",
      parameters: vec![id(g, "watchId")],
      body: body::<WatchChange>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Watch>(g))],
      codes: vec![BadRequest, NotFound, Conflict],
    },
    Route {
      method: "get",
      path: "/reminders",
      id: "listReminders",
      tag: "reminders",
      summary: "Reminders by scope, due before a time, fired or waiting.",
      parameters: query::<RemindersQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Reminder>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/reminders",
      id: "createReminder",
      tag: "reminders",
      summary: "Set a reminder; reminder.due is published at its time.",
      parameters: vec![],
      body: body::<NewReminder>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Reminder>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "delete",
      path: "/reminders/{reminderId}",
      id: "deleteReminder",
      tag: "reminders",
      summary: "Cancel a reminder.",
      parameters: vec![id(g, "reminderId")],
      body: None,
      raw_body: None,
      answers: vec![(204, None)],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/fragments",
      id: "listFragments",
      tag: "fragments",
      summary: "Fragments by kind, scope, subject, library, words, state, ids or keys; kind=person reads person \
                sheets.",
      parameters: query::<FragmentsQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Fragment>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "put",
      path: "/fragments/{fragmentKey}",
      id: "putFragment",
      tag: "fragments",
      summary: "Write a fragment whole.",
      parameters: vec![named(g, "fragmentKey")],
      body: body::<FragmentWrite>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Fragment>(g)), (201, json_answer::<Fragment>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "patch",
      path: "/fragments/{fragmentKey}",
      id: "changeFragment",
      tag: "fragments",
      summary: "Switch a fragment off or on for scopes, or set its state.",
      parameters: vec![named(g, "fragmentKey")],
      body: body::<FragmentSwitch>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Fragment>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "delete",
      path: "/fragments/{fragmentKey}",
      id: "deleteFragment",
      tag: "fragments",
      summary: "Remove a fragment.",
      parameters: vec![named(g, "fragmentKey")],
      body: None,
      raw_body: None,
      answers: vec![(204, None)],
      codes: vec![NotFound],
    },
    Route {
      method: "post",
      path: "/fragment-merges",
      id: "createFragmentMerge",
      tag: "fragment-merges",
      summary: "Merge fragments into one.",
      parameters: vec![],
      body: body::<FragmentMerge>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Fragment>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/fragment-splits",
      id: "createFragmentSplit",
      tag: "fragment-splits",
      summary: "Split a fragment into pieces.",
      parameters: vec![],
      body: body::<FragmentSplit>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<Page<Fragment>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/fragment-hits",
      id: "createFragmentHits",
      tag: "fragment-hits",
      summary: "Record the fragments a reading attached.",
      parameters: vec![],
      body: body::<FragmentHits>(g),
      raw_body: None,
      answers: vec![(201, json_answer::<FragmentHitsRecorded>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/fragment-kinds",
      id: "listFragmentKinds",
      tag: "fragment-kinds",
      summary: "The kinds of fragment, by name.",
      parameters: query::<NameCursor>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<FragmentKind>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/libraries",
      id: "listLibraries",
      tag: "libraries",
      summary: "The libraries by name, each with its documents and their SHA-256 versions.",
      parameters: query::<NameCursor>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Library>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/libraries/{libraryName}",
      id: "readLibrary",
      tag: "libraries",
      summary: "One library and its documents.",
      parameters: vec![named(g, "libraryName")],
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Library>(g))],
      codes: vec![NotFound],
    },
    Route {
      method: "put",
      path: "/libraries/{libraryName}",
      id: "putLibrary",
      tag: "libraries",
      summary: "A library put whole, replacing what it said before.",
      parameters: vec![named(g, "libraryName")],
      body: body::<LibraryPut>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Library>(g)), (201, json_answer::<Library>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/library-syncs",
      id: "createLibrarySync",
      tag: "library-syncs",
      summary: "Bring the fragments in line with their library's documents; dryRun reports the drift.",
      parameters: query::<DryRun>(g),
      body: body::<LibrarySyncAsk>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<LibrarySync>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/knowledge",
      id: "readKnowledge",
      tag: "knowledge",
      summary: "Knowledge for a set of scopes: fragments by slugs and kinds, and the standing rows.",
      parameters: query::<KnowledgeQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Knowledge>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "get",
      path: "/locks",
      id: "listLocks",
      tag: "locks",
      summary: "Who holds the store for editing; waitMs answers once it is free.",
      parameters: query::<LocksQuery>(g),
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<Page<Lock>>(g))],
      codes: vec![BadRequest],
    },
    Route {
      method: "post",
      path: "/locks",
      id: "createLock",
      tag: "locks",
      summary: "Lock the store for editing; 409 while another holds it, renewed for the holder holding it.",
      parameters: vec![],
      body: body::<NewLock>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<Lock>(g)), (201, json_answer::<Lock>(g))],
      codes: vec![BadRequest, Conflict],
    },
    Route {
      method: "delete",
      path: "/locks/{lockId}",
      id: "deleteLock",
      tag: "locks",
      summary: "Let a lock go.",
      parameters: vec![id(g, "lockId")],
      body: None,
      raw_body: None,
      answers: vec![(204, None)],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "get",
      path: "/scopes/{scope}",
      id: "readScope",
      tag: "scopes",
      summary: "What the service holds under a scope, counted by resource.",
      parameters: vec![named(g, "scope")],
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<ScopeHolding>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "patch",
      path: "/scopes/{scope}",
      id: "changeScope",
      tag: "scopes",
      summary: "Mark a scope forgotten, so its rows wither, or remembered again.",
      parameters: vec![named(g, "scope")],
      body: body::<ScopeChange>(g),
      raw_body: None,
      answers: vec![(200, json_answer::<ScopeHolding>(g))],
      codes: vec![BadRequest, NotFound],
    },
    Route {
      method: "delete",
      path: "/scopes/{scope}",
      id: "deleteScope",
      tag: "scopes",
      summary: "Let go of everything held under a scope; dryRun=true answers what would go.",
      parameters: {
        let mut parameters = vec![named(g, "scope")];
        parameters.extend(query::<DryRun>(g));
        parameters
      },
      body: None,
      raw_body: None,
      answers: vec![(200, json_answer::<ScopeHolding>(g))],
      codes: vec![BadRequest, NotFound],
    },
  ]
}

/// The brain's OpenAPI document.
pub fn openapi() -> Value {
  let mut generator = SchemaSettings::draft2020_12()
    .for_serialize()
    .with(|settings| {
      settings.definitions_path = SCHEMAS.into();
      settings.meta_schema = None;
    })
    .into_generator();
  let error = generator.subschema_for::<WireError>();
  let mut tags: Vec<&str> = Vec::new();
  let mut paths = Map::new();
  for route in routes(&mut generator) {
    if !tags.contains(&route.tag) {
      tags.push(route.tag);
    }
    let (path, method) = (route.path, route.method);
    paths.entry(path).or_insert_with(|| json!({}))[method] = route.operation(&error);
  }
  let x_tools: Vec<Value> = tools()
    .iter()
    .map(|tool| {
      json!({
        "name": tool.name,
        "description": tool.description,
        "input": tool.input(&mut generator),
        "output": tool.output(&mut generator),
        "route": tool.route,
      })
    })
    .collect();
  let responses: Map<String, Value> = ErrorCode::ALL
    .iter()
    .map(|&code| (response_name(code), answered(&error, &[code])))
    .collect();
  json!({
    "openapi": "3.1.0",
    "info": {
      "title": "bmo-brain",
      "version": env!("CARGO_PKG_VERSION"),
      "description": DESCRIPTION,
    },
    "servers": [{ "url": "/", "description": "the brain service" }],
    "security": [{ "bearer": [] }],
    "x-layer": "upstream",
    "x-stage": 2,
    "x-calls": [],
    "x-harness": { "plugin": "brain-harness", "mcp": "bmo-brain mcp", "entry": ENTRY },
    "x-events": EVENTS,
    "tags": tags.iter().map(|tag| json!({ "name": tag })).collect::<Vec<_>>(),
    "paths": paths,
    "components": {
      "schemas": generator.take_definitions(true),
      "responses": responses,
      "securitySchemes": {
        "bearer": {
          "type": "http",
          "scheme": "bearer",
          "description": "The brain's token, from the caller's own configuration.",
        },
      },
    },
    "x-tools": x_tools,
  })
}

/// The document as the repository keeps it: two-space indented JSON with a closing newline.
pub fn openapi_json() -> String {
  let mut text = serde_json::to_string_pretty(&openapi()).expect("a JSON value always renders");
  text.push('\n');
  text
}
