//! The person book's shape: its fixed headings, its numbered pieces, how an older document is cut into
//! stretches, and what a common document may not name.

mod support;

use std::fs;

use bmo_brain::editors::book::{
  FixedHeading, KnownIdentifiers, SPAN_CHARS, book_being_cut, common_boundary_violations, fixed_heading,
  fixed_heading_slug, numbered_piece_slug, outside_surface, person_spans, room_books, said_length,
  span_prefix,
};
use bmo_brain::policy::BRAIN;

use support::agency::{FakeAgency, registration};
use support::{AGENT, OTHER_AGENT};

#[test]
fn a_fixed_heading_is_its_persons_letter_id_and_topic() {
  assert!(fixed_heading_slug("c12-told"));
  assert!(fixed_heading_slug("p12-address"));
  assert!(!fixed_heading_slug("p12-address-book"));
  assert!(!fixed_heading_slug("x12-told"));
  assert!(!fixed_heading_slug("c-told"));
}

#[test]
fn a_fixed_heading_names_its_part_its_person_and_the_name_the_book_writes() {
  assert_eq!(
    fixed_heading("p56-ongoing"),
    Some(FixedHeading {
      common: false,
      user_id: 56,
      topic: "ongoing",
      name: "Ongoing and promises",
    })
  );
  assert!(
    fixed_heading("c56-requests")
      .is_some_and(|heading| heading.common && heading.name == "Working requests")
  );
  assert_eq!(fixed_heading("p56-02-ongoing"), None);
}

#[test]
fn a_numbered_piece_carries_two_digits_or_more_after_its_person() {
  assert!(numbered_piece_slug("p7-01-work", 7));
  assert!(numbered_piece_slug("p7-123-work", 7));
  assert!(!numbered_piece_slug("p7-1-work", 7));
  assert!(!numbered_piece_slug("p7-work", 7));
  assert!(!numbered_piece_slug("p77-01-work", 7));
  assert_eq!(span_prefix(7, 3), "p7-03");
}

#[test]
fn hangul_says_twice_what_a_latin_letter_does() {
  assert_eq!(said_length("ab"), 2);
  assert_eq!(said_length("\u{AC00}\u{AC01}"), 4);
}

#[test]
fn stretches_end_on_sentences_and_put_back_together_exactly() {
  let text = "One sentence here. Another one there.\n\nA paragraph after. And the last line.";
  let spans = person_spans(text, 20);

  assert_eq!(spans.concat(), text);
  assert!(spans.len() > 1);
  assert!(
    spans
      .iter()
      .take(spans.len() - 1)
      .all(|span| span.ends_with(' ') || span.ends_with('\n'))
  );
  assert!(person_spans("", 20).is_empty());
}

#[test]
fn a_short_last_stretch_rides_with_the_one_before() {
  let text = format!("{}. {}. x.", "a".repeat(40), "b".repeat(40));
  let spans = person_spans(&text, 40);

  assert_eq!(spans.concat(), text);
  assert_eq!(spans.len(), 2);
  assert!(spans[1].ends_with("b. x."));
}

#[test]
fn a_book_under_the_ceiling_is_never_being_cut() {
  assert!(!book_being_cut("short", &[], 1));
}

#[test]
fn a_book_over_the_ceiling_is_cut_until_every_stretch_has_a_piece() {
  let text = format!("{}.\n\n", "a".repeat(BRAIN.sheets.room_ceiling + 10));
  let spans = person_spans(&text, SPAN_CHARS).len();
  let all: Vec<String> = (1..=spans).map(|span| format!("{}-x", span_prefix(4, span))).collect();

  assert!(book_being_cut(&text, &[], 4));
  assert!(!book_being_cut(&text, &all, 4));
  assert!(!book_being_cut(&text, &["p4-work".to_owned()], 4));
}

#[test]
fn a_common_document_may_name_its_own_person_and_nothing_else_the_archive_knows() {
  let known = KnownIdentifiers {
    user_ids: vec![5, 9],
    chat_ids: vec![-100_123, 5],
    titles: vec!["Family Room".into(), "x".into()],
  };

  assert!(common_boundary_violations("person 5 on 2026-03-04", 5, &known).is_empty());
  assert_eq!(common_boundary_violations("talked with 9", 5, &known), vec!["user id 9"]);
  assert_eq!(common_boundary_violations("in -100123", 5, &known), vec!["chat id -100123"]);
  assert_eq!(
    common_boundary_violations("from the Family Room room", 5, &known),
    vec!["room title Family Room"]
  );
  assert!(common_boundary_violations("x marks", 5, &known).is_empty());
  assert!(common_boundary_violations("19", 5, &known).is_empty());
}

#[test]
fn a_path_under_no_allowed_directory_is_outside() {
  let paths = vec!["rooms/1/people/a.md".to_owned(), "knowledge/b.md".to_owned()];

  assert_eq!(outside_surface(&paths, &["rooms/1/people/".to_owned()]), vec!["knowledge/b.md"]);
}

#[tokio::test]
async fn a_rooms_books_are_the_rooms_folders_that_stand_in_every_agents_folder() {
  let dir = tempfile::tempdir().unwrap();
  let brain = dir.path();
  let agency = FakeAgency::default();
  let mut other = registration(OTHER_AGENT);

  other.brain_folder = format!("agents/{OTHER_AGENT}/");
  *agency.registrations.lock().unwrap() = vec![registration(AGENT), other];

  fs::create_dir_all(brain.join("rooms/-5/people")).unwrap();
  fs::create_dir_all(brain.join(format!("agents/{OTHER_AGENT}/rooms/-5"))).unwrap();
  fs::create_dir_all(brain.join("rooms/-6")).unwrap();

  assert_eq!(
    room_books(&agency, brain, -5).await.unwrap(),
    vec![brain.join("rooms/-5"), brain.join(format!("agents/{OTHER_AGENT}/rooms/-5"))]
  );
}
