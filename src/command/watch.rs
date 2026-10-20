//! `bmo watch set` and `bmo watch list` (docs/boundary-api.md, 「The command」): the watches, asked of
//! 「Watches」's routes.

use bmo_brain_client::{
  BrainClient, Labels, MemoryState, NewWatch, Object, Watch, WatchState, WatchesQuery,
};
use clap::{Args, Subcommand};
use serde_json::Value;

use super::{labels, labels_text, moment, moment_text};

/// The forms `bmo watch` takes.
#[derive(Subcommand)]
pub enum Word {
  /// Sets a watch and prints it.
  Set(Set),
  /// Lists the watches the filters name.
  List(List),
}

/// A watch to set.
#[derive(Args)]
#[command(group = clap::ArgGroup::new("condition").required(true).args(["at", "memory", "external"]))]
pub struct Set {
  /// The scope the watch is held under.
  #[arg(long)]
  scope: String,
  /// The caller's name for who sets it.
  #[arg(long)]
  setter: String,
  /// Fires once this moment has come.
  #[arg(long, value_name = "moment", conflicts_with_all = ["memory", "external"])]
  at: Option<String>,
  /// Fires once this row is in the state `--state` names.
  #[arg(long, value_name = "id", conflicts_with_all = ["at", "external"])]
  memory: Option<i64>,
  /// The row state the watch waits for, with `--memory`.
  #[arg(long, value_name = "state", requires = "memory")]
  state: Option<String>,
  /// Fires when a client moves the watch with this key to fired.
  #[arg(long, value_name = "key", conflicts_with_all = ["at", "memory"])]
  external: Option<String>,
  /// A label as key=value, one flag each.
  #[arg(long, value_name = "key=value")]
  label: Vec<String>,
}

/// Which watches to list.
#[derive(Args)]
pub struct List {
  /// The scope they are held under.
  #[arg(long)]
  scope: Option<String>,
  /// The caller's name for who set them.
  #[arg(long)]
  setter: Option<String>,
  /// The caller's name for who holds them.
  #[arg(long)]
  holder: Option<String>,
  /// The state they are in.
  #[arg(long, value_name = "state")]
  state: Option<String>,
  /// The last id seen.
  #[arg(long)]
  after: Option<i64>,
  /// How many at most.
  #[arg(long)]
  limit: Option<i64>,
}

fn state_of(name: &str) -> Result<WatchState, String> {
  WatchState::named(name)
    .ok_or_else(|| format!("'{name}' is no watch state: give armed, fired, claimed, ended or cancelled"))
}

/// One watch as the command prints it.
pub fn line(watch: &Watch) -> String {
  let mut line = format!(
    "watch: {} [{}] {} {}",
    watch.id,
    watch.state.name(),
    watch.scope,
    Value::Object(watch.condition.clone())
  );
  if let Some(holder) = &watch.holder_name {
    line.push_str(&format!(" held by {holder}"));
  }
  if let Some(fired) = watch.fired_at {
    line.push_str(&format!(" (fired {})", moment_text(fired)));
  }
  if !watch.labels.is_empty() {
    line.push_str(&format!(" (labels {})", labels_text(&watch.labels)));
  }
  line
}

/// Runs the form its words name.
pub async fn run(client: &BrainClient, word: Word) -> Result<Vec<String>, String> {
  match word {
    Word::Set(asked) => set(client, asked).await,
    Word::List(asked) => list(client, asked).await,
  }
}

async fn set(client: &BrainClient, asked: Set) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("watch: {why}");
  let condition: Object = if let Some(at) = &asked.at {
    let at = moment(at).map_err(refused)?;
    [("kind".to_owned(), Value::from("at")), ("at".to_owned(), Value::from(at))]
      .into_iter()
      .collect()
  } else if let Some(memory) = asked.memory {
    let state = asked
      .state
      .as_deref()
      .map(|name| {
        MemoryState::named(name).ok_or_else(|| {
          format!("'{name}' is no memory state: give standing, done, retired, candidate or off")
        })
      })
      .transpose()
      .map_err(refused)?
      .ok_or_else(|| refused("--memory takes the state it waits for, as --state".to_owned()))?;
    [
      ("kind".to_owned(), Value::from("memory")),
      ("memoryId".to_owned(), Value::from(memory)),
      ("state".to_owned(), Value::from(state.name())),
    ]
    .into_iter()
    .collect()
  } else if let Some(key) = &asked.external {
    [
      ("kind".to_owned(), Value::from("external")),
      ("key".to_owned(), Value::from(key.as_str())),
    ]
    .into_iter()
    .collect()
  } else {
    return Err(refused("give --at, --memory or --external".to_owned()));
  };
  let labels: Labels = labels(&asked.label).map_err(refused)?;

  let set = client
    .watch_add(&NewWatch {
      scope: asked.scope,
      condition,
      setter_name: asked.setter,
      labels,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  Ok(vec![line(&set)])
}

async fn list(client: &BrainClient, asked: List) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("watch: {why}");
  let state = asked.state.as_deref().map(state_of).transpose().map_err(refused)?;

  let page = client
    .watches(&WatchesQuery {
      scope: asked.scope,
      setter_name: asked.setter,
      holder_name: asked.holder,
      state,
      after: asked.after,
      limit: asked.limit,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  if page.items.is_empty() {
    return Ok(vec!["watch: no watches.".to_owned()]);
  }
  Ok(page.items.iter().map(line).collect())
}
