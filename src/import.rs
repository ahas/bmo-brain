//! The old archive's move into the brain's store (docs/boundary-api.md, and the migration plan's table
//! map for TODO #1923): one export directory read, one brain tree read, this store written.
//!
//! The export is the one the item's export hand wrote under
//! `workbench/worker/items/1923/exports/brain/`: `manifest.json` with each source table's exact count
//! and key, and one NDJSON file a table. The brain tree is the repository under the data root's
//! `brain/`, whose 1,563 content files become fragments; 52 files (`index.md`, `sorted.txt` and
//! `knowledge/candidates.md`) are not fragments, and the 38 old rows whose files are gone are skipped.
//!
//! The import reads only those, refuses a store already holding rows of its tables, and runs whole
//! inside one transaction, so a dry run and a failed run leave the store as it stood.
//!
//! The map:
//!
//! - `fragments` is read for what each file's row says — its title, hits and labels — and the file is
//!   the fragment's text. A key is the agent's name and the file's path, `:` in place of `/`, never the
//!   bare slug. The scope carries the agent, and the room too for a room's document. A person's
//!   document carries the person's user id as its subject.
//! - `memories`, `notes`, `reminders` and `watches` keep their own names; their scope is built the same
//!   way, and their times are written in milliseconds, the brain's own unit.
//! - `session_people` is `session_loads`, whose `loaded_at` stays in the seconds its caller gave.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::store::Store;

/// One value written to the store, already in the target's shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
  Null,
  Int(i64),
  Text(String),
}

/// One mapped row: the target's columns and the cell each takes.
pub type Mapped = Vec<(&'static str, Cell)>;
/// The rows of one table, in the order they are written.
pub type MappedRows = Vec<Mapped>;

/// One table of the report.
#[derive(Clone, Debug, PartialEq)]
pub struct Moved {
  pub source: String,
  pub target: String,
  pub exported: u64,
  pub written: u64,
  pub note: String,
}

/// What an import run did.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
  pub dry_run: bool,
  pub snapshot: String,
  pub read_at: String,
  pub tables: Vec<Moved>,
  pub sample: Sample,
}

/// The count-and-sample compare's result.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
  pub checked: usize,
  pub failures: Vec<String>,
}

/// One export table's entry in the manifest.
#[derive(Clone, Debug)]
pub struct TableEntry {
  pub file: String,
  pub count: u64,
  pub key: Vec<String>,
}

/// The export directory's manifest.
#[derive(Clone, Debug)]
pub struct Manifest {
  pub snapshot: String,
  pub read_at: String,
  pub tables: HashMap<String, TableEntry>,
}

fn text(value: Option<&Value>) -> Option<String> {
  match value {
    Some(Value::String(said)) => Some(said.clone()),
    Some(Value::Null) | None => None,
    Some(other) => Some(other.to_string()),
  }
}

fn int(value: Option<&Value>) -> Option<i64> {
  match value {
    Some(Value::Number(number)) => number.as_i64().or_else(|| number.as_f64().map(|n| n as i64)),
    Some(Value::String(said)) => said.parse().ok(),
    _ => None,
  }
}

fn is_set(value: Option<&Value>) -> bool {
  matches!(value, Some(value) if !value.is_null())
}

fn seconds(value: Option<&Value>) -> Option<i64> {
  int(value).map(|seconds| seconds * 1000)
}

fn put(map: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
  if let Some(value) = value
    && !value.is_null()
  {
    let said = match value {
      Value::String(said) => said.clone(),
      other => other.to_string(),
    };
    map.insert(key.to_owned(), Value::String(said));
  }
}

/// The scope a member is held under: the agent, and the room for a room's member.
pub fn scope(agent: &str, chat: Option<i64>) -> String {
  match chat {
    Some(chat) => format!("{agent}:room:{chat}"),
    None => agent.to_owned(),
  }
}

fn agent_of(row: &Value) -> String {
  text(row.get("agent"))
    .filter(|said| !said.is_empty())
    .unwrap_or_else(|| "bmo".to_owned())
}

/// The state a memory is in: it waits for approval, is done, was set aside, or stands.
pub fn memory_state(row: &Value) -> &'static str {
  if is_set(row.get("candidate_why")) {
    return "candidate";
  }
  if is_set(row.get("done_at")) {
    return "done";
  }
  if int(row.get("active")).unwrap_or(1) == 0 {
    return "retired";
  }

  "standing"
}

/// A memory row in the brain's shape.
pub fn memory(row: &Value) -> Vec<(&'static str, Cell)> {
  let agent = agent_of(row);
  let chat = int(row.get("chat_id"));
  let action = text(row.get("action")).unwrap_or_default();
  let condition = text(row.get("condition")).unwrap_or_default();
  let body = if action.trim().is_empty() { condition } else { action };
  let mut conditions = Map::new();
  for (key, source) in [
    ("reach", "reach"),
    ("condition", "condition"),
    ("type", "type"),
    ("event", "event"),
    ("messageId", "message_id"),
    ("todoId", "todo_id"),
  ] {
    if let Some(value) = row.get(source)
      && !value.is_null()
    {
      conditions.insert(key.to_owned(), value.clone());
    }
  }
  let mut labels = Map::new();
  for key in [
    "slug",
    "chat_id",
    "agent",
    "reach",
    "message_id",
    "todo_id",
    "event",
    "event_met_at",
    "down_reason",
    "down_at",
    "candidate_why",
    "candidate_at",
    "moved_at",
    "done_text",
    "set_by",
  ] {
    put(&mut labels, key, row.get(key));
  }
  let changed = ["done_at", "candidate_at", "moved_at", "last_hit_at", "set_at"]
    .iter()
    .filter_map(|key| seconds(row.get(*key)))
    .max()
    .unwrap_or(0);

  vec![
    ("id", cell_int(row.get("id"))),
    ("scope", Cell::Text(scope(&agent, chat))),
    (
      "subject_key",
      match text(row.get("for_user")) {
        Some(said) => Cell::Text(said),
        None => Cell::Null,
      },
    ),
    ("kind", Cell::Text(text(row.get("type")).unwrap_or_else(|| "memory".to_owned()))),
    ("text", Cell::Text(body)),
    ("state", Cell::Text(memory_state(row).to_owned())),
    ("conditions", Cell::Text(Value::Array(vec![Value::Object(conditions)]).to_string())),
    ("labels", Cell::Text(Value::Object(labels).to_string())),
    ("due_at", Cell::Null),
    ("match_count", cell_int(row.get("hits"))),
    ("last_matched_at", cell_seconds(row.get("last_hit_at"))),
    ("created_at", cell_seconds(row.get("set_at"))),
    ("changed_at", Cell::Int(changed)),
  ]
}

fn cell_int(value: Option<&Value>) -> Cell {
  match int(value) {
    Some(number) => Cell::Int(number),
    None => Cell::Null,
  }
}

fn cell_seconds(value: Option<&Value>) -> Cell {
  match seconds(value) {
    Some(number) => Cell::Int(number),
    None => Cell::Null,
  }
}

/// A note row in the brain's shape.
pub fn note(row: &Value) -> Vec<(&'static str, Cell)> {
  let agent = agent_of(row);
  let chat = int(row.get("chat_id"));

  vec![
    ("id", cell_int(row.get("id"))),
    ("scope", Cell::Text(scope(&agent, chat))),
    ("subject_key", Cell::Null),
    ("text", Cell::Text(text(row.get("text")).unwrap_or_default())),
    (
      "actor_name",
      Cell::Text(
        text(row.get("author"))
          .or_else(|| text(row.get("agent")))
          .unwrap_or_else(|| agent.clone()),
      ),
    ),
    ("created_at", cell_seconds(row.get("created_at"))),
  ]
}

/// A reminder row in the brain's shape.
pub fn reminder(row: &Value) -> Vec<(&'static str, Cell)> {
  let agent = agent_of(row);
  let chat = int(row.get("chat_id"));
  let mut labels = Map::new();
  for key in ["memory_id", "every", "fired", "user_id", "chat_id", "agent"] {
    put(&mut labels, key, row.get(key));
  }
  let fired = int(row.get("fired")).unwrap_or(0) == 1;

  vec![
    ("id", cell_int(row.get("id"))),
    ("scope", Cell::Text(scope(&agent, chat))),
    ("due_at", cell_seconds(row.get("due_at"))),
    ("text", Cell::Text(text(row.get("reason")).unwrap_or_default())),
    ("labels", Cell::Text(Value::Object(labels).to_string())),
    ("setter_name", Cell::Text(agent)),
    ("created_at", cell_seconds(row.get("created_at"))),
    ("fired_at", if fired { cell_seconds(row.get("due_at")) } else { Cell::Null }),
  ]
}

/// The state a watch is in, in the brain's words.
pub fn watch_state(state: &str) -> &'static str {
  match state {
    "fired" => "fired",
    "cancelled" => "cancelled",
    "lapsed" | "broken" => "ended",
    _ => "armed",
  }
}

/// A watch row in the brain's shape: its condition as the kind the brain knows, and what it waited
/// on kept in its labels.
pub fn watch(row: &Value) -> Vec<(&'static str, Cell)> {
  let agent = agent_of(row);
  let chat = int(row.get("chat_id"));
  let kind = text(row.get("kind")).unwrap_or_default();
  let condition = if kind == "timer" {
    let at = seconds(row.get("due_at")).or_else(|| seconds(row.get("deadline_at"))).unwrap_or(0);
    json!({"kind": "at", "at": at})
  } else {
    json!({
      "kind": "external",
      "key": text(row.get("dedupe_key")).or_else(|| text(row.get("name"))).unwrap_or_default(),
    })
  };
  let mut labels = Map::new();
  for key in [
    "kind",
    "name",
    "note",
    "dedupe_key",
    "interval_seconds",
    "unit",
    "run_id",
    "instruction_id",
    "last_checked_at",
    "last_exit",
    "memory_id",
    "by_chat",
    "agent",
  ] {
    put(&mut labels, key, row.get(key));
  }
  put(&mut labels, "condition", row.get("condition"));
  let created = seconds(row.get("armed_at")).or_else(|| seconds(row.get("created_at"))).unwrap_or(0);
  let changed = seconds(row.get("last_checked_at"))
    .or_else(|| seconds(row.get("ended_at")))
    .unwrap_or(created);

  vec![
    ("id", cell_int(row.get("id"))),
    ("scope", Cell::Text(scope(&agent, chat))),
    ("condition", Cell::Text(condition.to_string())),
    ("state", Cell::Text(watch_state(&text(row.get("state")).unwrap_or_default()).to_owned())),
    ("setter_name", Cell::Text(agent)),
    ("holder_name", Cell::Null),
    ("labels", Cell::Text(Value::Object(labels).to_string())),
    ("fired_at", cell_seconds(row.get("ended_at"))),
    ("created_at", Cell::Int(created)),
    ("changed_at", Cell::Int(changed)),
    ("at", cell_seconds(row.get("due_at"))),
    ("memory_id", cell_int(row.get("memory_id"))),
  ]
}

/// A session's load, its `loaded_at` still in the seconds its caller gave.
pub fn session_load(row: &Value) -> Vec<(&'static str, Cell)> {
  vec![
    ("session", Cell::Text(text(row.get("session_id")).unwrap_or_default())),
    ("path", Cell::Text(text(row.get("path")).unwrap_or_default())),
    ("version", Cell::Text(text(row.get("version")).unwrap_or_default())),
    ("loaded_at", cell_int(row.get("loaded_at"))),
  ]
}

/// What a brain file is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
  Piece,
  Sheet,
  Knowledge,
  Rule,
}

/// One content file of the brain tree.
#[derive(Clone, Debug)]
pub struct Document {
  pub path: String,
  pub agent: String,
  pub chat: Option<i64>,
  pub subject: Option<String>,
  pub kind: Kind,
  pub text: String,
  pub title: String,
}

/// The files that are no fragment: an index, a sorting state, the candidates document.
pub fn not_a_fragment(path: &str) -> bool {
  path.ends_with("/index.md")
    || path == "knowledge/index.md"
    || path.ends_with("/sorted.txt")
    || path == "knowledge/candidates.md"
}

/// The first heading of a text, or its first line, as a title.
fn heading(text: &str) -> Option<String> {
  text
    .lines()
    .find_map(|line| line.trim().strip_prefix("# ").map(str::trim))
    .filter(|said| !said.is_empty())
    .map(str::to_owned)
}

/// Classifies one path of the brain tree: its agent, room, subject and kind. The path kept is the
/// tree's own, `agents/<agent>/` and all, so an old row is found by it and a companion's document is
/// never mistaken for the root agent's.
pub fn classify(full: &str, text: &str) -> Document {
  let mut path = full.to_owned();
  let mut agent = "bmo".to_owned();
  if let Some(rest) = path.strip_prefix("agents/")
    && let Some((named, rest)) = rest.split_once('/')
  {
    agent = named.to_owned();
    path = rest.to_owned();
  }
  let parts: Vec<&str> = path.split('/').collect();
  let mut chat = None;
  let mut subject = None;
  let kind = if parts.first() == Some(&"knowledge") {
    Kind::Knowledge
  } else if parts.first() == Some(&"rooms") {
    chat = parts.get(1).and_then(|said| said.parse().ok());
    if parts.get(2) == Some(&"rules") {
      Kind::Rule
    } else if path.contains("/people/chunks/") {
      subject = parts
        .last()
        .and_then(|name| name.strip_prefix('p'))
        .and_then(|name| name.split_once('-').map(|(uid, _)| uid.to_owned()))
        .filter(|uid| !uid.is_empty());
      Kind::Piece
    } else {
      subject = parts.last().and_then(|name| name.strip_suffix(".md")).map(str::to_owned);
      Kind::Sheet
    }
  } else if parts.first() == Some(&"people") {
    if path.contains("/chunks/") {
      subject = parts.get(1).map(|uid| (*uid).to_owned());
      Kind::Piece
    } else {
      subject = parts.get(1).map(|uid| (*uid).to_owned());
      Kind::Sheet
    }
  } else {
    Kind::Knowledge
  };
  let title = heading(text)
    .or_else(|| parts.last().map(|name| name.trim_end_matches(".md").to_owned()))
    .unwrap_or_default();

  Document {
    path: full.to_owned(),
    agent,
    chat,
    subject,
    kind,
    text: text.to_owned(),
    title,
  }
}

/// The key a fragment is held by: its agent and its path without the agent's own folder, never a bare
/// slug.
pub fn key(document: &Document) -> String {
  let prefix = format!("agents/{}/", document.agent);
  let rest = document.path.strip_prefix(&prefix).unwrap_or(&document.path);
  format!("{}:{}", document.agent, rest.trim_end_matches(".md").replace('/', ":"))
}

/// Every content file of the brain tree, read, beside the path of every file the tree holds.
pub fn documents(root: &Path) -> Result<(Vec<Document>, Vec<String>), String> {
  let mut out = Vec::new();
  let mut files = Vec::new();
  walk(root, root, &mut out, &mut files)?;
  out.sort_by(|left, right| left.path.cmp(&right.path));
  files.sort();

  Ok((out, files))
}

fn walk(
  root: &Path,
  folder: &Path,
  out: &mut Vec<Document>,
  files: &mut Vec<String>,
) -> Result<(), String> {
  let entries = std::fs::read_dir(folder)
    .map_err(|error| format!("{} could not be read: {error}", folder.display()))?;
  for entry in entries {
    let entry = entry.map_err(|error| error.to_string())?;
    let path = entry.path();
    let name = entry.file_name();
    if name == ".git" {
      continue;
    }
    if path.is_dir() {
      walk(root, &path, out, files)?;
      continue;
    }
    let relative = path
      .strip_prefix(root)
      .map_err(|error| error.to_string())?
      .to_string_lossy()
      .trim_start_matches("./")
      .to_owned();
    files.push(relative.clone());
    if not_a_fragment(&relative) || !relative.ends_with(".md") {
      continue;
    }
    let text = std::fs::read_to_string(&path)
      .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    out.push(classify(&relative, &text));
  }

  Ok(())
}

/// The scope a document is held under: null for shared knowledge, the agent alone for its own common
/// documents, and the agent and the room for a room's.
pub fn document_scope(document: &Document, old: Option<&Value>) -> Option<String> {
  if document.kind == Kind::Knowledge {
    let old_scope = old.and_then(|row| text(row.get("scope"))).unwrap_or_default();
    let room = old_scope
      .strip_prefix("chat:")
      .and_then(|said| said.parse::<i64>().ok())
      .or_else(|| old_scope.parse::<i64>().ok());
    return room.map(|room| scope(&document.agent, Some(room)));
  }
  if document.chat.is_some() {
    return Some(scope(&document.agent, document.chat));
  }

  Some(document.agent.clone())
}

/// The kind of fragment a document is written as.
pub fn document_kind(document: &Document) -> &'static str {
  match document.kind {
    Kind::Piece | Kind::Sheet => "person",
    Kind::Knowledge => "library-knowledge",
    Kind::Rule => "room-rule",
  }
}

/// A fragment row built from its file and, where one stands, its old row.
pub fn fragment(document: &Document, old: Option<&Value>) -> Vec<(&'static str, Cell)> {
  let body = if document.text.trim().is_empty() { " ".to_owned() } else { document.text.clone() };
  let version = Sha256::digest(body.as_bytes())
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect::<String>();
  let mut labels = Map::new();
  for key in [
    "chat_id",
    "agent",
    "all_agents",
    "keywords",
    "items",
    "written_on",
    "confirmed_on",
    "status",
    "position",
    "scope",
    "slug",
  ] {
    if let Some(value) = old.and_then(|row| row.get(key)) {
      put(&mut labels, key, Some(value));
    }
  }
  if let Some(old) = old
    && let Some(kind) = text(old.get("kind"))
  {
    labels.insert("kind".to_owned(), Value::String(kind));
  }
  let active = old.map(|row| int(row.get("active")).unwrap_or(1) != 0).unwrap_or(true);
  let hits = old.and_then(|row| int(row.get("hits"))).unwrap_or(0);
  let last_hit = old.and_then(|row| seconds(row.get("last_hit_at")));
  let created = old.and_then(|row| seconds(row.get("set_at"))).unwrap_or(0);
  let changed = [last_hit, Some(created)].into_iter().flatten().max().unwrap_or(0);
  let title = old
    .and_then(|row| text(row.get("title")))
    .filter(|said| !said.trim().is_empty())
    .unwrap_or_else(|| document.title.clone());

  vec![
    ("key", Cell::Text(key(document))),
    ("kind", Cell::Text(document_kind(document).to_owned())),
    (
      "scope",
      match document_scope(document, old) {
        Some(scope) => Cell::Text(scope),
        None => Cell::Null,
      },
    ),
    (
      "subject_key",
      match &document.subject {
        Some(subject) => Cell::Text(subject.clone()),
        None => Cell::Null,
      },
    ),
    ("title", Cell::Text(title)),
    ("path", Cell::Text(document.path.clone())),
    ("text", Cell::Text(body)),
    ("version", Cell::Text(version)),
    ("library_name", Cell::Null),
    ("state", Cell::Text(if active { "active" } else { "off" }.to_owned())),
    ("switched_off", Cell::Text("{}".to_owned())),
    ("labels", Cell::Text(Value::Object(labels).to_string())),
    ("hit_count", Cell::Int(hits)),
    (
      "last_hit_at",
      match last_hit {
        Some(at) => Cell::Int(at),
        None => Cell::Null,
      },
    ),
    ("created_at", Cell::Int(created)),
    ("changed_at", Cell::Int(changed)),
  ]
}

/// The export directory's manifest.
pub fn read_manifest(dir: &Path) -> Result<Manifest, String> {
  let path = dir.join("manifest.json");
  let said = std::fs::read_to_string(&path)
    .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
  let document: Value =
    serde_json::from_str(&said).map_err(|error| format!("{} is not JSON: {error}", path.display()))?;
  let mut tables = HashMap::new();
  let list = document.get("tables").and_then(Value::as_array).ok_or("the manifest names no tables")?;
  for table in list {
    let source = table
      .get("source")
      .and_then(Value::as_str)
      .ok_or("a manifest table names no source")?
      .to_owned();
    let file = table
      .get("file")
      .and_then(Value::as_str)
      .ok_or("a manifest table names no file")?
      .to_owned();
    let count = table.get("count").and_then(Value::as_u64).ok_or("a manifest table names no count")?;
    let key: Vec<String> = table
      .get("key")
      .and_then(Value::as_array)
      .map(|keys| keys.iter().filter_map(Value::as_str).map(str::to_owned).collect())
      .unwrap_or_default();
    tables.insert(source, TableEntry { file, count, key });
  }

  Ok(Manifest {
    snapshot: document.get("snapshot").and_then(Value::as_str).unwrap_or_default().to_owned(),
    read_at: document.get("readAt").and_then(Value::as_str).unwrap_or_default().to_owned(),
    tables,
  })
}

/// One table's rows, each checked against the manifest's count as it is read.
pub fn rows(dir: &Path, manifest: &Manifest, source: &str) -> Result<Vec<Value>, String> {
  let entry = manifest
    .tables
    .get(source)
    .ok_or_else(|| format!("the manifest has no table {source}"))?;
  let path = dir.join(&entry.file);
  let said = std::fs::read_to_string(&path)
    .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
  let mut out = Vec::new();
  for (number, line) in said.lines().enumerate() {
    if line.trim().is_empty() {
      continue;
    }
    let row: Value = serde_json::from_str(line)
      .map_err(|error| format!("{} line {} is not JSON: {error}", path.display(), number + 1))?;
    out.push(row);
  }
  if out.len() as u64 != entry.count {
    return Err(format!(
      "{}: the manifest says {} rows and the file holds {}",
      path.display(),
      entry.count,
      out.len()
    ));
  }

  Ok(out)
}

/// The export and the tree read and mapped, before anything is written.
pub struct Prepared {
  pub manifest: Manifest,
  pub fragments: MappedRows,
  pub memories: MappedRows,
  pub notes: MappedRows,
  pub reminders: MappedRows,
  pub watches: MappedRows,
  pub loads: MappedRows,
  /// Old rows whose file is not in the tree at all.
  pub gone: u64,
  /// Old rows naming a file that is no fragment: an index, the sorting state, the candidates document.
  pub not_fragment: u64,
  /// Content files the old archive holds no row for.
  pub rowless: u64,
}

/// Reads the export and the brain tree, mapping every row to its target's shape.
pub fn prepare(export: &Path, brain: &Path) -> Result<Prepared, String> {
  let manifest = read_manifest(export)?;
  let old_fragments = rows(export, &manifest, "fragments")?;
  let by_path: HashMap<String, &Value> = old_fragments
    .iter()
    .filter_map(|row| text(row.get("path")).map(|path| (path, row)))
    .collect();

  let (documents, files) = documents(brain)?;
  let fragments: Vec<_> = documents
    .iter()
    .map(|document| fragment(document, by_path.get(&document.path).copied()))
    .collect();
  let files: std::collections::HashSet<&str> = files.iter().map(String::as_str).collect();
  let content: std::collections::HashSet<&str> =
    documents.iter().map(|document| document.path.as_str()).collect();
  let mut gone = 0;
  let mut not_fragment = 0;
  for row in &old_fragments {
    match text(row.get("path")) {
      None => gone += 1,
      Some(path) if content.contains(path.as_str()) => {}
      Some(path) if files.contains(path.as_str()) => not_fragment += 1,
      Some(_) => gone += 1,
    }
  }
  let rowless = documents.iter().filter(|document| !by_path.contains_key(&document.path)).count() as u64;

  let memories = rows(export, &manifest, "memories")?.iter().map(memory).collect();
  let notes = rows(export, &manifest, "notes")?.iter().map(note).collect();
  let reminders = rows(export, &manifest, "reminders")?.iter().map(reminder).collect();
  let watches = rows(export, &manifest, "watches")?.iter().map(watch).collect();
  let loads = rows(export, &manifest, "session_people")?.iter().map(session_load).collect();

  Ok(Prepared {
    manifest,
    fragments,
    memories,
    notes,
    reminders,
    watches,
    loads,
    gone,
    not_fragment,
    rowless,
  })
}

/// The target's columns and their types, so every value can be cast from its text.
async fn types(pool: &PgPool, table: &str) -> Result<HashMap<String, String>, String> {
  let rows = sqlx::query(
    "SELECT column_name, data_type FROM information_schema.columns \
     WHERE table_schema = 'public' AND table_name = $1",
  )
  .bind(table)
  .fetch_all(pool)
  .await
  .map_err(|error| format!("{table}'s columns could not be read: {error}"))?;
  let mut out = HashMap::new();
  for row in rows {
    let kind: String = row.get("data_type");
    out.insert(
      row.get::<String, _>("column_name"),
      // A column of many values is one array kind in the catalog; text is what its text is read as.
      if kind == "ARRAY" { "text[]".to_owned() } else { kind },
    );
  }
  if out.is_empty() {
    return Err(format!("{table} holds no columns"));
  }

  Ok(out)
}

fn bind<'q>(
  mut query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
  cell: &Cell,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
  let said = match cell {
    Cell::Null => None,
    Cell::Int(number) => Some(number.to_string()),
    Cell::Text(said) => Some(said.clone()),
  };
  query = query.bind(said);
  query
}

async fn insert(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  table: &str,
  kinds: &HashMap<String, String>,
  row: &[(&'static str, Cell)],
) -> Result<(), String> {
  let columns = row.iter().map(|(column, _)| format!("\"{column}\"")).collect::<Vec<_>>().join(", ");
  let values = row
    .iter()
    .enumerate()
    .map(|(position, (column, _))| {
      let kind = kinds.get(*column).map(String::as_str).unwrap_or("text");
      format!("CAST(${} AS {kind})", position + 1)
    })
    .collect::<Vec<_>>()
    .join(", ");
  let sql = format!("INSERT INTO \"{table}\" ({columns}) VALUES ({values})");
  let mut query = sqlx::query(&sql);
  for (_, cell) in row {
    query = bind(query, cell);
  }
  query
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("{table} could not be written: {error}"))?;

  Ok(())
}

/// The tables this import writes, and the key each is sampled by.
pub const WRITTEN: [(&str, &[&str]); 6] = [
  ("fragments", &["key"]),
  ("memories", &["id"]),
  ("notes", &["id"]),
  ("reminders", &["id"]),
  ("watches", &["id"]),
  ("session_loads", &["session", "path"]),
];

/// Refuses a store that already holds rows of any table this import writes.
pub async fn refused_if_holding(pool: &PgPool) -> Result<(), String> {
  let mut holding = Vec::new();
  for (table, _) in WRITTEN {
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{table}\""))
      .fetch_one(pool)
      .await
      .map_err(|error| format!("{table} could not be counted: {error}"))?;
    if count > 0 {
      holding.push(format!("{table} holds {count}"));
    }
  }
  if holding.is_empty() {
    Ok(())
  } else {
    Err(format!(
      "the store already holds rows of its tables, so nothing was written: {}",
      holding.join(", ")
    ))
  }
}

/// Whether two cells hold the same JSON document, whatever their spelling.
fn same_document(left: &Cell, right: &Cell) -> bool {
  match (left, right) {
    (Cell::Text(left), Cell::Text(right)) => {
      match (serde_json::from_str::<Value>(left), serde_json::from_str::<Value>(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
      }
    }
    _ => false,
  }
}

fn digest(said: &str) -> String {
  Sha256::digest(said.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sample<'a>(rows: &'a [Mapped], key: &[&str]) -> Vec<&'a Mapped> {
  if rows.len() <= 20 {
    return rows.iter().collect();
  }
  let mut ordered: Vec<(String, &Vec<(&'static str, Cell)>)> = rows
    .iter()
    .map(|row| {
      let said = key
        .iter()
        .map(|column| {
          row
            .iter()
            .find(|(held, _)| held == column)
            .map(|(_, cell)| format!("{cell:?}"))
            .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("|");
      (digest(&said), row)
    })
    .collect();
  ordered.sort_by(|left, right| left.0.cmp(&right.0));
  let mut picked: Vec<&Vec<(&'static str, Cell)>> = vec![&rows[0], &rows[rows.len() - 1]];
  for (_, row) in ordered.into_iter().take(20) {
    if !picked.iter().any(|held| std::ptr::eq(*held, row)) {
      picked.push(row);
    }
  }

  picked
}

async fn compare(pool: &PgPool, prepared: &Prepared) -> Sample {
  let mut failures = Vec::new();
  let mut checked = 0;
  let tables: [(&str, &[&str], &[Mapped]); 6] = [
    ("fragments", &["key"], &prepared.fragments),
    ("memories", &["id"], &prepared.memories),
    ("notes", &["id"], &prepared.notes),
    ("reminders", &["id"], &prepared.reminders),
    ("watches", &["id"], &prepared.watches),
    ("session_loads", &["session", "path"], &prepared.loads),
  ];
  for (target, key, rows) in tables {
    let kinds = match types(pool, target).await {
      Ok(kinds) => kinds,
      Err(error) => {
        failures.push(error);
        continue;
      }
    };
    let count: Result<i64, _> =
      sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{target}\"")).fetch_one(pool).await;
    match count {
      Ok(held) if held as u64 == rows.len() as u64 => {}
      Ok(held) => {
        failures.push(format!("{target}: {} rows were mapped and {target} holds {held}", rows.len()))
      }
      Err(error) => failures.push(format!("{target} could not be counted: {error}")),
    }
    for row in sample(rows, key) {
      checked += 1;
      let selected = row
        .iter()
        .map(|(column, _)| format!("\"{column}\"::text AS \"{column}\""))
        .collect::<Vec<_>>()
        .join(", ");
      let mut sql = format!("SELECT {selected} FROM \"{target}\" WHERE ");
      for (position, column) in key.iter().enumerate() {
        if position > 0 {
          sql.push_str(" AND ");
        }
        let kind = kinds.get(*column).map(String::as_str).unwrap_or("text");
        sql.push_str(&format!("\"{column}\" = CAST(${} AS {kind})", position + 1));
      }
      let mut query = sqlx::query(&sql);
      let mut bound = true;
      for column in key {
        match row.iter().find(|(held, _)| held == column).map(|(_, cell)| cell) {
          Some(cell) => query = bind(query, cell),
          None => {
            bound = false;
            break;
          }
        }
      }
      if !bound {
        continue;
      }
      match query.fetch_optional(pool).await {
        Ok(Some(held)) => {
          for (column, cell) in row {
            let actual = held
              .try_get::<Option<String>, _>(*column)
              .map(|said| said.map_or(Cell::Null, Cell::Text));
            let expected = match cell {
              Cell::Null => Cell::Null,
              Cell::Int(number) => Cell::Text(number.to_string()),
              Cell::Text(said) => Cell::Text(said.clone()),
            };
            // A document's own text is not its spelling in the store: compare them as documents.
            let document = matches!(kinds.get(*column).map(String::as_str), Some("jsonb") | Some("json"));
            match actual {
              Ok(actual) if actual == expected => {}
              Ok(actual) if document && same_document(&expected, &actual) => {}
              Ok(actual) => {
                failures.push(format!("{target}: {column} was written {expected:?} but is {actual:?}"))
              }
              Err(error) => failures.push(format!("{target}: {column} could not be read: {error}")),
            }
          }
        }
        Ok(None) => failures.push(format!("{target}: a sampled row is missing")),
        Err(error) => failures.push(format!("{target}: the sample could not be read: {error}")),
      }
    }
  }

  Sample { checked, failures }
}

/// Runs the import: reads the export and the tree, refuses a store holding rows, writes every row, and
/// compares what it wrote. A dry run rolls the transaction back.
pub async fn run(database: &str, export: &Path, brain: &Path, dry_run: bool) -> Result<Report, String> {
  let prepared = prepare(export, brain)?;
  let store = Store::lazy(database)?;
  store.migrate().await.map_err(|error| format!("the migrations: {error}"))?;
  let pool = PgPoolOptions::new()
    .max_connections(1)
    .connect(database)
    .await
    .map_err(|error| format!("{database} could not be opened: {error}"))?;
  refused_if_holding(&pool).await?;

  let mut kinds: HashMap<&str, HashMap<String, String>> = HashMap::new();
  for (table, _) in WRITTEN {
    kinds.insert(table, types(&pool, table).await?);
  }

  let mut tx = pool
    .begin()
    .await
    .map_err(|error| format!("the transaction could not be opened: {error}"))?;
  for (table, rows) in [
    ("fragments", &prepared.fragments),
    ("memories", &prepared.memories),
    ("notes", &prepared.notes),
    ("reminders", &prepared.reminders),
    ("watches", &prepared.watches),
    ("session_loads", &prepared.loads),
  ] {
    let table_kinds = kinds.get(table).expect("a table's types");
    for row in rows {
      insert(&mut tx, table, table_kinds, row).await?;
    }
  }

  if dry_run {
    tx.rollback()
      .await
      .map_err(|error| format!("the dry run could not be rolled back: {error}"))?;
  } else {
    tx.commit()
      .await
      .map_err(|error| format!("the import could not be committed: {error}"))?;
  }

  let sample = if dry_run {
    Sample {
      checked: 0,
      failures: Vec::new(),
    }
  } else {
    compare(&pool, &prepared).await
  };

  let exported = |source: &str| prepared.manifest.tables.get(source).map(|entry| entry.count).unwrap_or(0);
  let tables = vec![
    Moved {
      source: "fragments".to_owned(),
      target: "fragments".to_owned(),
      exported: exported("fragments"),
      written: prepared.fragments.len() as u64,
      note: format!(
        "the brain tree's files; {} old rows whose files are gone, {} naming an index or sorting file, and {} files with no old row",
        prepared.gone, prepared.not_fragment, prepared.rowless
      ),
    },
    Moved {
      source: "memories".to_owned(),
      target: "memories".to_owned(),
      exported: exported("memories"),
      written: prepared.memories.len() as u64,
      note: String::new(),
    },
    Moved {
      source: "notes".to_owned(),
      target: "notes".to_owned(),
      exported: exported("notes"),
      written: prepared.notes.len() as u64,
      note: String::new(),
    },
    Moved {
      source: "reminders".to_owned(),
      target: "reminders".to_owned(),
      exported: exported("reminders"),
      written: prepared.reminders.len() as u64,
      note: String::new(),
    },
    Moved {
      source: "watches".to_owned(),
      target: "watches".to_owned(),
      exported: exported("watches"),
      written: prepared.watches.len() as u64,
      note: String::new(),
    },
    Moved {
      source: "session_people".to_owned(),
      target: "session_loads".to_owned(),
      exported: exported("session_people"),
      written: prepared.loads.len() as u64,
      note: String::new(),
    },
  ];
  pool.close().await;

  Ok(Report {
    dry_run,
    snapshot: prepared.manifest.snapshot.clone(),
    read_at: prepared.manifest.read_at.clone(),
    tables,
    sample,
  })
}

/// The count-and-sample compare alone, against a store the import wrote.
pub async fn verify(database: &str, export: &Path, brain: &Path) -> Result<Sample, String> {
  let prepared = prepare(export, brain)?;
  let pool = PgPoolOptions::new()
    .max_connections(1)
    .connect(database)
    .await
    .map_err(|error| format!("{database} could not be opened: {error}"))?;
  let sample = compare(&pool, &prepared).await;
  pool.close().await;

  Ok(sample)
}

/// The tree's root under the data root, for a caller that names none.
pub fn brain_root(data_root: &Path) -> PathBuf {
  data_root.join("brain")
}
