//! The word-chain policy read whole, with the vocabulary it is expected to hold read from
//! `fixtures/policy.toml`.

use serde::Deserialize;
use word_chain::policy::WORD_CHAIN;

#[derive(Deserialize)]
struct Expected {
  join: String,
  closing: String,
}

fn expected() -> Expected {
  toml::from_str(include_str!("fixtures/policy.toml")).unwrap()
}

#[test]
fn a_game_recruits_for_a_minute_and_holds_a_turn_between_ten_seconds_and_two_minutes() {
  assert_eq!(WORD_CHAIN.times.recruit_seconds, 60);
  assert_eq!((WORD_CHAIN.times.least_turn_seconds, WORD_CHAIN.times.most_turn_seconds), (10, 120));
  assert!(WORD_CHAIN.words.join.contains(&expected().join));
  assert!(!WORD_CHAIN.rounds.syllables.is_empty());
}

#[test]
fn a_name_given_for_a_game_closes_on_the_policys_word() {
  assert_eq!(WORD_CHAIN.names.closing, expected().closing);
  assert_eq!(WORD_CHAIN.names.listed, 3);
}
