//! The harness tools: exactly the ones the brain's plugin offers, each a thin call of one route through the
//! brain's client (docs/boundary-api.md, 「The harness plugin」). The MCP server lists them and the OpenAPI
//! document states them under `x-tools`, both from this one table.

use std::future::Future;
use std::pin::Pin;

use bmo_brain_client::{
  BrainClient, BrainsQuery, ClientFailure, Confirmations, Fold, FragmentWrite, FragmentsQuery, Hits,
  KnowledgeQuery, LayersQuery, MemoriesQuery, MemoryMove, MemoryVersion, NewBrain, NewLayer, NewLock,
  NewMemory, NewNote, NewReminder, NewScopedMemory, NewSubject, NewWatch, NotesQuery, RecallAsk,
  RemindersQuery, ScopedMemoriesQuery, ScopedMemoryChange, SubjectsQuery, WatchChange, WatchesQuery,
};
use bmo_brain_client::{Consolidated, Learning, SimilarAsk, UnconsolidatedQuery};
use schemars::generate::SchemaGenerator;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a tool's call came to short of the route's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolFailure {
  /// The arguments are not the shape the tool takes.
  Arguments(String),
  /// The call reached no answer of the route's shape.
  Client(ClientFailure),
}

impl ToolFailure {
  /// The failure as the tool error's content: the route's status, code and sentence, or why nothing answered.
  pub fn content(&self) -> Value {
    match self {
      Self::Arguments(why) => json!({ "code": "bad-request", "error": why }),
      Self::Client(ClientFailure::Answered {
        status,
        answer: Some(answer),
      }) => json!({ "status": status, "code": answer.code, "error": answer.error }),
      Self::Client(failure @ ClientFailure::Answered { status, .. }) => {
        json!({ "status": status, "error": failure.to_string() })
      }
      Self::Client(failure) => json!({ "error": failure.to_string() }),
    }
  }
}

/// A tool's answer: the route's answer as JSON, or why there is none.
pub type Answered = Pin<Box<dyn Future<Output = Result<Value, ToolFailure>> + Send>>;

type Call = Box<dyn Fn(BrainClient, Value) -> Answered + Send + Sync>;

/// One tool: its name, what it does, the route it calls, its input and output, and the call itself.
pub struct BrainTool {
  pub name: &'static str,
  pub description: &'static str,
  /// The route the tool calls, as its method and path.
  pub route: &'static str,
  input: fn(&mut SchemaGenerator) -> Schema,
  output: fn(&mut SchemaGenerator) -> Schema,
  call: Call,
}

impl BrainTool {
  /// The tool's input, its nested types referred to where `generator` keeps its definitions.
  pub fn input(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.input)(generator)
  }

  /// The tool's output, referred to where `generator` keeps its definitions.
  pub fn output(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.output)(generator)
  }

  /// Calls the tool's route on the brain `client` reaches, with the tool's arguments.
  pub fn call(&self, client: BrainClient, arguments: Value) -> Answered {
    (self.call)(client, arguments)
  }
}

/// A tool whose input is `I` and whose answer is `O`, calling `call` with the arguments read as `I`.
fn tool<I, O, F, Fut>(
  name: &'static str,
  description: &'static str,
  route: &'static str,
  call: F,
) -> BrainTool
where
  I: DeserializeOwned + JsonSchema,
  O: Serialize + JsonSchema,
  F: Fn(BrainClient, I) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = Result<O, ClientFailure>> + Send + 'static,
{
  BrainTool {
    name,
    description,
    route,
    input: I::json_schema,
    output: SchemaGenerator::subschema_for::<O>,
    call: Box::new(move |client, arguments| {
      let input = match serde_json::from_value::<I>(arguments) {
        Ok(input) => input,
        Err(error) => {
          let why = format!("The arguments are not the shape this tool takes: {error}.");
          return Box::pin(std::future::ready(Err(ToolFailure::Arguments(why))));
        }
      };
      let answer = call(client, input);
      Box::pin(async move {
        let answer = answer.await.map_err(ToolFailure::Client)?;
        Ok(serde_json::to_value(answer).expect("a route's answer always renders as JSON"))
      })
    }),
  }
}

/// What a route that answers no content answers through a tool: an empty object.
#[derive(Serialize, JsonSchema)]
#[schemars(inline)]
struct Nothing {}

/// One memory row, by its id.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MemoryId {
  memory_id: i64,
}

/// A row and the change to it.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MemoryUpdate {
  memory_id: i64,
  #[serde(flatten)]
  change: ScopedMemoryChange,
}

/// A watch and the move it makes.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct WatchUpdate {
  watch_id: i64,
  #[serde(flatten)]
  change: WatchChange,
}

/// One reminder, by its id.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ReminderId {
  reminder_id: i64,
}

/// A fragment's key and the fragment written whole.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FragmentPut {
  fragment_key: String,
  body: FragmentWrite,
}

/// One brain, by its id.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct BrainId {
  brain_id: i64,
}

/// One memory of a brain, by their ids.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct BrainMemoryId {
  brain_id: i64,
  memory_id: i64,
}

/// A brain and what a route of it takes.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Within<T> {
  brain_id: i64,
  #[serde(flatten)]
  body: T,
}

/// A memory of a brain and the version written over it.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct VersionOf {
  brain_id: i64,
  memory_id: i64,
  #[serde(flatten)]
  version: MemoryVersion,
}

/// A memory of a brain and the move it makes.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MoveOf {
  brain_id: i64,
  memory_id: i64,
  #[serde(flatten)]
  change: MemoryMove,
}

/// The rendered persona, as a tool answers it.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PersonaText {
  text: String,
}

/// One lock, by its id.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LockId {
  lock_id: i64,
}

/// The tools, in the order the plugin lists them: the brains' first, then the scope routes'.
pub fn tools() -> Vec<BrainTool> {
  vec![
    tool(
      "brain_brains",
      "The brains, by account and agent.",
      "GET /brains",
      |client, query: BrainsQuery| async move { client.brains(&query).await },
    ),
    tool(
      "brain_brain_add",
      "Make a brain for an account and an agent that have none.",
      "POST /brains",
      |client, body: NewBrain| async move { client.brain_add(&body).await },
    ),
    tool(
      "brain_subjects",
      "A brain's subjects, by type, key and words of the description.",
      "GET /brains/{brainId}/subjects",
      |client, Within { brain_id, body }: Within<SubjectsQuery>| async move {
        client.subjects(brain_id, &body).await
      },
    ),
    tool(
      "brain_subject_add",
      "Add a subject, or the one standing at its type and key.",
      "POST /brains/{brainId}/subjects",
      |client, Within { brain_id, body }: Within<NewSubject>| async move {
        client.subject_add(brain_id, &body).await
      },
    ),
    tool(
      "brain_layers",
      "A brain's layers, by position.",
      "GET /brains/{brainId}/layers",
      |client, Within { brain_id, body }: Within<LayersQuery>| async move {
        client.layers(brain_id, &body).await
      },
    ),
    tool(
      "brain_layer_add",
      "Add a layer, or the one standing at its name.",
      "POST /brains/{brainId}/layers",
      |client, Within { brain_id, body }: Within<NewLayer>| async move {
        client.layer_add(brain_id, &body).await
      },
    ),
    tool(
      "brain_memories_list",
      "A brain's memories, by type, status, layer, subject, key, lineage, label and words.",
      "GET /brains/{brainId}/memories",
      |client, Within { brain_id, body }: Within<MemoriesQuery>| async move {
        client.memories(brain_id, &body).await
      },
    ),
    tool(
      "brain_memory_write",
      "Write a memory as a first version.",
      "POST /brains/{brainId}/memories",
      |client, Within { brain_id, body }: Within<NewMemory>| async move {
        client.memory_add(brain_id, &body).await
      },
    ),
    tool(
      "brain_memory_read",
      "One memory in one version, with its subjects and strengths.",
      "GET /brains/{brainId}/memories/{memoryId}",
      |client,
       BrainMemoryId {
         brain_id,
         memory_id,
       }| async move { client.memory(brain_id, memory_id).await },
    ),
    tool(
      "brain_memory_version",
      "Write a new version of a memory, superseding the one named.",
      "POST /brains/{brainId}/memories/{memoryId}/versions",
      |client,
       VersionOf {
         brain_id,
         memory_id,
         version,
       }| async move { client.memory_version(brain_id, memory_id, &version).await },
    ),
    tool(
      "brain_memory_move",
      "Move a memory's status: approved, done or invalidated.",
      "PATCH /brains/{brainId}/memories/{memoryId}",
      |client,
       MoveOf {
         brain_id,
         memory_id,
         change,
       }| async move { client.memory_move(brain_id, memory_id, &change).await },
    ),
    tool(
      "brain_hits",
      "Record a hand-out: each row hit, and marked for the session named.",
      "POST /brains/{brainId}/hits",
      |client, Within { brain_id, body }: Within<Hits>| async move { client.hits_record(brain_id, &body).await },
    ),
    tool(
      "brain_confirmations",
      "Record confirmations: each row's storage strength raised by the gap since its last.",
      "POST /brains/{brainId}/confirmations",
      |client, Within { brain_id, body }: Within<Confirmations>| async move {
        client.confirmations_record(brain_id, &body).await
      },
    ),
    tool(
      "brain_types",
      "The types in use, counted, the reserved ones first.",
      "GET /brains/{brainId}/types",
      |client, BrainId { brain_id }| async move { client.types(brain_id).await },
    ),
    tool(
      "brain_recall",
      "What a turn is handed: the room's core and the searched part, under the budget.",
      "POST /brains/{brainId}/recall",
      |client, Within { brain_id, body }: Within<RecallAsk>| async move { client.recall(brain_id, &body).await },
    ),
    tool(
      "brain_fold",
      "A session folded or handed off: its marks cleared.",
      "POST /brains/{brainId}/folds",
      |client, Within { brain_id, body }: Within<Fold>| async move { client.fold(brain_id, &body).await },
    ),
    tool(
      "brain_learn",
      "Apply a learning whole: confirm the memories it repeats, version the one it strengthens, close the ones it contradicts (unless heard), and write it as a fact or a rumor by its source.",
      "POST /brains/{brainId}/learnings",
      |client, Within { brain_id, body }: Within<Learning>| async move { client.learn(brain_id, &body).await },
    ),
    tool(
      "brain_similar",
      "The live memories most like a candidate: read these before brain_learn and judge the candidate against them.",
      "POST /brains/{brainId}/similar",
      |client, Within { brain_id, body }: Within<SimilarAsk>| async move {
        client.similar(brain_id, &body).await
      },
    ),
    tool(
      "brain_unconsolidated",
      "The episodes not yet consolidated, most important first.",
      "GET /brains/{brainId}/unconsolidated",
      |client, Within { brain_id, body }: Within<UnconsolidatedQuery>| async move {
        client.unconsolidated(brain_id, &body).await
      },
    ),
    tool(
      "brain_consolidated",
      "Mark episodes consolidated after writing what they come to.",
      "POST /brains/{brainId}/consolidated",
      |client, Within { brain_id, body }: Within<Consolidated>| async move {
        client.consolidated(brain_id, &body).await
      },
    ),
    tool(
      "brain_persona",
      "The persona rendered as text.",
      "GET /brains/{brainId}/persona",
      |client, BrainId { brain_id }| async move {
        client.persona(brain_id).await.map(|text| PersonaText { text })
      },
    ),
    tool(
      "brain_memories",
      "Memory rows by scope, scope prefix, subject, kind, state, label; standing rows, rows due.",
      "GET /memories",
      |client, query: ScopedMemoriesQuery| async move { client.scoped_memories(&query).await },
    ),
    tool(
      "brain_memory_add",
      "Add a memory row.",
      "POST /memories",
      |client, body: NewScopedMemory| async move { client.scoped_memory_add(&body).await },
    ),
    tool(
      "brain_memory",
      "One memory row and the watches on it.",
      "GET /memories/{memoryId}",
      |client, MemoryId { memory_id }| async move { client.scoped_memory(memory_id).await },
    ),
    tool(
      "brain_memory_update",
      "Change a row: its words, or its state (done, retired, candidate, off, standing again).",
      "PATCH /memories/{memoryId}",
      |client, MemoryUpdate { memory_id, change }| async move {
        client.scoped_memory_update(memory_id, &change).await
      },
    ),
    tool(
      "brain_notes",
      "Notes by scope and subject.",
      "GET /notes",
      |client, query: NotesQuery| async move { client.notes(&query).await },
    ),
    tool("brain_note_add", "Add a note.", "POST /notes", |client, body: NewNote| async move {
      client.note_add(&body).await
    }),
    tool(
      "brain_watches",
      "Watches by scope, setter, holder or state.",
      "GET /watches",
      |client, query: WatchesQuery| async move { client.watches(&query).await },
    ),
    tool(
      "brain_watch_add",
      "Set a watch.",
      "POST /watches",
      |client, body: NewWatch| async move { client.watch_add(&body).await },
    ),
    tool(
      "brain_watch_update",
      "Move a watch: fired (an external condition held), claimed by a holder (409 when held), ended, cancelled.",
      "PATCH /watches/{watchId}",
      |client, WatchUpdate { watch_id, change }| async move { client.watch_update(watch_id, &change).await },
    ),
    tool(
      "brain_reminders",
      "Reminders by scope, due before a time.",
      "GET /reminders",
      |client, query: RemindersQuery| async move { client.reminders(&query).await },
    ),
    tool(
      "brain_reminder_add",
      "Set a reminder; reminder.due is published at its time.",
      "POST /reminders",
      |client, body: NewReminder| async move { client.reminder_add(&body).await },
    ),
    tool(
      "brain_reminder_cancel",
      "Cancel a reminder.",
      "DELETE /reminders/{reminderId}",
      |client, ReminderId { reminder_id }| async move {
        client.reminder_cancel(reminder_id).await.map(|()| Nothing {})
      },
    ),
    tool(
      "brain_fragments",
      "Fragments by kind, scope, subject, library, words, state; kind=person reads person sheets.",
      "GET /fragments",
      |client, query: FragmentsQuery| async move { client.fragments(&query).await },
    ),
    tool(
      "brain_fragment_put",
      "Write a fragment.",
      "PUT /fragments/{fragmentKey}",
      |client, FragmentPut { fragment_key, body }| async move {
        client.fragment_put(&fragment_key, &body).await
      },
    ),
    tool(
      "brain_knowledge",
      "Knowledge for a set of scopes: fragments by slugs and kinds, and the standing rows.",
      "GET /knowledge",
      |client, query: KnowledgeQuery| async move { client.knowledge(&query).await },
    ),
    tool(
      "brain_lock",
      "Lock the store for editing; 409 while another holds it.",
      "POST /locks",
      |client, body: NewLock| async move { client.lock(&body).await },
    ),
    tool(
      "brain_unlock",
      "Let a lock go.",
      "DELETE /locks/{lockId}",
      |client, LockId { lock_id }| async move { client.unlock(lock_id).await.map(|()| Nothing {}) },
    ),
  ]
}
