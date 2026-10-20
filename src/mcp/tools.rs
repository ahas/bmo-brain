//! The harness tools: exactly the ones the brain's plugin offers, each a thin call of one route through the
//! brain's client (docs/boundary-api.md, 「The harness plugin」). The MCP server lists them and the OpenAPI
//! document states them under `x-tools`, both from this one table.

use std::future::Future;
use std::pin::Pin;

use bmo_brain_client::{
  BrainClient, ClientFailure, FragmentWrite, FragmentsQuery, KnowledgeQuery, MemoriesQuery, MemoryChange,
  NewLock, NewMemory, NewNote, NewReminder, NewWatch, NotesQuery, RemindersQuery, WatchChange,
  WatchesQuery,
};
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
  change: MemoryChange,
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

/// One lock, by its id.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LockId {
  lock_id: i64,
}

/// The seventeen tools, in the order the plugin lists them.
pub fn tools() -> Vec<BrainTool> {
  vec![
    tool(
      "brain_memories",
      "Memory rows by scope, scope prefix, subject, kind, state, label; standing rows, rows due.",
      "GET /memories",
      |client, query: MemoriesQuery| async move { client.memories(&query).await },
    ),
    tool(
      "brain_memory_add",
      "Add a memory row.",
      "POST /memories",
      |client, body: NewMemory| async move { client.memory_add(&body).await },
    ),
    tool(
      "brain_memory",
      "One memory row and the watches on it.",
      "GET /memories/{memoryId}",
      |client, MemoryId { memory_id }| async move { client.memory(memory_id).await },
    ),
    tool(
      "brain_memory_update",
      "Change a row: its words, or its state (done, retired, candidate, off, standing again).",
      "PATCH /memories/{memoryId}",
      |client, MemoryUpdate { memory_id, change }| async move { client.memory_update(memory_id, &change).await },
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
