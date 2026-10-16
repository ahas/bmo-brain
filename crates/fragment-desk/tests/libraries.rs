//! The brain libraries as the desk asks them about a path, in whichever agent's folder it stands.

use fragment_desk::libraries::{Folders, LIBRARIES};
use fragment_desk::rows::AgentFolder;

fn folders() -> Folders {
  Folders::new(
    vec![AgentFolder {
      agent: "agent-b".into(),
      folder: "agents/agent-b".into(),
    }],
    "agent-a",
  )
}

#[test]
fn a_piece_is_its_librarys_in_whichever_agents_folder_it_stands() {
  let held = folders();

  assert_eq!(held.library_owning("rooms/-5/rules/greeting.md").map(|one| one.kind), Some("room-rule"));
  assert_eq!(
    held.library_owning("agents/agent-b/knowledge/deploy.md").map(|one| one.kind),
    Some("library-knowledge")
  );
  assert_eq!(
    held.library_owning("people/7/chunks/c7-address.md").map(|one| one.kind),
    Some("person-book")
  );
  assert!(held.library_owning("people/7/chunks/c8-address.md").is_none());
  assert!(held.library_owning("knowledge/index.md").is_none());
  assert_eq!(held.agent_of("agents/agent-b/knowledge/deploy.md"), ("agent-b", "knowledge/deploy.md"));
  assert_eq!(held.agent_of("knowledge/deploy.md"), ("agent-a", "knowledge/deploy.md"));
}

#[test]
fn a_folder_handed_in_with_a_closing_slash_still_holds_its_pieces() {
  let held = Folders::new(
    vec![AgentFolder {
      agent: "agent-b".into(),
      folder: "agents/agent-b/".into(),
    }],
    "agent-a",
  );

  assert_eq!(held.agent_of("agents/agent-b/knowledge/deploy.md"), ("agent-b", "knowledge/deploy.md"));
}

#[test]
fn a_knowledge_chunk_takes_its_scope_from_what_its_writer_said() {
  let held = folders();
  let knowledge = &LIBRARIES[1];

  assert_eq!(
    held.place(knowledge, "knowledge/a.md", Some(true), Some(5)).unwrap().scope.as_deref(),
    Some("[shared]")
  );
  assert_eq!(
    held.place(knowledge, "knowledge/a.md", None, Some(5)).unwrap().scope.as_deref(),
    Some("[chat:5]")
  );
  assert!(held.place(knowledge, "knowledge/a.md", None, None).is_none());
}
