//! Whose long-term memory a brain editor writes (docs/agent-ownership.md; docs/knowledge.md, 「Whose
//! library it is」): every awake agent the agency registers with a brain folder, the chat agent at the brain
//! root among them even before its registration says so, each with the label its task calls it by and the
//! registration its recollectors go out on where it names one.

use std::path::{Path, PathBuf};

use crate::Failure;
use crate::ports::agency::{Agency, Registration};

/// One agent's long-term memory, as a run writing it stands in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recollection {
  /// The agent's name, as its registration and every owner column carry it.
  pub agent: String,
  /// What its task calls it: its registration's label, or its name where it has none.
  pub label: String,
  /// Its folder, relative to the brain root; empty for the root itself.
  pub folder: String,
  /// The folder as a directory.
  pub brain: PathBuf,
  /// The registration every recollector run for it goes out on, or none, which leaves each recollector on its
  /// own work key's road.
  pub registration: Option<String>,
}

impl Recollection {
  /// A path relative to the agent's folder, as the table names it from the brain root.
  pub fn outward(&self, path: &str) -> String {
    if self.folder.is_empty() { path.to_owned() } else { format!("{}/{path}", self.folder) }
  }

  /// A path the table names from the brain root, as the run names it from the agent's folder.
  pub fn inward(&self, path: &str) -> String {
    match path.strip_prefix(&format!("{}/", self.folder)) {
      Some(inside) if !self.folder.is_empty() => inside.to_owned(),
      _ => path.to_owned(),
    }
  }

  /// The folder made where it is missing, since the run that writes it has file tools and no shell.
  pub fn stand(&self) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(&self.brain)?;

    Ok(self.brain.clone())
  }
}

fn of(brain_root: &Path, registration: Option<&Registration>, agent: &str, folder: String) -> Recollection {
  let brain = if folder.is_empty() { brain_root.to_path_buf() } else { brain_root.join(&folder) };
  let label = registration
    .and_then(|registration| registration.label.clone())
    .filter(|label| !label.is_empty())
    .unwrap_or_else(|| agent.to_owned());
  let road = registration
    .and_then(|registration| registration.recollector_registration.clone())
    .filter(|road| !road.is_empty());

  Recollection {
    agent: agent.to_owned(),
    label,
    folder,
    brain,
    registration: road,
  }
}

// Every awake agent's memory in the order the agency lists them, the chat agent's at the root first where no
// registration holds the root. An agent listed with no brain folder keeps no long-term memory.
async fn every(
  agency: &dyn Agency,
  brain_root: &Path,
  chat_key: &str,
) -> Result<Vec<Recollection>, Failure> {
  let registrations = agency.registrations(None).await?;
  let mut found: Vec<Recollection> = registrations
    .iter()
    .filter(|registration| registration.awake)
    .filter_map(|registration| {
      let folder = registration.brain_folder.as_deref()?;

      Some(of(
        brain_root,
        Some(registration),
        &registration.name,
        folder.trim_end_matches('/').to_owned(),
      ))
    })
    .collect();

  if !found.iter().any(|one| one.folder.is_empty()) {
    let chat = registrations.iter().find(|registration| registration.name == chat_key);

    found.insert(0, of(brain_root, chat, chat.map_or(chat_key, |chat| chat.name.as_str()), String::new()));
  }

  Ok(found)
}

/// The recollection of one agent, or none for an agent that keeps no long-term memory. `chat_key` is the chat
/// agent's registration key, whose memory is the brain's root.
pub async fn recollection_for(
  agency: &dyn Agency,
  brain_root: &Path,
  chat_key: &str,
  agent: &str,
) -> Result<Option<Recollection>, Failure> {
  Ok(every(agency, brain_root, chat_key).await?.into_iter().find(|one| one.agent == agent))
}

/// Every agent that keeps a long-term memory, the one whose memory is the brain's root first.
pub async fn recollections(
  agency: &dyn Agency,
  brain_root: &Path,
  chat_key: &str,
) -> Result<Vec<Recollection>, Failure> {
  let mut found = every(agency, brain_root, chat_key).await?;

  found.sort_by_key(|one| !one.folder.is_empty());

  Ok(found)
}
