//! The libraries as the desk asks them about a path, in whichever agent's folder it stands: the members its
//! caller hands in, asked in order, and none at all by default.

mod support;

use fragment_desk::libraries::Folders;
use fragment_desk::rows::AgentFolder;

use support::libraries;

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
  let held = folders().holding(libraries());

  assert_eq!(
    held.library_owning("rooms/-100/rules/greeting.md").map(|one| one.kind()),
    Some("room-rule")
  );
  assert_eq!(
    held.library_owning("agents/agent-b/knowledge/deploy.md").map(|one| one.kind()),
    Some("library-knowledge")
  );
  assert!(held.library_owning("knowledge/index.md").is_none());
  assert!(held.library_owning("people/7/sheet.md").is_none());
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
fn a_piece_takes_its_place_from_its_own_library_and_what_its_writer_said() {
  let held = folders().holding(libraries());
  let notes = held.library_owning("knowledge/a.md").unwrap();

  assert_eq!(
    held.place(notes, "knowledge/a.md", Some(true), Some(5)).unwrap().scope.as_deref(),
    Some("[shared]")
  );
  assert_eq!(
    held.place(notes, "knowledge/a.md", None, Some(5)).unwrap().scope.as_deref(),
    Some("[chat:5]")
  );
  assert!(held.place(notes, "knowledge/a.md", None, None).is_none());
}

#[test]
fn a_path_no_library_owns_is_refused_naming_what_each_library_holds() {
  assert_eq!(
    folders().holding(libraries()).no_piece("people/7/sheet.md"),
    "people/7/sheet.md is no piece of a brain library: a rule or a note. An index.md and notes/candidates.md are not pieces and have no row."
  );
}

#[test]
fn a_desk_handed_no_library_takes_no_piece() {
  let held = folders();

  assert!(held.library_owning("knowledge/deploy.md").is_none());
  assert!(held.library_owning("rooms/-100/rules/greeting.md").is_none());
  assert_eq!(
    held.no_piece("knowledge/deploy.md"),
    "knowledge/deploy.md is no piece of a brain library: this desk was handed none."
  );
}
