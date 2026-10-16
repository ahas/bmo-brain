//! The desk an editor run writes its rows on: what it keeps of the folder as the run found it, where the run
//! stands, and what it reads back once the run ends.

mod support;

use std::path::{Path, PathBuf};

use bmo_brain::editors::DocumentBackup;
use bmo_brain::editors::desk::{DeskScope, ScopedDesk, close_desk, desk_brain, in_places, open_desk};
use bmo_brain::editors::editor::RunDesk;
use fragment_desk::desk::{Desk, Names};

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

#[test]
fn a_path_is_in_a_place_only_beneath_it() {
  let places = vec!["rooms/5/people/".to_owned()];

  assert!(in_places(&places, "rooms/5/people/chunks/p1-a.md"));
  assert!(!in_places(&places, "rooms/5/peoples/a.md"));
  assert!(!in_places(&places, "rooms/5/people"));
}
