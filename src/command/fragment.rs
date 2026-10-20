//! `bmo fragment` (docs/boundary-api.md, 「The command」): the fragment table and the libraries it is
//! synced from, asked of 「Fragments」's and 「Libraries」's routes.

use bmo_brain_client::{
  BrainClient, Fragment, FragmentState, FragmentSwitch, FragmentWrite, FragmentsQuery, LibrarySync,
  LibrarySyncAsk,
};
use clap::{Args, Subcommand};

use super::{labels, labels_text, text_of};

/// The forms `bmo fragment` takes.
#[derive(Subcommand)]
pub enum Word {
  /// Lists the fragments the filters name.
  List(List),
  /// Writes one fragment whole and prints it.
  Write(Write),
  /// Removes one fragment.
  Remove(Remove),
  /// Switches one fragment's state or the scopes it is off for, and prints it.
  Switch(Switch),
  /// Brings a library's fragments in line with its documents.
  Sync(Sync),
}

/// Which fragments to list.
#[derive(Args)]
pub struct List {
  /// The kind the fragments name.
  #[arg(long)]
  kind: Option<String>,
  /// The scope they are held under.
  #[arg(long)]
  scope: Option<String>,
  /// The caller's key for what they are about.
  #[arg(long)]
  subject: Option<String>,
  /// The library they were synced from.
  #[arg(long)]
  library: Option<String>,
  /// Whether they are offered.
  #[arg(long, value_name = "state")]
  state: Option<String>,
  /// Words the title or the text holds, in any case.
  #[arg(long)]
  q: Option<String>,
  /// Their ids, comma-separated.
  #[arg(long)]
  ids: Option<String>,
  /// Their keys, comma-separated.
  #[arg(long)]
  keys: Option<String>,
  /// The last id seen.
  #[arg(long)]
  after: Option<i64>,
  /// How many at most.
  #[arg(long)]
  limit: Option<i64>,
}

/// A fragment written whole.
#[derive(Args)]
pub struct Write {
  /// The fragment's key.
  key: String,
  /// The kind it names.
  #[arg(long)]
  kind: String,
  /// Its text; `-` reads it from standard input.
  #[arg(long, allow_hyphen_values = true)]
  text: String,
  /// The scope it is held under; every scope where none is given.
  #[arg(long)]
  scope: Option<String>,
  /// The caller's key for what it is about.
  #[arg(long)]
  subject: Option<String>,
  /// What it is.
  #[arg(long)]
  title: Option<String>,
  /// Where the caller keeps it.
  #[arg(long)]
  path: Option<String>,
  /// The library it belongs to.
  #[arg(long)]
  library: Option<String>,
  /// Whether it is offered.
  #[arg(long, value_name = "state")]
  state: Option<String>,
  /// A label as key=value, one flag each.
  #[arg(long, value_name = "key=value")]
  label: Vec<String>,
}

/// A fragment to remove.
#[derive(Args)]
pub struct Remove {
  /// The fragment's key.
  key: String,
}

/// A fragment switched.
#[derive(Args)]
pub struct Switch {
  /// The fragment's key.
  key: String,
  /// The state to move it to.
  #[arg(long, value_name = "state")]
  state: Option<String>,
  /// A scope it is switched off for, replacing the scopes it was off for; one flag each.
  #[arg(long = "off-for")]
  off_for: Vec<String>,
}

/// A library whose fragments to bring in line.
#[derive(Args)]
pub struct Sync {
  /// The library's name.
  library: String,
  /// Only says what would change, and changes nothing.
  #[arg(long)]
  dry_run: bool,
}

fn state_of(name: &str) -> Result<FragmentState, String> {
  FragmentState::named(name).ok_or_else(|| format!("'{name}' is no fragment state: give active or off"))
}

/// One fragment as the command prints it, without its text.
pub fn line(fragment: &Fragment) -> String {
  let scope = fragment.scope.as_deref().unwrap_or("shared");
  let mut line = format!(
    "fragment: {} [{}] {} {} {} (version {})",
    fragment.key,
    fragment.state.name(),
    fragment.kind,
    scope,
    fragment.title.as_deref().unwrap_or("(untitled)"),
    fragment.version
  );
  if let Some(library) = &fragment.library_name {
    line.push_str(&format!(" from library {library}"));
  }
  if !fragment.labels.is_empty() {
    line.push_str(&format!(" (labels {})", labels_text(&fragment.labels)));
  }
  line
}

/// Runs the form its words name.
pub async fn run(client: &BrainClient, word: Word) -> Result<Vec<String>, String> {
  match word {
    Word::List(asked) => list(client, asked).await,
    Word::Write(asked) => write(client, asked).await,
    Word::Remove(asked) => remove(client, asked).await,
    Word::Switch(asked) => switched(client, asked).await,
    Word::Sync(asked) => synced(client, asked).await,
  }
}

async fn list(client: &BrainClient, asked: List) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("fragment: {why}");
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;

  let page = client
    .fragments(&FragmentsQuery {
      kind: asked.kind,
      scope: asked.scope,
      subject_key: asked.subject,
      library_name: asked.library,
      state,
      q: asked.q,
      ids: asked.ids,
      keys: asked.keys,
      after: asked.after,
      limit: asked.limit,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  if page.items.is_empty() {
    return Ok(vec!["fragment: no fragments.".to_owned()]);
  }
  Ok(page.items.iter().map(line).collect())
}

async fn write(client: &BrainClient, asked: Write) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("fragment: {why}");
  let labels = labels(&asked.label).map_err(refused)?;
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;
  let text = text_of(&asked.text).map_err(refused)?;

  let written = client
    .fragment_put(
      &asked.key,
      &FragmentWrite {
        kind: asked.kind,
        text,
        scope: asked.scope,
        subject_key: asked.subject,
        title: asked.title,
        path: asked.path,
        library_name: asked.library,
        state,
        switched_off: None,
        labels: (!labels.is_empty()).then_some(labels),
      },
    )
    .await
    .map_err(|why| refused(why.to_string()))?;

  Ok(vec![line(&written)])
}

async fn remove(client: &BrainClient, asked: Remove) -> Result<Vec<String>, String> {
  client.fragment_remove(&asked.key).await.map_err(|why| format!("fragment: {why}"))?;

  Ok(vec![format!("fragment: {} removed.", asked.key)])
}

async fn switched(client: &BrainClient, asked: Switch) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("fragment: {why}");
  if asked.state.is_none() && asked.off_for.is_empty() {
    return Err(refused("give --state or --off-for".to_owned()));
  }
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;

  let switched = client
    .fragment_switch(
      &asked.key,
      &FragmentSwitch {
        switched_off: (!asked.off_for.is_empty()).then_some(asked.off_for),
        state,
      },
    )
    .await
    .map_err(|why| refused(why.to_string()))?;

  Ok(vec![line(&switched)])
}

async fn synced(client: &BrainClient, asked: Sync) -> Result<Vec<String>, String> {
  let sync: LibrarySync = client
    .library_sync(
      &LibrarySyncAsk {
        library_name: asked.library.clone(),
      },
      asked.dry_run,
    )
    .await
    .map_err(|why| format!("fragment: {why}"))?;

  let ending = if sync.applied { "applied" } else { "not applied" };
  if sync.drift.is_empty() {
    return Ok(vec![format!("fragment: {}: in line with its documents.", sync.library_name)]);
  }

  let mut lines = vec![format!("fragment: {}: {} changes, {ending}.", sync.library_name, sync.drift.len())];
  for drift in &sync.drift {
    lines.push(format!(
      "fragment: {} {}",
      drift.key,
      match drift.change {
        bmo_brain_client::DriftChange::Added => "added",
        bmo_brain_client::DriftChange::Changed => "changed",
        bmo_brain_client::DriftChange::Removed => "removed",
        bmo_brain_client::DriftChange::Claimed => "claimed",
      }
    ));
  }
  Ok(lines)
}
