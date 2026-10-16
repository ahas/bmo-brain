//! Whose long-term memory an editor writes: every awake agent with a brain folder, the chat agent at the root
//! among them, and how a path goes out to the table and comes back in.

mod support;

use std::path::{Path, PathBuf};

use bmo_brain::editors::recollection::{Recollection, recollection_for, recollections};

use support::agency::{FakeAgency, registration};
use support::{AGENT, OTHER_AGENT};

fn at(folder: &str) -> Recollection {
  Recollection {
    agent: OTHER_AGENT.into(),
    label: "Helper".into(),
    folder: folder.into(),
    brain: PathBuf::from("/brain").join(folder),
    registration: None,
  }
}

#[test]
fn a_companions_paths_go_out_to_the_table_and_come_back_in() {
  let companion = at(&format!("agents/{OTHER_AGENT}"));

  assert_eq!(
    companion.outward("people/1/chunks/c1-x.md"),
    format!("agents/{OTHER_AGENT}/people/1/chunks/c1-x.md")
  );
  assert_eq!(
    companion.inward(&format!("agents/{OTHER_AGENT}/people/1/chunks/c1-x.md")),
    "people/1/chunks/c1-x.md"
  );
  assert_eq!(companion.inward("people/1/chunks/c1-x.md"), "people/1/chunks/c1-x.md");
}

#[test]
fn the_chat_agents_paths_stand_as_they_are() {
  let root = at("");

  assert_eq!(root.outward("knowledge/a.md"), "knowledge/a.md");
  assert_eq!(root.inward("knowledge/a.md"), "knowledge/a.md");
}

fn agency() -> FakeAgency {
  let agency = FakeAgency::default();
  let mut chat = registration(AGENT);
  let mut companion = registration(OTHER_AGENT);
  let mut asleep = registration("agent-c");

  chat.brain_folder = String::new();
  chat.label = Some("Chat".into());
  companion.brain_folder = format!("agents/{OTHER_AGENT}/");
  companion.recollector_registration = Some("road-b".into());
  asleep.awake = false;
  *agency.registrations.lock().unwrap() = vec![companion, chat, asleep];
  agency
}

#[tokio::test]
async fn every_awake_agent_keeps_a_memory_the_root_first() {
  let agency = agency();
  let found = recollections(&agency, Path::new("/brain"), AGENT).await.unwrap();

  assert_eq!(
    found,
    vec![
      Recollection {
        agent: AGENT.into(),
        label: "Chat".into(),
        folder: String::new(),
        brain: PathBuf::from("/brain"),
        registration: None,
      },
      Recollection {
        agent: OTHER_AGENT.into(),
        label: OTHER_AGENT.into(),
        folder: format!("agents/{OTHER_AGENT}"),
        brain: PathBuf::from(format!("/brain/agents/{OTHER_AGENT}")),
        registration: Some("road-b".into()),
      },
    ]
  );
  assert_eq!(recollection_for(&agency, Path::new("/brain"), AGENT, "agent-c").await.unwrap(), None);
}

#[tokio::test]
async fn the_chat_agent_stands_at_the_root_before_its_registration_says_so() {
  let agency = FakeAgency::default();
  let mut companion = registration(OTHER_AGENT);

  companion.brain_folder = format!("agents/{OTHER_AGENT}");
  *agency.registrations.lock().unwrap() = vec![companion];

  let root = recollection_for(&agency, Path::new("/brain"), AGENT, AGENT).await.unwrap().unwrap();

  assert_eq!(root.folder, "");
  assert_eq!(root.label, AGENT);
  assert_eq!(recollections(&agency, Path::new("/brain"), AGENT).await.unwrap().len(), 2);
}
