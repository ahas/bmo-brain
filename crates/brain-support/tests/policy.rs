//! A policy line's marks filled every one at once, and a policy file read into its type.

use brain_support::policy::{fill_marks, read};
use serde::Deserialize;

#[test]
fn every_mark_is_filled_at_once_and_an_unnamed_one_stays() {
  assert_eq!(fill_marks("<{tag}>", &[("tag", "b")]), "<b>");
  assert_eq!(fill_marks("{a} {b} {c}", &[("a", "{b}"), ("b", "2")]), "{b} 2 {c}");
  assert_eq!(fill_marks("{ a} {} {a", &[("a", "1")]), "{ a} {} {a");
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Part {
  count: u32,
}

#[test]
fn a_part_reads_into_its_type() {
  assert_eq!(read::<Part>("part.toml", "count = 3"), Part { count: 3 });
}

#[test]
#[should_panic(expected = "the policy's part.toml does not read")]
fn a_part_that_does_not_read_names_its_file() {
  read::<Part>("part.toml", "count = 3\nother = 1");
}
