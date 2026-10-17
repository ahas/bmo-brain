//! The memory tools, the entries of the `memory` tool (docs/boundary-api.md, 「Its tools」; docs/memories.md): a
//! memory row written, moved on, checked off, taken down, put back, raised or listed, a note kept for the room's
//! digest and a watch set, each answered through the brain's route for it. A call reads its turn afresh from the
//! session's record and hands it to the route, which files the row under the turn's agent in the turn's room.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tool_call::{Answer, Asked, Brain, Entry, Handed, Said, Session, Turn, input_schema};

/// The catalogue's capability every entry is shown by.
const CATALOGUE: &[&str] = &["room.tools"];

/// The output schema every reading tool declares: the reading, one line each.
pub fn reading_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "lines": { "type": "array", "items": { "type": "string" }, "description": "The reading, one line each." },
    },
    "required": ["lines"],
  })
}

const fn entry(
  name: &'static str,
  description: &'static str,
  input: &'static str,
  input_schema: fn() -> Value,
  grant: Option<&'static str>,
) -> Entry {
  Entry {
    namespace: "bmo",
    name,
    title: None,
    description,
    input,
    input_schema,
    output: None,
    capabilities: CATALOGUE,
    hints: &[],
    grant,
  }
}

/// Every entry the executable answers, as its manifest declares them.
pub const ENTRIES: &[Entry] = &[
  Entry {
    output: Some(("reading.json", reading_schema)),
    hints: &[("read_only", true)],
    ..entry(
      "memory_list",
      "A room's memory rows, or the global ones; open rows unless all are asked for.",
      "memory_list.json",
      input_schema::<MemoryListInput>,
      None,
    )
  },
  entry(
    "memory_add",
    "Write a memory row: a promise, a check, a follow-up, a situation rule, or the state of something under way.",
    "memory_add.json",
    input_schema::<MemoryAddInput>,
    Some("memories"),
  ),
  entry(
    "memory_update",
    "Move a room's open state on: its text becomes where the thing now stands.",
    "memory_update.json",
    input_schema::<MemoryUpdateInput>,
    Some("memories"),
  ),
  entry(
    "memory_done",
    "Check off a room's errand row, or release its state.",
    "memory_done.json",
    input_schema::<MemoryDoneInput>,
    Some("memories"),
  ),
  entry(
    "memory_retire",
    "Take a memory row down, saying why.",
    "memory_retire.json",
    input_schema::<MemoryRetireInput>,
    Some("memories"),
  ),
  entry(
    "memory_revive",
    "Put a memory row that was taken down back.",
    "memory_revive.json",
    input_schema::<MemoryReviveInput>,
    Some("memories"),
  ),
  entry(
    "memory_candidate",
    "Raise a memory row as a candidate for a person to take down on the console.",
    "memory_candidate.json",
    input_schema::<MemoryCandidateInput>,
    Some("memories"),
  ),
  entry(
    "note_add",
    "Keep a note for the room's digest, for that room or for every room. Sends nothing.",
    "note_add.json",
    input_schema::<NoteAddInput>,
    Some("notes"),
  ),
  entry(
    "watch_set",
    "Set a watch that comes back to the room when a check passes, or a timer when no check can say it.",
    "watch_set.json",
    input_schema::<WatchSetInput>,
    Some("watch"),
  ),
];

fn off(flag: &bool) -> bool {
  !*flag
}

/// The message a call acts on: the id of the person's message in the turn that asked for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActedOn(pub i64);

impl JsonSchema for ActedOn {
  fn inline_schema() -> bool {
    true
  }

  fn schema_name() -> Cow<'static, str> {
    "ActedOn".into()
  }

  fn json_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
      "type": "integer",
      "description": "The message this call acts on: the id of the person's message in this turn that asked for it. Omitted only in a turn whose every new message is the owner's, or that no person's message opened.",
    })
  }
}

/// The kinds a memory row is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(inline)]
pub enum MemoryType {
  Promise,
  Check,
  Followup,
  Situation,
  State,
}

/// Which rooms a global row reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(inline)]
pub enum Reach {
  Owner,
  Group,
  All,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryListInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// A row tied to no room.
  #[serde(default, skip_serializing_if = "off")]
  pub global: bool,
  /// One kind of row.
  #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<MemoryType>,
  /// Finished and taken-down rows too.
  #[serde(default, skip_serializing_if = "off")]
  pub all: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryAddInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// A row tied to no room.
  #[serde(default, skip_serializing_if = "off")]
  pub global: bool,
  /// What kind of row.
  #[serde(rename = "type")]
  pub kind: MemoryType,
  /// What attaches: what to do.
  pub action: String,
  /// When it attaches, for the matcher to judge.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub condition: Option<String>,
  /// The recorded event it waits for: deployed[:<item>], job-ended[:<item>] or queue-changed.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub on: Option<String>,
  /// A short read-only check that exits 0 once what it waits for has happened; sets a watch.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub until: Option<String>,
  /// The watch's short name.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  /// Seconds between checks.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub every: Option<i64>,
  /// Minutes the watch waits at most.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub within: Option<i64>,
  /// The row's name, lower-case letters, digits and hyphens.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub slug: Option<String>,
  /// The person the row is for, by user id.
  #[serde(rename = "for", default, skip_serializing_if = "Option::is_none")]
  pub for_user: Option<i64>,
  /// Who set it, by user id.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub by: Option<i64>,
  /// The message it came from.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<i64>,
  /// The item whose unfinished strand it carries.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub todo: Option<i64>,
  /// Which rooms a global row reaches.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reach: Option<Reach>,
  /// The slug of the row this one takes the place of.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub replaces: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryUpdateInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// The row's id or slug.
  pub memory: String,
  /// Where it stands now.
  pub action: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryDoneInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// The row's id or slug.
  pub memory: String,
  /// How it ended.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRetireInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// A row tied to no room.
  #[serde(default, skip_serializing_if = "off")]
  pub global: bool,
  /// The row's id or slug.
  pub memory: String,
  /// Why it comes down.
  pub why: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryReviveInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// A row tied to no room.
  #[serde(default, skip_serializing_if = "off")]
  pub global: bool,
  /// The row's id or slug.
  pub memory: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryCandidateInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// A row tied to no room.
  #[serde(default, skip_serializing_if = "off")]
  pub global: bool,
  /// The row's id or slug.
  pub memory: String,
  /// Why it should come down.
  pub why: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteAddInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// What to keep.
  pub text: String,
  /// Who it is about or from, by user id.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub author: Option<i64>,
  /// For every room: a way of working, know-how or a fact that holds wherever the work is done.
  #[serde(default, skip_serializing_if = "off")]
  pub global: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchSetInput {
  /// The room, by chat id, where it is not this turn's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chat: Option<i64>,
  /// A short read-only check that exits 0 once it has happened.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub until: Option<String>,
  /// For a timer: 'YYYY-MM-DD HH:MM' on the Seoul clock.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<String>,
  /// The watch's short name.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  /// What comes back, and for whom.
  pub note: String,
  /// Seconds between checks.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub every: Option<i64>,
  /// Minutes it waits at most.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub within: Option<i64>,
  /// The memory row it serves.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub item: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub message: Option<ActedOn>,
}

// What a route answered, as a tool answers it: its lines, or its refusal after the lines it wrote first, `heading`
// set before the refusal's sentence.
fn said(answered: Result<Said, String>, heading: &str) -> Answer {
  match answered {
    Ok(Said::Reading(lines)) => Answer::text(lines.join("\n")),
    Ok(Said::Refused { message, lines, .. }) => Answer::refusal(
      lines.into_iter().chain([format!("{heading}{message}")]).collect::<Vec<_>>().join("\n"),
    ),
    Ok(Said::Object(_)) => Answer::failure("the brain answered no reading"),
    Err(why) => Answer::failure(why),
  }
}

// The route's body: the call's own arguments, under the turn it is made in and the message it acts on.
fn body(turn: &Turn, message: Option<i64>, arguments: impl Serialize) -> Value {
  let mut body = serde_json::to_value(arguments).unwrap_or_else(|_| json!({}));

  body["turn"] = json!({ "agent": turn.agent, "chatId": turn.chat, "messageId": message });
  body
}

fn read<T: serde::de::DeserializeOwned>(name: &str, arguments: Value) -> Result<T, Answer> {
  serde_json::from_value(arguments)
    .map_err(|why| Answer::failure(format!("invalid arguments: {name}: {why}")))
}

/// One call of an entry, by its name, with its arguments, answered through the brain the session's record reaches.
pub async fn call(session: &Session, name: &str, arguments: Value) -> Answer {
  let brain = match Brain::of(session) {
    Ok(brain) => brain,
    Err(why) => return Answer::failure(why),
  };

  call_on(&brain, session, name, arguments).await
}

/// One call of an entry answered through this brain.
pub async fn call_on(brain: &Brain, session: &Session, name: &str, arguments: Value) -> Answer {
  let turn = Turn::of(session);
  let asked = async {
    Ok::<Answer, Answer>(match name {
      "memory_list" => {
        let input: MemoryListInput = read(name, arguments)?;

        match brain.post("/memory/list", &body(&turn, None, &input)).await {
          Ok(Said::Reading(lines)) => Answer::structured(json!({ "lines": lines })),
          other => said(other, ""),
        }
      }
      "memory_add" => {
        let input: MemoryAddInput = read(name, arguments)?;

        said(brain.post("/memory/add", &body(&turn, input.message, &input)).await, "")
      }
      "memory_update" => {
        let input: MemoryUpdateInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/memory/update", &body(&turn, message, &input)).await, "")
      }
      "memory_done" => {
        let input: MemoryDoneInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/memory/done", &body(&turn, message, &input)).await, "")
      }
      "memory_retire" => {
        let input: MemoryRetireInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/memory/retire", &body(&turn, message, &input)).await, "")
      }
      "memory_revive" => {
        let input: MemoryReviveInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/memory/revive", &body(&turn, message, &input)).await, "")
      }
      "memory_candidate" => {
        let input: MemoryCandidateInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/memory/candidate", &body(&turn, message, &input)).await, "")
      }
      "note_add" => {
        let input: NoteAddInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/notes/add", &body(&turn, message, &input)).await, "")
      }
      "watch_set" => {
        let input: WatchSetInput = read(name, arguments)?;
        let message = input.message.map(|acted| acted.0);

        said(brain.post("/watches/set", &body(&turn, message, &input)).await, "watch: ")
      }
      other => Answer::failure(format!("this executable holds no entry {other}")),
    })
  };

  asked.await.unwrap_or_else(|refused| refused)
}

/// One request answered: a call of an entry by its full name, resource requests answered as a tool that serves
/// none, and any other kind refused.
pub async fn answer(handed: Handed) -> Result<Value, String> {
  let answered = match handed.asked {
    Asked::Call {
      entry: asked,
      arguments,
    } => match asked.strip_prefix("bmo/") {
      Some(name) if ENTRIES.iter().any(|entry| entry.name == name) => {
        call(&handed.session, name, arguments).await
      }
      _ => Answer::failure(format!("this executable holds no entry {asked}")),
    },
    Asked::ResourcesList => return Ok(json!({ "resources": [] })),
    Asked::ResourcesRead { .. } => return Ok(json!({ "error": "this executable serves no resources" })),
    Asked::Other => return Err("this executable answers calls alone".to_owned()),
  };

  serde_json::to_value(answered).map_err(|why| format!("the answer could not be written: {why}"))
}
