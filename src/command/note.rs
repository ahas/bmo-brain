//! `bmo note` (docs/boundary-api.md, 「The command」): a note kept for a later reading, asked of
//! 「Notes」's route.

use bmo_brain_client::{BrainClient, NewNote};
use clap::Args;

use super::text_of;

/// The words `bmo note` takes.
#[derive(Args)]
pub struct Asked {
  /// The scope the note is held under.
  #[arg(long)]
  scope: String,
  /// The caller's key for what the note is about.
  #[arg(long)]
  subject: Option<String>,
  /// The caller's name for who keeps it.
  #[arg(long)]
  actor: String,
  /// The note's text; standard input where none is given, and `-` names it.
  #[arg(long, allow_hyphen_values = true)]
  text: Option<String>,
}

/// Runs `bmo note`.
pub async fn run(client: &BrainClient, asked: Asked) -> Result<Vec<String>, String> {
  let refused = |why: String| format!("note: {why}");
  let text = match asked.text.as_deref() {
    Some(text) => text_of(text),
    None => text_of("-"),
  }
  .map_err(refused)?;

  let kept = client
    .note_add(&NewNote {
      scope: asked.scope,
      subject_key: asked.subject,
      text,
      actor_name: asked.actor,
    })
    .await
    .map_err(|why| refused(why.to_string()))?;

  let mut line = format!("note: {} {}: {}", kept.id, kept.scope, kept.text);
  if let Some(subject) = &kept.subject_key {
    line.push_str(&format!(" (subject {subject})"));
  }
  Ok(vec![line])
}
