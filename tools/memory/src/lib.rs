//! The memory tool (docs/boundary-api.md, 「The memory tool」): one call handed by the tool host on standard
//! input, answered on standard output. It finds the brain through the data root's env file, the agent's brain
//! by the turn's agent, and reads and writes an agent's room memories with the brain's own routes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bmo_brain_client::{
  BrainClient, BrainKind, BrainsQuery, ClientFailure, MemoriesQuery, MemoryMove, MemoryStatus,
  MemoryVersion, NewMemory, Restoration, SubjectKey, SubjectsQuery,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The protocol version the tool speaks.
pub const PROTOCOL: u32 = 1;

/// The variables the tool reads of the session record: the data root, and the turn's agent and room.
pub const DATA_ROOT_VARIABLE: &str = "BMO_DATA_DIR";
pub const AGENT_VARIABLE: &str = "BMO_TURN_AGENT";
pub const CHAT_VARIABLE: &str = "BMO_TURN_CHAT";

/// The env file under the data root the brain's `tool-env` verb writes, and its variables.
pub const ENV_FILE: &str = "env/brain.env";
pub const ADDRESS_VARIABLE: &str = "BRAIN_ADDRESS";
pub const TOKEN_VARIABLE: &str = "BRAIN_TOKEN";

/// Who the tool writes as.
pub const ACTOR: &str = "agent-session";

/// The types a call may name.
pub const TYPES: [&str; 5] = ["promise", "check", "followup", "situation", "state"];

/// The entries the manifest declares, by their names.
pub const ENTRIES: [&str; 7] = [
  "memory_list",
  "memory_add",
  "memory_update",
  "memory_done",
  "memory_retire",
  "memory_revive",
  "memory_candidate",
];

/// What the host hands the executable.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handed {
  pub protocol: u32,
  pub kind: String,
  #[serde(default)]
  pub entry: String,
  #[serde(default)]
  pub arguments: Value,
  #[serde(default)]
  pub session: Session,
}

/// The parts of the session record the tool reads.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
  #[serde(default)]
  pub holder_name: String,
  #[serde(default)]
  pub variables: BTreeMap<String, String>,
}

/// A tool result.
#[derive(Clone, Debug, Serialize)]
pub struct Answer {
  pub content: Vec<Value>,
  #[serde(rename = "isError")]
  pub is_error: bool,
  #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
  pub structured: Option<Value>,
}

impl Answer {
  pub fn structured(value: Value) -> Self {
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    Self {
      content: vec![json!({ "type": "text", "text": text })],
      is_error: false,
      structured: Some(value),
    }
  }

  /// A success whose text is the caller's own words, beside the structured value.
  pub fn with_text(text: impl Into<String>, value: Value) -> Self {
    Self {
      content: vec![json!({ "type": "text", "text": text.into() })],
      is_error: false,
      structured: Some(value),
    }
  }

  pub fn refusal(text: impl Into<String>) -> Self {
    Self {
      content: vec![json!({ "type": "text", "text": text.into() })],
      is_error: true,
      structured: None,
    }
  }
}

/// A variable of this process's environment, else of the env file's lines.
fn variable(name: &str, file: &[(String, String)]) -> Option<String> {
  std::env::var(name)
    .ok()
    .or_else(|| file.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()))
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
}

/// The `KEY=value` lines of an env file, none where there is no such file.
fn env_lines(path: &Path) -> Vec<(String, String)> {
  std::fs::read_to_string(path)
    .map(|text| {
      text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
    })
    .unwrap_or_default()
}

/// The brain the call reaches, from the tool's environment or the data root's env file.
pub fn brain_at(session: &Session) -> Result<BrainClient, String> {
  // The data root: the record's, else this process's, else the deployment's conventional one under the home
  // the host passes through.
  let root = session
    .variables
    .get(DATA_ROOT_VARIABLE)
    .map(PathBuf::from)
    .or_else(|| std::env::var(DATA_ROOT_VARIABLE).ok().map(PathBuf::from))
    .or_else(|| std::env::var("HOME").ok().map(|home| PathBuf::from(home).join(".local/share/bmo/data")));
  let file = root.map(|root| env_lines(&root.join(ENV_FILE))).unwrap_or_default();
  let address = variable(ADDRESS_VARIABLE, &file).ok_or_else(|| {
    format!("no {ADDRESS_VARIABLE} in this tool's environment or in the data root's {ENV_FILE}")
  })?;
  let token = variable(TOKEN_VARIABLE, &file).unwrap_or_default();
  BrainClient::new(&address, token)
}

/// The agent whose brain the call writes: the turn's agent, else the record's holder.
pub fn agent_of(session: &Session) -> Result<String, String> {
  session
    .variables
    .get(AGENT_VARIABLE)
    .cloned()
    .filter(|agent| !agent.is_empty())
    .or_else(|| Some(session.holder_name.clone()).filter(|holder| !holder.is_empty()))
    .ok_or_else(|| format!("the call names no agent: neither {AGENT_VARIABLE} nor the record's holder"))
}

/// The turn's room, where the turn has one.
pub fn room_of(session: &Session) -> Option<String> {
  session.variables.get(CHAT_VARIABLE).cloned().filter(|room| !room.is_empty())
}

fn failed(why: ClientFailure) -> String {
  why.to_string()
}

/// The agent's brain: the one brain registered for the agent's key.
pub async fn brain_id(client: &BrainClient, agent: &str) -> Result<i64, String> {
  let page = client
    .brains(&BrainsQuery {
      kind: Some(BrainKind::Agent),
      agent_key: Some(agent.to_owned()),
      limit: Some(2),
      ..Default::default()
    })
    .await
    .map_err(failed)?;
  match page.items.as_slice() {
    [brain] => Ok(brain.id),
    [] => Err(format!("the agent {agent:?} has no brain")),
    _ => Err(format!("the agent {agent:?} has more than one brain; the tool cannot choose")),
  }
}

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

/// The arguments of a list.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ListArgs {
  #[serde(rename = "type", default)]
  pub kind: Option<String>,
  #[serde(default)]
  pub all: Option<bool>,
  #[serde(default)]
  pub global: Option<bool>,
  #[serde(default)]
  pub chat: Option<i64>,
}

/// The arguments of an add.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AddArgs {
  #[serde(rename = "type", default)]
  pub kind: Option<String>,
  #[serde(default)]
  pub action: Option<String>,
  #[serde(default)]
  pub condition: Option<String>,
  #[serde(default)]
  pub slug: Option<String>,
  #[serde(default)]
  pub global: Option<bool>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(rename = "for", default)]
  pub for_: Option<i64>,
  #[serde(default)]
  pub by: Option<i64>,
  #[serde(default)]
  pub message: Option<i64>,
}

/// The arguments of an update.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateArgs {
  #[serde(default)]
  pub memory: Option<String>,
  #[serde(default)]
  pub action: Option<String>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(default)]
  pub message: Option<i64>,
}

/// The arguments of a done.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct DoneArgs {
  #[serde(default)]
  pub memory: Option<String>,
  #[serde(default)]
  pub text: Option<String>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(default)]
  pub message: Option<i64>,
}

/// The arguments of a retire.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RetireArgs {
  #[serde(default)]
  pub memory: Option<String>,
  #[serde(default)]
  pub why: Option<String>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(default)]
  pub global: Option<bool>,
  #[serde(default)]
  pub message: Option<i64>,
}

/// The arguments of a revive.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReviveArgs {
  #[serde(default)]
  pub memory: Option<String>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(default)]
  pub global: Option<bool>,
  #[serde(default)]
  pub message: Option<i64>,
}

/// The arguments of a candidate.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CandidateArgs {
  #[serde(default)]
  pub memory: Option<String>,
  #[serde(default)]
  pub why: Option<String>,
  #[serde(default)]
  pub chat: Option<i64>,
  #[serde(default)]
  pub global: Option<bool>,
  #[serde(default)]
  pub message: Option<i64>,
}

/// One entry called with the arguments that matter to it, after they read.
#[derive(Clone, Debug)]
pub enum Call {
  List {
    kind: Option<String>,
    all: bool,
    global: bool,
    chat: Option<i64>,
  },
  Add {
    kind: String,
    action: String,
    condition: Option<String>,
    slug: Option<String>,
    global: bool,
    chat: Option<i64>,
    person: Option<i64>,
  },
  Update {
    memory: String,
    action: String,
  },
  Done {
    memory: String,
    text: Option<String>,
  },
  Retire {
    memory: String,
    why: String,
  },
  Revive {
    memory: String,
  },
  Candidate {
    memory: String,
    why: String,
  },
}

fn read<T: serde::de::DeserializeOwned>(arguments: &Value) -> Result<T, String> {
  serde_json::from_value(arguments.clone()).map_err(|why| format!("the arguments did not read: {why}"))
}

fn required(value: Option<String>, name: &str) -> Result<String, String> {
  value.filter(|value| !value.is_empty()).ok_or_else(|| format!("{name} is required"))
}

fn named_type(kind: &str) -> Result<(), String> {
  if TYPES.contains(&kind) {
    Ok(())
  } else {
    Err(format!("type is {kind:?}; it is promise, check, followup, situation or state"))
  }
}

fn kept(value: Option<String>) -> Option<String> {
  value.filter(|value| !value.is_empty())
}

/// The entry named by `entry`, its arguments read and the required ones checked.
pub fn parse(entry: &str, arguments: &Value) -> Result<Call, String> {
  let name = entry.rsplit('/').next().unwrap_or(entry);
  match name {
    "memory_list" => {
      let args: ListArgs = read(arguments)?;
      let kind = match args.kind {
        Some(kind) => {
          named_type(&kind)?;
          Some(kind)
        }
        None => None,
      };
      Ok(Call::List {
        kind,
        all: args.all.unwrap_or(false),
        global: args.global.unwrap_or(false),
        chat: args.chat,
      })
    }
    "memory_add" => {
      let args: AddArgs = read(arguments)?;
      let kind = required(args.kind, "type")?;
      named_type(&kind)?;
      let action = required(args.action, "action")?;
      Ok(Call::Add {
        kind,
        action,
        condition: kept(args.condition),
        slug: kept(args.slug),
        global: args.global.unwrap_or(false),
        chat: args.chat,
        person: args.for_,
      })
    }
    "memory_update" => {
      let args: UpdateArgs = read(arguments)?;
      Ok(Call::Update {
        memory: required(args.memory, "memory")?,
        action: required(args.action, "action")?,
      })
    }
    "memory_done" => {
      let args: DoneArgs = read(arguments)?;
      Ok(Call::Done {
        memory: required(args.memory, "memory")?,
        text: kept(args.text),
      })
    }
    "memory_retire" => {
      let args: RetireArgs = read(arguments)?;
      Ok(Call::Retire {
        memory: required(args.memory, "memory")?,
        why: required(args.why, "why")?,
      })
    }
    "memory_revive" => {
      let args: ReviveArgs = read(arguments)?;
      Ok(Call::Revive {
        memory: required(args.memory, "memory")?,
      })
    }
    "memory_candidate" => {
      let args: CandidateArgs = read(arguments)?;
      Ok(Call::Candidate {
        memory: required(args.memory, "memory")?,
        why: required(args.why, "why")?,
      })
    }
    other => Err(format!("the tool holds no entry {other}")),
  }
}

/// The room's subject, where one stands, by the chat id the room is keyed by.
async fn room_subject(client: &BrainClient, brain: i64, chat: &str) -> Result<Option<i64>, String> {
  let page = client
    .subjects(
      brain,
      &SubjectsQuery {
        kind: Some("room".to_owned()),
        key: Some(chat.to_owned()),
        limit: Some(2),
        ..Default::default()
      },
    )
    .await
    .map_err(failed)?;
  match page.items.as_slice() {
    [subject] => Ok(Some(subject.id)),
    [] => Ok(None),
    _ => Err(format!("the brain holds more than one room subject for the room {chat:?}")),
  }
}

/// The id of the memory a call names: the id itself where it is all digits, else the key among live rows.
async fn resolve_memory(client: &BrainClient, brain: i64, named: &str) -> Result<i64, String> {
  if !named.is_empty() && named.bytes().all(|byte| byte.is_ascii_digit()) {
    let id: i64 = named.parse().map_err(|_| format!("the memory id {named:?} does not read"))?;
    return match client.memory(brain, id).await {
      Ok(_) => Ok(id),
      Err(ClientFailure::Answered { status: 404, .. }) => Err(format!("no memory with id {id} was found")),
      Err(why) => Err(failed(why)),
    };
  }
  let page = client
    .memories(
      brain,
      &MemoriesQuery {
        key: Some(named.to_owned()),
        status: Some("active,pending".to_owned()),
        ..Default::default()
      },
    )
    .await
    .map_err(failed)?;
  match page.items.as_slice() {
    [memory] => Ok(memory.id),
    [] => Err(format!("no live memory has the key {named:?}")),
    _ => Err(format!("more than one live memory has the key {named:?}")),
  }
}

/// One list's lines, as the structured value and as the text; a line saying so where nothing stands.
fn lines_answer(lines: Vec<String>) -> Answer {
  let lines = if lines.is_empty() { vec!["No memories stand.".to_owned()] } else { lines };
  let text = lines.join("\n");
  Answer::with_text(text, json!({ "lines": lines }))
}

/// One entry called with its arguments under the session, answered.
pub async fn answer(handed: Handed) -> Result<Answer, String> {
  if handed.protocol != PROTOCOL {
    return Err(format!("the tool speaks protocol {PROTOCOL}, and was handed {}", handed.protocol));
  }
  if handed.kind != "call" {
    return Err(format!("the tool answers calls alone, not {}", handed.kind));
  }
  let name = handed.entry.rsplit('/').next().unwrap_or(&handed.entry).to_owned();
  if !ENTRIES.contains(&name.as_str()) {
    return Err(format!("the tool holds no entry {}", handed.entry));
  }
  let call = match parse(&name, &handed.arguments) {
    Ok(call) => call,
    Err(why) => return Ok(Answer::refusal(why)),
  };
  let client = match brain_at(&handed.session) {
    Ok(client) => client,
    Err(why) => return Ok(Answer::refusal(why)),
  };
  let agent = match agent_of(&handed.session) {
    Ok(agent) => agent,
    Err(why) => return Ok(Answer::refusal(why)),
  };
  let brain = match brain_id(&client, &agent).await {
    Ok(id) => id,
    Err(why) => return Ok(Answer::refusal(why)),
  };
  let answered = entry(&client, brain, &handed.session, call).await;
  Ok(match answered {
    Ok(answer) => answer,
    Err(why) => Answer::refusal(why),
  })
}

/// One entry called with its arguments, its failure answered as a refusal.
async fn entry(client: &BrainClient, brain: i64, session: &Session, call: Call) -> Result<Answer, String> {
  match call {
    Call::List {
      kind,
      all,
      global,
      chat,
    } => {
      let room = if global { None } else { chat.map(|id| id.to_string()).or_else(|| room_of(session)) };
      let subject_id = match room {
        Some(chat) => match room_subject(client, brain, &chat).await? {
          Some(id) => Some(id),
          None => return Ok(lines_answer(Vec::new())),
        },
        None => None,
      };
      let page = client
        .memories(
          brain,
          &MemoriesQuery {
            kind,
            status: if all { None } else { Some("active".to_owned()) },
            subject_id,
            ..Default::default()
          },
        )
        .await
        .map_err(failed)?;
      // A global row is one tied to no room: the brain lists by subject and has no filter for its absence,
      // so the roomless rows are picked here.
      let lines: Vec<String> = page
        .items
        .iter()
        .filter(|memory| !global || memory.subjects.iter().all(|about| about.kind != "room"))
        .map(|memory| format!("#{} [{}] {}", memory.id, memory.kind, memory.text))
        .collect();
      Ok(lines_answer(lines))
    }
    Call::Add {
      kind,
      action,
      condition,
      slug,
      global,
      chat,
      person,
    } => {
      let mut subjects = Vec::new();
      if !global && let Some(chat) = chat.map(|id| id.to_string()).or_else(|| room_of(session)) {
        subjects.push(subject("room", &chat));
      }
      if let Some(person) = person {
        subjects.push(subject("person", &person.to_string()));
      }
      let memory = client
        .memory_add(
          brain,
          &NewMemory {
            key: slug,
            kind: kind.clone(),
            text: action,
            layer_id: None,
            position: None,
            subject_ids: Vec::new(),
            subjects,
            importance: None,
            valid_from: None,
            recall_condition: condition,
            release_condition: None,
            status: Some(MemoryStatus::Active),
            actor_name: ACTOR.to_owned(),
            labels: Default::default(),
            internal_only: None,
            locked: None,
            standing: None,
          },
        )
        .await
        .map_err(failed)?;
      let status = memory.status.name();
      let line = format!("Memory #{} [{}] is {}.", memory.id, kind, status);
      Ok(Answer::with_text(
        line,
        json!({ "memoryId": memory.id, "type": kind, "status": status }),
      ))
    }
    Call::Update { memory, action } => {
      let id = resolve_memory(client, brain, &memory).await?;
      let version = client
        .memory_version(
          brain,
          id,
          &MemoryVersion {
            text: Some(action),
            actor_name: ACTOR.to_owned(),
            ..Default::default()
          },
        )
        .await
        .map_err(failed)?;
      Ok(Answer::structured(json!({
        "memoryId": version.id,
        "lineageId": version.lineage_id,
        "status": version.status.name(),
      })))
    }
    Call::Done { memory, text } => {
      let id = resolve_memory(client, brain, &memory).await?;
      let moved = client
        .memory_move(
          brain,
          id,
          &MemoryMove {
            status: MemoryStatus::Done,
            at: None,
            actor_name: ACTOR.to_owned(),
            reason: None,
            text,
            standing: None,
          },
        )
        .await
        .map_err(failed)?;
      Ok(Answer::structured(json!({ "memoryId": moved.id, "status": moved.status.name() })))
    }
    Call::Retire { memory, why } => {
      let id = resolve_memory(client, brain, &memory).await?;
      let moved = client
        .memory_move(
          brain,
          id,
          &MemoryMove {
            status: MemoryStatus::Invalidated,
            at: None,
            actor_name: ACTOR.to_owned(),
            reason: Some(why),
            text: None,
            standing: None,
          },
        )
        .await
        .map_err(failed)?;
      Ok(Answer::structured(json!({ "memoryId": moved.id, "status": moved.status.name() })))
    }
    Call::Revive { memory } => {
      let id = resolve_memory(client, brain, &memory).await?;
      let restored = client
        .memory_restore(
          brain,
          id,
          &Restoration {
            text: None,
            actor_name: ACTOR.to_owned(),
            standing: None,
          },
        )
        .await
        .map_err(failed)?;
      Ok(Answer::structured(json!({
        "memoryId": restored.id,
        "lineageId": restored.lineage_id,
        "status": restored.status.name(),
      })))
    }
    Call::Candidate { memory, why } => {
      let id = resolve_memory(client, brain, &memory).await?;
      let moved = client
        .memory_move(
          brain,
          id,
          &MemoryMove {
            status: MemoryStatus::Pending,
            at: None,
            actor_name: ACTOR.to_owned(),
            reason: Some(why),
            text: None,
            standing: None,
          },
        )
        .await
        .map_err(failed)?;
      Ok(Answer::structured(json!({ "memoryId": moved.id, "status": moved.status.name() })))
    }
  }
}

/// The one request on standard input answered on standard output; the exit code.
pub async fn answer_standard_io() -> std::process::ExitCode {
  let handed: Result<Handed, String> = serde_json::from_reader(std::io::stdin().lock())
    .map_err(|why| format!("the request did not read: {why}"));
  let said = match handed {
    Ok(handed) => answer(handed).await,
    Err(why) => Err(why),
  };
  match said.and_then(|answer| {
    serde_json::to_writer(std::io::stdout().lock(), &answer)
      .map_err(|why| format!("the answer could not be written: {why}"))
  }) {
    Ok(()) => std::process::ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("memory-tool: {why}");
      std::process::ExitCode::FAILURE
    }
  }
}
