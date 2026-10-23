//! `bmo fragment` (docs/boundary-api.md, 「The command」): the fragment table, asked of 「Fragments」's routes.

use bmo_brain_client::{BrainClient, Fragment, FragmentState, FragmentSwitch, FragmentsQuery};
use clap::{Args, Subcommand};

use super::labels_text;

/// The forms `bmo fragment` takes.
#[derive(Subcommand)]
pub enum Word {
  /// Lists the fragments the filters name.
  List(List),
  /// Switches one fragment's state or the scopes it is off for, and prints it.
  Switch(Switch),
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
  /// The caller's name for the set they came in.
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
    Word::Switch(asked) => switched(client, asked).await,
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
