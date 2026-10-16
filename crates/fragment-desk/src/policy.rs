//! The desk's policy, under this crate's `policy/` folder: the words a title says when its piece is
//! needed by.

use serde::Deserialize;

/// The words that say when a piece is needed, which a title must carry one of.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Titles {
  /// Matched as whole words, ignoring case.
  pub situation_words: Vec<String>,
  /// Matched wherever they stand.
  pub situation_stems: Vec<String>,
}

brain_support::part!(pub TITLES: Titles = "titles.toml");
