//! `bmo memory` (docs/boundary-api.md, 「The command」): the memory rows, asked of 「Memory rows」's routes.

use bmo_brain_client::{
  BrainClient, Labels, NewScopedMemory, Object, ScopedMemoriesQuery, ScopedMemory, ScopedMemoryChange,
  ScopedMemoryState,
};
use clap::{Args, Subcommand};

use super::{labels, labels_text, moment, moment_text, object, text_of};

/// The forms `bmo memory` takes.
#[derive(Subcommand)]
pub enum Word {
  /// Adds a row and prints it.
  Add(Add),
  /// Changes a row's text, state, labels or due moment, and prints it.
  Update(Update),
  /// Moves a row to done, with the text it ends on where one is given, and prints it.
  Done(Done),
  /// Moves a row to retired and prints it.
  Retire(Move),
  /// Puts a retired or set-aside row back to standing and prints it.
  Revive(Move),
  /// Raises a row as a candidate and prints it.
  Candidate(Move),
  /// Lists the rows the filters name.
  List(List),
}

/// A row to add.
#[derive(Args)]
pub struct Add {
  /// The scope the row is held under.
  #[arg(long)]
  scope: String,
  /// The caller's key for what the row is about.
  #[arg(long)]
  subject: Option<String>,
  /// The caller's word for what the row is.
  #[arg(long, visible_alias = "type")]
  kind: String,
  /// The row's text; `-` reads it from standard input.
  #[arg(long, allow_hyphen_values = true)]
  text: String,
  /// A condition kept as given, as a JSON object; one flag each.
  #[arg(long, value_name = "json")]
  condition: Vec<String>,
  /// A label as key=value, one flag each.
  #[arg(long, value_name = "key=value")]
  label: Vec<String>,
  /// The moment it is due, as epoch milliseconds or an RFC 3339 time.
  #[arg(long, value_name = "moment")]
  due: Option<String>,
  /// Where it starts: standing where none is given.
  #[arg(long, value_name = "state")]
  state: Option<String>,
}

/// A row to change.
#[derive(Args)]
pub struct Update {
  /// The row's id.
  id: i64,
  /// The row's new text; `-` reads it from standard input.
  #[arg(long, allow_hyphen_values = true)]
  text: Option<String>,
  /// The state to move it to.
  #[arg(long, value_name = "state")]
  state: Option<String>,
  /// The labels, replacing them whole; one flag each.
  #[arg(long, value_name = "key=value")]
  label: Vec<String>,
  /// The moment it is due.
  #[arg(long, value_name = "moment", conflicts_with = "no_due")]
  due: Option<String>,
  /// Clears the moment it is due.
  #[arg(long = "no-due")]
  no_due: bool,
}

/// A row moved to done.
#[derive(Args)]
pub struct Done {
  /// The row's id.
  id: i64,
  /// The text it ends on; `-` reads it from standard input.
  #[arg(long, allow_hyphen_values = true)]
  text: Option<String>,
}

/// A row moved to another state.
#[derive(Args)]
pub struct Move {
  /// The row's id.
  id: i64,
}

/// Which rows to list.
#[derive(Args)]
pub struct List {
  /// The scope the rows are held under.
  #[arg(long)]
  scope: Option<String>,
  /// The rows of every scope beginning with this.
  #[arg(long = "scope-prefix")]
  scope_prefix: Option<String>,
  /// The caller's key for what the rows are about.
  #[arg(long)]
  subject: Option<String>,
  /// The caller's word for what the rows are.
  #[arg(long, visible_alias = "type")]
  kind: Option<String>,
  /// The state the rows are in.
  #[arg(long, value_name = "state")]
  state: Option<String>,
  /// A label the rows carry, as `key` or `key=value`.
  #[arg(long, value_name = "key[=value]")]
  label: Option<String>,
  /// The rows due at or before this moment.
  #[arg(long = "due-before", value_name = "moment")]
  due_before: Option<String>,
  /// Words the text holds, in any case.
  #[arg(long)]
  q: Option<String>,
  /// The last id seen.
  #[arg(long)]
  after: Option<i64>,
  /// How many rows at most.
  #[arg(long)]
  limit: Option<i64>,
}

fn state_of(name: &str) -> Result<ScopedMemoryState, String> {
  ScopedMemoryState::named(name)
    .ok_or_else(|| format!("'{name}' is no memory state: give standing, done, retired, candidate or off"))
}

/// One row as the command prints it.
pub fn line(memory: &ScopedMemory) -> String {
  let mut details = vec![format!("scope {}", memory.scope)];
  if let Some(subject) = &memory.subject_key {
    details.push(format!("subject {subject}"));
  }
  if let Some(due) = memory.due_at {
    details.push(format!("due {}", moment_text(due)));
  }
  if !memory.labels.is_empty() {
    details.push(format!("labels {}", labels_text(&memory.labels)));
  }
  format!(
    "memory: {} [{}] {}: {} ({})",
    memory.id,
    memory.state.name(),
    memory.kind,
    memory.text,
    details.join(", ")
  )
}

/// Runs the form its words name.
pub async fn run(client: &BrainClient, word: Word) -> Result<Vec<String>, String> {
  match word {
    Word::Add(asked) => add(client, asked).await,
    Word::Update(asked) => update(client, asked).await,
    Word::Done(asked) => done(client, asked).await,
    Word::Retire(asked) => moved(client, asked.id, ScopedMemoryState::Retired).await,
    Word::Revive(asked) => moved(client, asked.id, ScopedMemoryState::Standing).await,
    Word::Candidate(asked) => moved(client, asked.id, ScopedMemoryState::Candidate).await,
    Word::List(asked) => list(client, asked).await,
  }
}

async fn add(client: &BrainClient, asked: Add) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("memory: {why}");
  let conditions: Vec<Object> = asked
    .condition
    .iter()
    .map(|condition| object(condition))
    .collect::<Result<_, _>>()
    .map_err(refused)?;
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;
  let labels = labels(&asked.label).map_err(refused)?;
  let due_at = asked.due.as_deref().map(moment).transpose().map_err(refused)?;
  let text = text_of(&asked.text).map_err(refused)?;

  let added = client
    .scoped_memory_add(&NewScopedMemory {
      scope: asked.scope,
      subject_key: asked.subject,
      kind: asked.kind,
      text,
      conditions,
      labels,
      due_at,
      state,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  Ok(vec![line(&added)])
}

async fn update(client: &BrainClient, asked: Update) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("memory: {why}");
  if asked.text.is_none()
    && asked.state.is_none()
    && asked.label.is_empty()
    && asked.due.is_none()
    && !asked.no_due
  {
    return Err(refused("give --text, --state, --label, --due or --no-due".to_owned()));
  }
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;
  let text = asked.text.as_deref().map(text_of).transpose().map_err(refused)?;
  let labels: Option<Labels> =
    (!asked.label.is_empty()).then(|| labels(&asked.label)).transpose().map_err(refused)?;
  let due_at = if asked.no_due {
    Some(None)
  } else {
    asked.due.as_deref().map(moment).transpose().map_err(refused)?.map(Some)
  };

  let changed = client
    .scoped_memory_update(
      asked.id,
      &ScopedMemoryChange {
        text,
        state,
        labels,
        due_at,
      },
    )
    .await
    .map_err(|why| refused(why.to_string()))?;

  Ok(vec![line(&changed)])
}

async fn done(client: &BrainClient, asked: Done) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("memory: {why}");
  let text = asked.text.as_deref().map(text_of).transpose().map_err(refused)?;

  let changed = client
    .scoped_memory_update(
      asked.id,
      &ScopedMemoryChange {
        text,
        state: Some(ScopedMemoryState::Done),
        ..ScopedMemoryChange::default()
      },
    )
    .await
    .map_err(|why| refused(why.to_string()))?;

  Ok(vec![line(&changed)])
}

async fn moved(client: &BrainClient, id: i64, state: ScopedMemoryState) -> Result<Vec<String>, String> {
  let changed = client
    .scoped_memory_update(
      id,
      &ScopedMemoryChange {
        state: Some(state),
        ..ScopedMemoryChange::default()
      },
    )
    .await
    .map_err(|why| format!("memory: {why}"))?;

  Ok(vec![line(&changed)])
}

async fn list(client: &BrainClient, asked: List) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("memory: {why}");
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;
  let due_before = asked.due_before.as_deref().map(moment).transpose().map_err(refused)?;

  let page = client
    .scoped_memories(&ScopedMemoriesQuery {
      scope: asked.scope,
      scope_prefix: asked.scope_prefix,
      subject_key: asked.subject,
      kind: asked.kind,
      state,
      label: asked.label,
      due_before,
      q: asked.q,
      after: asked.after,
      limit: asked.limit,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  if page.items.is_empty() {
    return Ok(vec!["memory: no rows.".to_owned()]);
  }
  Ok(page.items.iter().map(line).collect())
}
