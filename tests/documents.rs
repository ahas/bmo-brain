//! The documents an editor run stands among: what is backed up and put back, what the run may have written,
//! how a batch's people are shared out, and what the archive knows that a common document may not name.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bmo_brain::editors::book::{KnownIdentifiers, PersonPiece};
use bmo_brain::editors::documents::{
  BatchDocuments, RunClaim, WholeRoomRun, backup_documents, document_list, known_identifiers,
  library_documents, mark_sorted, restore_documents, room_book_people, run_touched, sender_groups,
  whole_room_commit, within_ceiling, write_problems, write_problems_cause,
};
use bmo_brain::ports::receiver::RoomTitle;

use support::agency::{FakeAgency, registration};
use support::receiver::FakeReceiver;
use support::{AGENT, OTHER_AGENT};

fn write(root: &Path, path: &str, text: &str) {
  let at = root.join(path);

  fs::create_dir_all(at.parent().unwrap()).unwrap();
  fs::write(at, text).unwrap();
}

#[test]
fn only_the_libraries_markdown_documents_are_backed_up() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  write(root, "rooms/1/people/chunks/p2-a.md", "a");
  write(root, "people/2/chunks/c2-b.md", "b");
  write(root, "knowledge/c.md", "c");
  write(root, "knowledge/c.txt", "c");
  write(root, "notes/d.md", "d");

  assert_eq!(
    library_documents(root),
    vec!["knowledge/c.md", "people/2/chunks/c2-b.md", "rooms/1/people/chunks/p2-a.md"]
  );
}

#[test]
fn a_runs_own_writing_is_put_back_and_another_writers_left_standing() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  write(root, "rooms/1/people/chunks/p2-a.md", "before");
  write(root, "knowledge/k.md", "kept");

  let backup = backup_documents(root);

  write(root, "rooms/1/people/chunks/p2-a.md", "after");
  write(root, "rooms/1/people/chunks/p2-new.md", "new");
  write(root, "knowledge/k.md", "another writer");

  let claim = RunClaim {
    places: vec!["rooms/1/people".into()],
    named: BTreeSet::new(),
  };
  let touched = run_touched(root, &backup, &claim);

  assert_eq!(touched.others, vec!["knowledge/k.md"]);

  let put_back = restore_documents(root, &backup, &claim);

  assert_eq!(put_back.len(), 2);
  assert_eq!(fs::read_to_string(root.join("rooms/1/people/chunks/p2-a.md")).unwrap(), "before");
  assert!(!root.join("rooms/1/people/chunks/p2-new.md").exists());
  assert_eq!(fs::read_to_string(root.join("knowledge/k.md")).unwrap(), "another writer");
}

#[test]
fn a_document_the_runs_own_tools_named_outside_its_places_is_its_problem() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  let backup = backup_documents(root);

  write(root, "knowledge/k.md", "written");
  write(root, "rooms/1/people/chunks/p3-x.md", "someone else's");
  write(root, "people/2/chunks/c2-a.md", "talked with 9 in Family Room");

  let claim = RunClaim {
    places: vec!["rooms/1/people".into(), "people/2".into()],
    named: [root.join("knowledge/k.md")].into_iter().collect(),
  };
  let known = KnownIdentifiers {
    user_ids: vec![2, 9],
    chat_ids: vec![],
    titles: vec!["Family Room".into()],
  };
  let problems = write_problems(root, &backup, &claim, &known, Some(&[2]));

  assert_eq!(
    problems,
    vec![
      "wrote outside its place: knowledge/k.md".to_owned(),
      "wrote the room piece of somebody this run does not write for: rooms/1/people/chunks/p3-x.md"
        .to_owned(),
      "people/2/chunks/c2-a.md names user id 9, room title Family Room".to_owned(),
    ]
  );
  assert_eq!(
    write_problems_cause(&problems),
    "The check on what the run wrote stopped it: the run's own file tools wrote 1 document(s) outside the part of the brain it may write (knowledge/k.md); the run changed 1 room piece(s) of people it was not writing for (rooms/1/people/chunks/p3-x.md); 1 common document(s) it wrote named another person's id or a room, which a common document may not. So everything the run wrote was put back."
  );
}

#[test]
fn a_room_book_names_everyone_with_a_sheet_or_a_piece() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  assert_eq!(room_book_people(root, "rooms/1/people"), None);

  write(root, "rooms/1/people/30.md", "sheet");
  write(root, "rooms/1/people/index.md", "index");
  write(root, "rooms/1/people/chunks/p4-a.md", "a");

  assert_eq!(room_book_people(root, "rooms/1/people"), Some(vec![4, 30]));
}

#[test]
fn a_batch_is_shared_out_under_the_ceiling_and_a_person_is_never_split() {
  let piece = |text: &str| PersonPiece {
    path: "x".into(),
    text: text.into(),
  };
  let documents = BatchDocuments {
    shared: vec![piece("ii")],
    by_sender: vec![(1, vec![piece("aaaa")]), (2, vec![piece("bbbb")]), (3, vec![piece(&"c".repeat(20))])],
  };

  assert_eq!(sender_groups(&documents, 10), vec![vec![1, 2], vec![3]]);
  assert_eq!(document_list(&documents, &[2]).len(), 2);
}

#[test]
fn one_persons_pieces_are_handed_in_order_while_they_fit_and_one_too_large_is_left_out() {
  let piece = |path: &str, chars: usize| PersonPiece {
    path: path.into(),
    text: "x".repeat(chars),
  };
  let (handed, held) =
    within_ceiling(vec![piece("a", 4), piece("ongoing", 20), piece("b", 5), piece("c", 2)], 10);
  let paths = |pieces: &[PersonPiece]| pieces.iter().map(|piece| piece.path.clone()).collect::<Vec<_>>();

  assert_eq!(paths(&handed), ["a", "b"]);
  assert_eq!(paths(&held), ["ongoing", "c"]);
}

#[test]
fn the_sorted_list_names_only_pieces_that_stand() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  write(root, "rooms/1/people/chunks/p4-a.md", "a");
  mark_sorted(root, "rooms/1/people", &["p4-a".into(), "p4-gone".into()]);

  assert_eq!(fs::read_to_string(root.join("rooms/1/people/sorted.txt")).unwrap(), "p4-a\n");

  fs::remove_file(root.join("rooms/1/people/chunks/p4-a.md")).unwrap();
  mark_sorted(root, "rooms/1/people", &[]);

  assert!(!root.join("rooms/1/people/sorted.txt").exists());
}

#[test]
fn a_whole_room_sheet_rewrite_commits_its_peoples_common_books() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  write(root, "rooms/1/people/4.md", "sheet");

  let (paths, message) = whole_room_commit(root, 1, WholeRoomRun::Sheets);

  assert_eq!(paths, vec!["rooms/1/people", "people/4"]);
  assert_eq!(message, "Room 1's standing sheets are written again from the pieces");
}

#[tokio::test]
async fn what_the_archive_knows_is_every_room_its_senders_and_its_titles_but_an_agents_name() {
  let agency = FakeAgency::default();
  let mut labelled = registration(OTHER_AGENT);
  let mut asleep = registration("agent-c");

  labelled.label = Some("Helper".into());
  asleep.awake = false;
  *agency.registrations.lock().unwrap() = vec![registration(AGENT), labelled, asleep];

  let receiver = FakeReceiver::default();

  *receiver.titles.lock().unwrap() = vec![
    RoomTitle {
      chat_id: -7,
      title: Some("Family Room".into()),
      senders: vec![9, 4],
    },
    RoomTitle {
      chat_id: -3,
      title: Some(" helper ".into()),
      senders: vec![4],
    },
    RoomTitle {
      chat_id: 4,
      title: Some("agent-c".into()),
      senders: vec![4],
    },
    RoomTitle {
      chat_id: -1,
      title: None,
      senders: vec![],
    },
  ];

  let known = known_identifiers(&agency, &receiver).await.unwrap();

  assert_eq!(known.user_ids, vec![4, 9]);
  assert_eq!(known.chat_ids, vec![-7, -3, -1, 4]);
  assert_eq!(known.titles, vec!["Family Room".to_owned(), "agent-c".to_owned()]);
}
