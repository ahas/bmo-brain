//! The brain's tool (docs/boundary-api.md, 「Its tool」): one call handed by the tool host on standard input,
//! answered on standard output. It finds the brain through the data root's env file, the agent's brain by the
//! turn's agent, and answers each entry with one call of the brain's client; a reading also asks the platform's
//! base, unless the record turns it off.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bmo_brain_client::{
  BrainClient, BrainKind, BrainsQuery, ClientFailure, Consolidated, DuplicatesQuery, Fold, Learning,
  MemoriesQuery, Memory, MemoryMerge, MemoryMove, MemoryStatus, NewMemory, SimilarAsk, Source, SubjectKey,
  UnconsolidatedQuery,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The protocol version the tool speaks.
pub const PROTOCOL: u32 = 1;

/// The variables the tool reads of the session record: the data root, and the turn's agent and room.
pub const DATA_ROOT_VARIABLE: &str = "BMO_DATA_DIR";
pub const AGENT_VARIABLE: &str = "BMO_TURN_AGENT";
/// The variable naming the chat agent a run works for, whose brain its calls read and write.
pub const BRAIN_AGENT_VARIABLE: &str = "BMO_BRAIN_AGENT";
pub const CHAT_VARIABLE: &str = "BMO_TURN_CHAT";
/// The variable turning the base off for the session's readings, where it says `off`.
pub const BASE_VARIABLE: &str = "BMO_BRAIN_BASE";

/// The account key and the agent key the base is filed under, an org brain (docs/design.md, 「The base」).
pub const BASE_KEY: &str = "platform";

/// The label naming the brain a memory of the base was raised from.
pub const SOURCE_BRAIN_LABEL: &str = "sourceBrainId";

/// The env file under the data root the brain's `tool-env` verb writes, and its variables.
pub const ENV_FILE: &str = "env/brain.env";
pub const ADDRESS_VARIABLE: &str = "BRAIN_ADDRESS";
pub const TOKEN_VARIABLE: &str = "BRAIN_TOKEN";

/// Who the tool writes as.
pub const ACTOR: &str = "agent-session";

/// The entries the manifest declares, by their names.
pub const ENTRIES: [&str; 10] = [
  "brain_episode",
  "brain_similar",
  "brain_learn",
  "brain_move",
  "brain_list",
  "brain_unconsolidated",
  "brain_consolidated",
  "brain_fold",
  "brain_duplicates",
  "brain_merge",
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

/// The agent whose brain the call writes: the chat agent the run works for, else the turn's agent, else the
/// record's holder.
pub fn agent_of(session: &Session) -> Result<String, String> {
  let named = |variable: &str| session.variables.get(variable).cloned().filter(|agent| !agent.is_empty());

  named(BRAIN_AGENT_VARIABLE)
    .or_else(|| named(AGENT_VARIABLE))
    .or_else(|| Some(session.holder_name.clone()).filter(|holder| !holder.is_empty()))
    .ok_or_else(|| {
      format!(
        "the call names no agent: neither {BRAIN_AGENT_VARIABLE}, {AGENT_VARIABLE} nor the record's holder"
      )
    })
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

/// Whether the session's readings take the base in: unless the record says `off`.
pub fn reads_base(session: &Session) -> bool {
  session.variables.get(BASE_VARIABLE).is_none_or(|said| said.trim() != "off")
}

/// The base, where one stands; none where none does or it cannot be read.
pub async fn base_id(client: &BrainClient) -> Option<i64> {
  let page = client
    .brains(&BrainsQuery {
      kind: Some(BrainKind::Org),
      account_key: Some(BASE_KEY.to_owned()),
      agent_key: Some(BASE_KEY.to_owned()),
      limit: Some(1),
      ..Default::default()
    })
    .await
    .ok()?;
  page.items.first().map(|brain| brain.id)
}

/// Whether a memory of the base is one to hand this brain: active, and not raised from this brain, which holds
/// its source.
fn handed_from_base(memory: &Memory, brain: i64) -> bool {
  memory.status == MemoryStatus::Active
    && memory.labels.get(SOURCE_BRAIN_LABEL).is_none_or(|source| source != &brain.to_string())
}

fn listed_memory(m: &Memory) -> Value {
  json!({
    "id": m.id, "type": m.kind, "text": m.text, "status": m.status, "importance": m.importance,
    "subjects": m.subjects, "validFrom": m.valid_from, "validTo": m.valid_to,
    "recallCondition": m.recall_condition, "releaseCondition": m.release_condition,
  })
}

fn liked_memory(m: &Memory) -> Value {
  json!({
    "id": m.id, "type": m.kind, "text": m.text, "importance": m.importance,
    "subjects": m.subjects, "validFrom": m.valid_from, "status": m.status,
  })
}

/// A subject of a brain found by its type and key: its id, none where the brain holds none, or why it was not read.
async fn subject_in(
  client: &BrainClient,
  brain: i64,
  named: &SubjectKey,
) -> Result<Option<i64>, ClientFailure> {
  let page = client
    .subjects(
      brain,
      &bmo_brain_client::SubjectsQuery {
        kind: Some(named.kind.clone()),
        key: Some(named.key.clone()),
        ..Default::default()
      },
    )
    .await?;
  Ok(page.items.first().map(|found| found.id))
}

/// The same list asked of the base, for the agent's brain: its active rows less those raised from that brain;
/// none where no base stands, the list names a subject the base holds none of, or the base cannot be read.
async fn base_listed(
  client: &BrainClient,
  brain: i64,
  named: Option<&SubjectKey>,
  query: &MemoriesQuery,
) -> Vec<Value> {
  let Some(base) = base_id(client).await else {
    return vec![];
  };
  let subject_id = match named {
    None => None,
    Some(named) => match subject_in(client, base, named).await {
      Ok(Some(id)) => Some(id),
      Ok(None) | Err(_) => return vec![],
    },
  };
  let asked = MemoriesQuery {
    status: Some(MemoryStatus::Active.name().to_owned()),
    subject_id,
    ..query.clone()
  };
  match client.memories(base, &asked).await {
    Ok(page) => page.items.iter().filter(|m| handed_from_base(m, brain)).map(listed_memory).collect(),
    Err(_) => vec![],
  }
}

/// The same ask of the base, for the agent's brain: its active rows less those raised from that brain; none
/// where no base stands or it cannot be read.
async fn base_liked(client: &BrainClient, brain: i64, ask: &SimilarAsk) -> Vec<Value> {
  let Some(base) = base_id(client).await else {
    return vec![];
  };
  match client.similar(base, ask).await {
    Ok(likes) => likes.iter().filter(|m| handed_from_base(m, brain)).map(liked_memory).collect(),
    Err(_) => vec![],
  }
}

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

fn string(arguments: &Value, name: &str) -> Option<String> {
  arguments.get(name).and_then(Value::as_str).map(str::to_owned).filter(|s| !s.is_empty())
}

fn integer(arguments: &Value, name: &str) -> Option<i64> {
  arguments.get(name).and_then(Value::as_i64)
}

/// A moment given as a date (`YYYY-MM-DD`, its start in UTC) or an RFC 3339 time, as epoch milliseconds;
/// none where the argument is absent, and why where it does not read.
fn moment(arguments: &Value, name: &str) -> Result<Option<i64>, String> {
  let Some(text) = string(arguments, name) else {
    return Ok(None);
  };
  let text = text.trim();
  if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text) {
    return Ok(Some(at.timestamp_millis()));
  }
  chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
    .ok()
    .and_then(|date| date.and_hms_opt(0, 0, 0))
    .map(|start| Some(start.and_utc().timestamp_millis()))
    .ok_or_else(|| format!("{name} is {text:?}; give a date (YYYY-MM-DD) or an RFC 3339 time"))
}

fn integers(arguments: &Value, name: &str) -> Vec<i64> {
  arguments
    .get(name)
    .and_then(Value::as_array)
    .map(|items| items.iter().filter_map(Value::as_i64).collect())
    .unwrap_or_default()
}

fn subjects(arguments: &Value, name: &str) -> Vec<SubjectKey> {
  arguments
    .get(name)
    .and_then(Value::as_array)
    .map(|items| items.iter().filter_map(|item| serde_json::from_value(item.clone()).ok()).collect())
    .unwrap_or_default()
}

/// The room a write links: the argument where given (empty for none), else the turn's room.
fn room_subject(arguments: &Value, session: &Session) -> Option<SubjectKey> {
  match arguments.get("room") {
    Some(Value::String(room)) if room.is_empty() => None,
    Some(Value::String(room)) => Some(subject("room", room)),
    Some(Value::Number(room)) => Some(subject("room", &room.to_string())),
    _ => room_of(session).map(|room| subject("room", &room)),
  }
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
  let arguments = &handed.arguments;
  let session = &handed.session;
  let answered: Result<Value, ClientFailure> = match name.as_str() {
    "brain_episode" => {
      let Some(text) = string(arguments, "text") else {
        return Ok(Answer::refusal("text is required"));
      };
      let Some(speaker) = string(arguments, "speaker") else {
        return Ok(Answer::refusal("speaker is required"));
      };
      let mut about = vec![subject("person", &speaker)];
      about.extend(room_subject(arguments, session));
      client
        .memory_add(
          brain,
          &NewMemory {
            key: None,
            kind: "episode".to_owned(),
            text,
            layer_id: None,
            position: None,
            subject_ids: vec![],
            subjects: about,
            importance: integer(arguments, "importance").or(Some(3)),
            valid_from: None,
            recall_condition: None,
            release_condition: None,
            status: None,
            actor_name: ACTOR.to_owned(),
            labels: Default::default(),
            internal_only: None,
            shared: None,
            locked: None,
            standing: None,
          },
        )
        .await
        .map(|memory| json!({ "memoryId": memory.id, "importance": memory.importance }))
    }
    "brain_similar" => {
      let Some(text) = string(arguments, "text") else {
        return Ok(Answer::refusal("text is required"));
      };
      let ask = SimilarAsk {
        text,
        subjects: subjects(arguments, "subjects"),
        limit: integer(arguments, "limit"),
        as_of: None,
      };
      match client.similar(brain, &ask).await {
        Ok(likes) => {
          let base = if reads_base(session) { base_liked(&client, brain, &ask).await } else { vec![] };
          Ok(json!({
            "memories": likes.iter().map(liked_memory).collect::<Vec<_>>(),
            "base": base,
          }))
        }
        Err(why) => Err(why),
      }
    }
    "brain_learn" => {
      let (Some(kind), Some(text), Some(source)) =
        (string(arguments, "type"), string(arguments, "text"), string(arguments, "source"))
      else {
        return Ok(Answer::refusal("type, text and source are required"));
      };
      let source = match source.as_str() {
        "own" => Source::Own,
        "owner" => Source::Owner,
        "told" => Source::Told,
        "inferred" => Source::Inferred,
        other => {
          return Ok(Answer::refusal(format!("source is {other:?}; it is own, owner, told or inferred")));
        }
      };
      let valid_from = match moment(arguments, "validFrom") {
        Ok(at) => at,
        Err(why) => return Ok(Answer::refusal(why)),
      };
      let mut about = subjects(arguments, "subjects");
      if let Some(room) = room_subject(arguments, session)
        && !about.contains(&room)
      {
        about.push(room);
      }
      client
        .learn(
          brain,
          &Learning {
            kind,
            text,
            subjects: about,
            importance: integer(arguments, "importance"),
            source,
            confirms: integers(arguments, "confirms"),
            strengthens: integer(arguments, "strengthens"),
            replaces: integers(arguments, "replaces"),
            grounds: integers(arguments, "grounds"),
            valid_from,
            recall_condition: string(arguments, "recallCondition"),
            release_condition: string(arguments, "releaseCondition"),
            key: string(arguments, "key"),
            layer_id: None,
            actor_name: ACTOR.to_owned(),
            labels: Default::default(),
            internal_only: None,
            shared: None,
            locked: None,
            standing: None,
          },
        )
        .await
        .map(|learned| {
          json!({
            "confirmedCount": learned.confirmed_count,
            "invalidatedIds": learned.invalidated_ids,
            "versioned": learned.versioned,
            "written": learned.written.as_ref().map(|m| json!({ "id": m.id, "type": m.kind, "status": m.status, "lineageId": m.lineage_id })),
          })
        })
    }
    "brain_move" => {
      let (Some(id), Some(status)) = (integer(arguments, "memory_id"), string(arguments, "status")) else {
        return Ok(Answer::refusal("memory_id and status are required"));
      };
      let Some(status) = MemoryStatus::named(&status) else {
        return Ok(Answer::refusal(format!("status is {status:?}; it is active, done or invalidated")));
      };
      client
        .memory_move(
          brain,
          id,
          &MemoryMove {
            status,
            at: integer(arguments, "at"),
            actor_name: ACTOR.to_owned(),
            reason: None,
            text: None,
            standing: None,
          },
        )
        .await
        .map(|m| json!({ "id": m.id, "status": m.status, "validTo": m.valid_to }))
    }
    "brain_list" => {
      let named = arguments
        .get("subject")
        .and_then(|s| serde_json::from_value::<SubjectKey>(s.clone()).ok());
      let query = MemoriesQuery {
        kind: string(arguments, "type"),
        status: string(arguments, "status").filter(|s| MemoryStatus::named(s).is_some()),
        key: string(arguments, "key"),
        q: string(arguments, "words"),
        situation: string(arguments, "situation"),
        trigger: string(arguments, "trigger"),
        limit: integer(arguments, "limit"),
        ..Default::default()
      };
      let base = if reads_base(session) {
        base_listed(&client, brain, named.as_ref(), &query).await
      } else {
        vec![]
      };
      let subject_id = match &named {
        Some(named) => match subject_in(&client, brain, named).await {
          Ok(Some(id)) => Some(id),
          Ok(None) => {
            return Ok(Answer::structured(json!({ "memories": [], "more": false, "base": base })));
          }
          Err(why) => return Ok(Answer::refusal(failed(why))),
        },
        None => None,
      };
      client
        .memories(
          brain,
          &MemoriesQuery {
            subject_id,
            ..query
          },
        )
        .await
        .map(|page| {
          json!({
            "memories": page.items.iter().map(listed_memory).collect::<Vec<_>>(),
            "more": page.more,
            "base": base,
          })
        })
    }
    "brain_unconsolidated" => client
      .unconsolidated(
        brain,
        &UnconsolidatedQuery {
          limit: integer(arguments, "limit"),
        },
      )
      .await
      .map(|episodes| {
        json!({
          "episodes": episodes.iter().map(|e| json!({
            "id": e.memory.id, "text": e.memory.text, "importance": e.memory.importance,
            "subjects": e.memory.subjects, "validFrom": e.memory.valid_from,
            "priority": e.priority, "gain": e.gain, "need": e.need, "taggedBy": e.tagged_by,
          })).collect::<Vec<_>>()
        })
      }),
    "brain_consolidated" => client
      .consolidated(
        brain,
        &Consolidated {
          episode_ids: integers(arguments, "episode_ids"),
        },
      )
      .await
      .map(|recorded| json!({ "recordedCount": recorded.recorded_count })),
    "brain_fold" => {
      let session_key =
        handed.session.variables.get("BMO_RUN_NAME").cloned().unwrap_or_else(|| agent.clone());
      client
        .fold(brain, &Fold { session_key })
        .await
        .map(|folded| json!({ "clearedCount": folded.cleared_count }))
    }
    "brain_duplicates" => client
      .duplicates(
        brain,
        &DuplicatesQuery {
          limit: integer(arguments, "limit"),
        },
      )
      .await
      .map(|pairs| {
        json!({
          "pairs": pairs.iter().map(|pair| json!({
            "first": listed_memory(&pair.first), "second": listed_memory(&pair.second),
            "subjectIds": pair.subject_ids, "vectorSimilarity": pair.vector_similarity,
            "wordShare": pair.word_share,
          })).collect::<Vec<_>>()
        })
      }),
    "brain_merge" => {
      let (Some(kind), Some(text)) = (string(arguments, "type"), string(arguments, "text")) else {
        return Ok(Answer::refusal("memory_ids, type and text are required"));
      };
      let memory_ids = integers(arguments, "memory_ids");
      if memory_ids.len() < 2 {
        return Ok(Answer::refusal("memory_ids names fewer than two memories"));
      }
      client
        .memory_merge(
          brain,
          &MemoryMerge {
            memory_ids,
            kind,
            text,
            key: None,
            layer_id: None,
            position: None,
            subject_ids: None,
            subjects: None,
            importance: integer(arguments, "importance"),
            valid_from: None,
            recall_condition: string(arguments, "recallCondition"),
            release_condition: string(arguments, "releaseCondition"),
            actor_name: ACTOR.to_owned(),
            labels: None,
            ..Default::default()
          },
        )
        .await
        .map(|m| json!({ "id": m.id, "type": m.kind, "status": m.status, "supersedes": m.supersedes }))
    }
    _ => unreachable!("the entry was checked against the list"),
  };
  Ok(match answered {
    Ok(value) => Answer::structured(value),
    Err(why) => Answer::refusal(failed(why)),
  })
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
      eprintln!("brain-tool: {why}");
      std::process::ExitCode::FAILURE
    }
  }
}
