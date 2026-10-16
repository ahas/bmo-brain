//! The service's own policy read whole: the common words, the words an item is named by, and the memory and
//! person-book numbers.

use bmo_brain::policy::{BRAIN, COMMON_WORDS, ITEM_WORDS};
use brain_support::policy::fill_marks;

#[test]
fn the_common_words_are_lower_case_and_named_once() {
  let mut seen = std::collections::BTreeSet::new();

  assert!(!COMMON_WORDS.words.is_empty());
  for word in &COMMON_WORDS.words {
    assert_eq!(word, &word.to_lowercase());
    assert!(!word.trim().is_empty() && seen.insert(word.as_str()), "{word} is blank or named twice");
  }
}

#[test]
fn the_names_join_the_common_words_lower_case_and_once() {
  let words = COMMON_WORDS.with_names(["Agent-A", " agent-a ", "", "Room"]);

  assert_eq!(words.len(), COMMON_WORDS.words.len() + 1);
  assert_eq!(words.last().map(String::as_str), Some("agent-a"));
}

#[test]
fn an_item_is_named_by_one_word_after_its_number_and_one_before_it() {
  assert!(!ITEM_WORDS.after_number.trim().is_empty() && !ITEM_WORDS.before_number.trim().is_empty());
  assert!(
    !ITEM_WORDS.not_after.is_empty() && ITEM_WORDS.not_after.iter().all(|word| !word.trim().is_empty())
  );
}

#[test]
fn a_still_state_is_released_after_whole_hours_with_a_code() {
  let hours = (BRAIN.memories.state_idle_seconds / 3600).to_string();

  assert_eq!(BRAIN.memories.state_idle_seconds % 3600, 0);
  assert_eq!(fill_marks(&BRAIN.memories.released, &[("hours", &hours)]), "stood-still hours=12");
  assert!(BRAIN.memories.wither_every_seconds < BRAIN.memories.wither_after_seconds);
}

#[test]
fn a_sheet_ceiling_stands_for_each_part() {
  assert_eq!((BRAIN.sheets.common_ceiling, BRAIN.sheets.room_ceiling), (8000, 6000));
}
