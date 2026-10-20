//! `bmo reminder list` (docs/boundary-api.md, 「The command」): the reminders, asked of 「Reminders」's
//! routes.

use bmo_brain_client::{BrainClient, Reminder, RemindersQuery};
use clap::{Args, Subcommand};

use super::{moment, moment_text};

/// The forms `bmo reminder` takes.
#[derive(Subcommand)]
pub enum Word {
  /// Lists the reminders the filters name, or cancels the one `--cancel` names.
  List(List),
}

/// Which reminders to list.
#[derive(Args)]
pub struct List {
  /// The scope they are held under; every scope where none is given.
  #[arg(long, conflicts_with = "cancel")]
  scope: Option<String>,
  /// The reminders due at or before this moment.
  #[arg(long = "due-before", value_name = "moment", conflicts_with = "cancel")]
  due_before: Option<String>,
  /// Only the reminders already fired.
  #[arg(long, conflicts_with_all = ["waiting", "cancel"])]
  fired: bool,
  /// Only the reminders still waiting.
  #[arg(long, conflicts_with_all = ["fired", "cancel"])]
  waiting: bool,
  /// The last id seen.
  #[arg(long, conflicts_with = "cancel")]
  after: Option<i64>,
  /// How many at most.
  #[arg(long, conflicts_with = "cancel")]
  limit: Option<i64>,
  /// Cancels this reminder instead of listing.
  #[arg(long, value_name = "id")]
  cancel: Option<i64>,
}

/// One reminder as the command prints it.
pub fn line(reminder: &Reminder) -> String {
  let mut line = format!(
    "reminder: {} {} {}: {}",
    reminder.id,
    moment_text(reminder.due_at),
    reminder.scope,
    reminder.text
  );
  if let Some(fired) = reminder.fired_at {
    line.push_str(&format!(" (fired {})", moment_text(fired)));
  }
  line
}

/// Runs the form its words name.
pub async fn run(client: &BrainClient, word: Word) -> Result<Vec<String>, String> {
  match word {
    Word::List(asked) => list(client, asked).await,
  }
}

async fn list(client: &BrainClient, asked: List) -> Result<Vec<String>, String> {
  if let Some(id) = asked.cancel {
    client.reminder_cancel(id).await.map_err(|why| format!("reminder: {why}"))?;
    return Ok(vec![format!("reminder: {id} cancelled.")]);
  }

  let refused = |why: String| format!("reminder: {why}");
  let due_before = asked.due_before.as_deref().map(moment).transpose().map_err(refused)?;
  let fired = if asked.fired {
    Some(true)
  } else if asked.waiting {
    Some(false)
  } else {
    None
  };

  let page = client
    .reminders(&RemindersQuery {
      scope: asked.scope,
      due_before,
      fired,
      after: asked.after,
      limit: asked.limit,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  if page.items.is_empty() {
    return Ok(vec!["reminder: no reminders.".to_owned()]);
  }
  Ok(page.items.iter().map(line).collect())
}
