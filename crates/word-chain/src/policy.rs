//! What a game is played by (docs/word-chain.md), read from this crate's `policy/word-chain.toml`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordChainPlay {
  pub words: RoomWords,
  pub times: Times,
  pub rounds: Rounds,
  pub points: Points,
  pub names: Names,
  pub particles: Particles,
  pub install: Install,
}

/// The subject particle a name takes by the sound it ends on.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Particles {
  pub after_vowel: String,
  pub after_consonant: String,
  /// Both, for a name whose spelling does not say how it ends.
  pub unread: String,
}

/// What the word book is installed from.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
  pub kkutu: Source,
  pub nikl: Dictionaries,
}

/// One source the book is read out of: its repository, the commit it is read at and its licence.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
  pub repository: String,
  pub commit: String,
  pub licence: String,
}

/// The dictionaries a word's meaning is read out of, and how they are read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dictionaries {
  pub repository: String,
  pub commit: String,
  pub licence: String,
  pub stdict_parts: usize,
  pub stdict_last: String,
  pub krdict_parts: usize,
  pub noun: String,
  pub off_standard: Vec<String>,
  pub proper_fields: Vec<String>,
  pub no_field: String,
  pub meaning_chars: usize,
}

/// The words a room says to a standing game.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomWords {
  pub join: Vec<String>,
  pub begin: Vec<String>,
  pub give_up: Vec<String>,
  pub stop: Vec<String>,
  /// What may come before an offered opening word, besides the begin words.
  pub offer: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Times {
  pub recruit_seconds: u64,
  pub idle_seconds: u64,
  pub robot_ms: u64,
  pub least_turn_seconds: u64,
  pub most_turn_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rounds {
  /// How many words a round's opening word must leave to answer with.
  pub answers: u64,
  /// How many candidates the robot weighs before it moves.
  pub robot_reach: usize,
  /// The syllables a round's word is drawn from.
  pub syllables: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Points {
  pub per_syllable: i64,
  pub hanbang: i64,
  pub survivor: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Names {
  /// How many players a game's name lists before it counts the rest.
  pub listed: usize,
  /// The word a name given for a game may close on, set aside when the name is matched.
  pub closing: String,
}

brain_support::part!(pub WORD_CHAIN: WordChainPlay = "word-chain.toml");
