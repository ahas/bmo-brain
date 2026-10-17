//! The service's own policy, under the root `policy/` folder: the common words and the words an item is named
//! by (docs/fragments.md), the memory and person-book numbers (docs/memories.md, docs/people-book.md), and the
//! Korean words the prompts quote (docs/boundary-api.md, 「Its prompts」).

use std::collections::BTreeMap;

use serde::Deserialize;

/// The words every turn of the rooms carries, which a fragment's title or keyword hanging on them cannot
/// carry as its substance.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommonWords {
  /// Each word as a keyword is compared with it, lower case.
  pub words: Vec<String>,
}

impl CommonWords {
  /// The policy's words joined by the names the rooms call the chat agent and the owner by, as its
  /// registration and the configuration give them, each lower case and named once.
  pub fn with_names<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut words = self.words.clone();

    for name in names {
      let name = name.trim().to_lowercase();

      if !name.is_empty() && !words.contains(&name) {
        words.push(name);
      }
    }

    words
  }
}

brain_support::part!(pub COMMON_WORDS: CommonWords = "common-words.toml");

/// The words a text names a TODO item by without its mark, which the editor's narrowing reads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemWords {
  /// The word that follows an item's number.
  pub after_number: String,
  /// What, standing right after that word, makes the number something else.
  pub not_after: Vec<String>,
  /// The word an item is named as one by, ahead of its number.
  pub before_number: String,
}

brain_support::part!(pub ITEM_WORDS: ItemWords = "items.toml");

/// The brain's own numbers.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brain {
  pub memories: Memories,
  pub sheets: Sheets,
}

/// A memory row's withering and a still state's release.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memories {
  pub wither_after_seconds: i64,
  pub wither_every_seconds: i64,
  pub state_idle_seconds: i64,
  /// The code a state released for standing still is checked off with; `{hours}` is how long it stood.
  pub released: String,
}

/// How long a standing sheet attached on every turn of its speaker may be, in characters.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheets {
  pub common_ceiling: usize,
  pub room_ceiling: usize,
}

brain_support::part!(pub BRAIN: Brain = "brain.toml");

/// The Korean words the recollectors' prompts quote, each by the name of the `{{name}}` mark that stands for it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptWords {
  pub words: BTreeMap<String, String>,
}

brain_support::part!(pub PROMPT_WORDS: PromptWords = "prompts.toml");
