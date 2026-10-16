//! What is read off a fragment's body and title without asking anybody, with the text in the rooms'
//! language read from `fixtures/meta.toml`.

use fragment_desk::desk::Names;
use fragment_desk::meta::{
  body_facts, body_hash, item_bracket_problem, title_shape_problem, words_carried,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
  names: FixtureNames,
  hash: Hash,
  titles: Titles,
  carried: Carried,
}

#[derive(Deserialize)]
struct FixtureNames {
  agent: String,
  agent_key: String,
  common_words: Vec<String>,
}

#[derive(Deserialize)]
struct Hash {
  body: String,
  digest: String,
}

#[derive(Deserialize)]
struct Titles {
  short: String,
  naming_the_agent: String,
  standing: String,
}

#[derive(Deserialize)]
struct Carried {
  words: Vec<String>,
  one: String,
  both: String,
}

fn fixture() -> Fixture {
  toml::from_str(include_str!("fixtures/meta.toml")).unwrap()
}

fn names(fixture: &Fixture) -> Names {
  Names {
    agent: fixture.names.agent.clone(),
    agent_key: fixture.names.agent_key.clone(),
    common_words: fixture.names.common_words.clone(),
  }
}

#[test]
fn a_body_says_its_days_and_items() {
  let facts = body_facts(
    "- [date 2026-10-03 · confirmed 2026-10-05] #758 one\n- [basis 2026-09-01 · confirmed unknown] #12",
  );
  let hash = fixture().hash;

  assert_eq!(facts.written_on.as_deref(), Some("2026-09-01"));
  assert_eq!(facts.confirmed_on.as_deref(), Some("2026-10-05"));
  assert_eq!(facts.items, [758, 12]);
  assert_eq!(body_hash(&hash.body), hash.digest);
}

#[test]
fn an_unnamed_day_bracket_is_a_problem_and_a_named_one_is_not() {
  assert!(item_bracket_problem("- [2026-10-03 · confirmed 2026-10-04] x").is_some());
  assert!(item_bracket_problem("- [date 2026-10-03 · confirmed 2026-10-04] x").is_none());
  assert!(item_bracket_problem("- [a link](https://x) y").is_none());
}

#[test]
fn a_title_is_held_to_its_shape() {
  let fixture = fixture();
  let names = names(&fixture);

  assert!(title_shape_problem(&fixture.titles.short, &names).unwrap().contains("too short"));
  assert!(
    title_shape_problem(&fixture.titles.naming_the_agent, &names)
      .unwrap()
      .starts_with(&format!("carries {},", names.agent))
  );
  assert_eq!(title_shape_problem(&fixture.titles.standing, &names), None);
}

#[test]
fn a_title_in_english_says_when_its_piece_is_needed() {
  let names = names(&fixture());

  assert_eq!(
    title_shape_problem("A note on the order of a deploy, read when a deploy has to be run again.", &names),
    None
  );
  assert_eq!(
    title_shape_problem("A note on the order of a deploy and the steps it is made of, in full.", &names)
      .as_deref(),
    Some("does not say when the piece is needed")
  );
}

#[test]
fn words_are_carried_whole_unless_hangul() {
  let carried = fixture().carried;

  assert_eq!(words_carried(&carried.words, &carried.one), 1);
  assert_eq!(words_carried(&carried.words, &carried.both), 2);
}
