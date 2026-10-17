//! The desk an editor run writes its rows on: what it keeps of the folder as the run found it, where the run
//! stands, what it reads back once the run ends, and the brain's libraries as the desk is handed them.

mod support;

use std::path::{Path, PathBuf};

use bmo_brain::editors::DocumentBackup;
use bmo_brain::editors::desk::{DeskScope, ScopedDesk, close_desk, desk_brain, in_places, open_desk};
use bmo_brain::editors::editor::RunDesk;
use bmo_brain::editors::fragments::libraries::{LIBRARIES, on_the_desk};
use fragment_desk::desk::{Desk, Names};
use fragment_desk::libraries::Folders;
use fragment_desk::rows::AgentFolder;

use support::AGENT;

fn names() -> Names {
  Names {
    agent: AGENT.to_owned(),
    agent_key: AGENT.to_owned(),
    common_words: vec![],
  }
}

fn scope() -> DeskScope {
  DeskScope {
    room: Some(5),
    agent: Some(AGENT.into()),
    folder: format!("agents/{AGENT}"),
    places: vec!["rooms/5/people".into()],
    ..DeskScope::default()
  }
}

#[test]
fn a_desk_keeps_the_pieces_under_its_places_and_reads_back_what_was_written() {
  let dir = tempfile::tempdir().unwrap();
  let mut before = DocumentBackup::new();

  before.insert("rooms/5/people/chunks/p1-a.md".into(), b"# a\n".to_vec());
  before.insert("knowledge/b.md".into(), b"# b\n".to_vec());

  let path =
    open_desk(&scope(), dir.path(), "postgres://archive", Path::new("/brain"), &before, &names()).unwrap();
  let mut desk = Desk::read(&path).unwrap();

  assert_eq!(desk.brain, format!("/brain/agents/{AGENT}"));
  assert_eq!(desk.before.keys().collect::<Vec<_>>(), vec!["rooms/5/people/chunks/p1-a.md"]);
  assert_eq!(desk.names, Some(names()));
  assert_eq!(desk_brain(&path), Some(PathBuf::from(format!("/brain/agents/{AGENT}"))));

  desk.writes.push(fragment_desk::meta::removal_write("rooms/5/people/chunks/p1-a.md"));
  desk.save(&path).unwrap();

  assert_eq!(close_desk(&path).len(), 1);
  assert!(!path.exists());
  assert!(close_desk(&path).is_empty());
}

#[test]
fn a_scoped_desk_opens_its_folder_and_a_fresh_desk_each_time() {
  let dir = tempfile::tempdir().unwrap();
  let before = DocumentBackup::new();
  let names = names();
  let desk = ScopedDesk {
    scope: scope(),
    dir: dir.path().join("fragment-desks"),
    db: "postgres://archive",
    brain: Path::new("/brain"),
    before: &before,
    names: &names,
  };
  let first = desk.open().unwrap();
  let second = desk.open().unwrap();

  assert_ne!(first, second);
  assert!(first.starts_with(dir.path().join("fragment-desks")));
  assert_eq!(desk.brain(&first), Some(PathBuf::from(format!("/brain/agents/{AGENT}"))));
  assert!(desk.close(&first).is_empty());
  assert!(!first.exists());
}

// The brain's folders as a desk holds them, handed the brain's libraries.
fn brain_folders() -> Folders {
  Folders::new(
    vec![AgentFolder {
      agent: "agent-b".into(),
      folder: "agents/agent-b".into(),
    }],
    AGENT,
  )
  .holding(on_the_desk())
}

#[test]
fn the_desk_is_handed_the_brains_libraries_in_their_own_order() {
  let kinds: Vec<&str> = on_the_desk().iter().map(|library| library.kind()).collect();

  assert_eq!(kinds, LIBRARIES.map(|library| library.kind()));
}

#[test]
fn a_piece_is_its_librarys_in_whichever_agents_folder_it_stands() {
  let held = brain_folders();
  let kind_of = |path: &str| held.library_owning(path).map(|one| one.kind());

  assert_eq!(kind_of("rooms/-5/rules/greeting.md"), Some("room-rule"));
  assert_eq!(kind_of("agents/agent-b/knowledge/deploy.md"), Some("library-knowledge"));
  assert_eq!(kind_of("people/7/chunks/c7-address.md"), Some("person-book"));
  assert_eq!(kind_of("rooms/5/people/chunks/p7-address.md"), Some("person-book"));
  assert_eq!(kind_of("people/7/chunks/c8-address.md"), None);
  assert_eq!(kind_of("knowledge/index.md"), None);
  assert_eq!(kind_of("knowledge/candidates.md"), None);
  assert_eq!(kind_of("rooms/-5/rules/index.md"), None);
  assert_eq!(held.agent_of("agents/agent-b/knowledge/deploy.md"), ("agent-b", "knowledge/deploy.md"));
  assert_eq!(held.agent_of("knowledge/deploy.md"), (AGENT, "knowledge/deploy.md"));
}

#[test]
fn a_piece_answers_to_the_row_its_own_library_places_it_at() {
  let held = brain_folders();
  let place = |path: &str, shared: Option<bool>, room: Option<i64>| {
    let library = held.library_owning(path).unwrap();

    held.place(library, path, shared, room)
  };
  let knowledge = place("knowledge/a.md", Some(true), Some(5)).unwrap();

  assert_eq!((knowledge.slug.as_str(), knowledge.scope.as_deref()), ("a", Some("[shared]")));
  assert_eq!(place("knowledge/a.md", None, Some(5)).unwrap().scope.as_deref(), Some("[chat:5]"));
  assert_eq!(
    place("knowledge/a.md", Some(false), Some(5)).unwrap().scope.as_deref(),
    Some("[chat:5]")
  );
  assert!(place("knowledge/a.md", None, None).is_none());

  let rule = place("rooms/-5/rules/greeting.md", None, Some(5)).unwrap();

  assert_eq!((rule.owner, rule.slug.as_str(), rule.scope), (None, "greeting", None));

  let common = place("agents/agent-b/people/7/chunks/c7-address.md", None, Some(5)).unwrap();
  let room = place("rooms/5/people/chunks/p7-address.md", None, Some(5)).unwrap();

  assert_eq!((common.owner, common.slug.as_str()), (Some(7), "c7-address"));
  assert_eq!((room.owner, room.slug.as_str(), room.scope), (Some(7), "p7-address", None));
}

#[test]
fn a_path_no_brain_library_owns_is_refused_naming_each_librarys_pieces() {
  assert_eq!(
    brain_folders().no_piece("people/7/sheet.md"),
    "people/7/sheet.md is no piece of a brain library: a room rule chunk, a knowledge chunk or a person-book piece under chunks/. An index.md, knowledge/candidates.md and a standing sheet are not pieces and have no row."
  );
}

#[test]
fn a_path_is_in_a_place_only_beneath_it() {
  let places = vec!["rooms/5/people/".to_owned()];

  assert!(in_places(&places, "rooms/5/people/chunks/p1-a.md"));
  assert!(!in_places(&places, "rooms/5/peoples/a.md"));
  assert!(!in_places(&places, "rooms/5/people"));
}
