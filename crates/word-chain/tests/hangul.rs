//! The Hangul arithmetic, with the syllables and words it is checked on read from
//! `fixtures/word-chain/hangul.toml`.

use serde::Deserialize;
use word_chain::hangul::{is_hangul_word, last_final, openings_for};

#[derive(Deserialize)]
struct Fixture {
  openings: Vec<Openings>,
  words: Vec<String>,
  not_words: Vec<String>,
  finals: Vec<Final>,
}

#[derive(Deserialize)]
struct Openings {
  syllable: String,
  openings: Vec<String>,
}

#[derive(Deserialize)]
struct Final {
  word: String,
  last: u32,
}

fn fixture() -> Fixture {
  toml::from_str(include_str!("fixtures/word-chain/hangul.toml")).unwrap()
}

#[test]
fn the_initial_sound_rule_turns_rieul_and_nieun_one_way() {
  for case in fixture().openings {
    let syllable = case.syllable.chars().next().expect("a syllable");

    assert_eq!(openings_for(syllable), case.openings, "{}", case.syllable);
  }
}

#[test]
fn a_word_is_whole_syllables_and_nothing_else() {
  let fixture = fixture();

  for word in &fixture.words {
    assert!(is_hangul_word(word), "{word}");
  }

  for text in &fixture.not_words {
    assert!(!is_hangul_word(text), "{text:?}");
  }

  for case in &fixture.finals {
    assert_eq!(last_final(&case.word), case.last, "{}", case.word);
  }
}
