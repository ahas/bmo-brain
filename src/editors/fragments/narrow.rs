//! The editor's own narrowing (docs/fragments.md, 「Finding a piece before writing one」): which of the rows the
//! table picked for a batch its writer is handed whole, by the batch's own text — an item it names or a keyword
//! it carries — beside the rows the batch always hands over; and a row written out as its index line. It is the
//! brain's copy of the rule the gate narrows a turn's candidates by.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use brain_support::js;
use fragment_desk::meta::{period_of, words_carried};
use regex::Regex;

use super::store::Row;
use crate::policy::ITEM_WORDS;

// The ending a person's address piece carries, whose keywords are the names they are called by.
const ADDRESS_TOPIC: &str = "-address";

static WRITTEN_ITEM: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"#([0-9]{1,5})(?-u:\b)").expect("an item's mark"));
static NAMED_ITEM: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(&format!("{}[{}]*#?([0-9]{{1,5}})", regex::escape(&ITEM_WORDS.before_number), js::SPACE))
    .expect("an item named as one")
});

fn unique<T: Clone + Eq + std::hash::Hash>(values: impl IntoIterator<Item = T>) -> Vec<T> {
  let mut seen = HashSet::new();

  values.into_iter().filter(|value| seen.insert(value.clone())).collect()
}

// The items a text names by a number and the word that follows it: a run of two to five digits standing on
// its own, then that word, not followed by what makes the number something else.
fn items_by_word(text: &str) -> Vec<i64> {
  let word = js::chars(&ITEM_WORDS.after_number);
  let not_after: Vec<Vec<char>> = ITEM_WORDS
    .not_after
    .iter()
    .map(|other| js::chars(other))
    .filter(|other| !other.is_empty())
    .collect();
  let characters = js::chars(text);
  let mut found = vec![];

  if word.is_empty() {
    return found;
  }

  for start in 0..characters.len() {
    if start > 0 && characters[start - 1].is_ascii_digit() {
      continue;
    }

    let digits = characters[start..].iter().take_while(|character| character.is_ascii_digit()).count();

    if !(2..=5).contains(&digits) {
      continue;
    }

    let mut at = start + digits;

    while at < characters.len() && js::is_space(characters[at]) {
      at += 1;
    }

    let rest = &characters[at..];

    if rest.starts_with(&word) && !not_after.iter().any(|other| rest[word.len()..].starts_with(other)) {
      found.push(js::text(&characters[start..start + digits]).parse().unwrap_or_default());
    }
  }

  found
}

/// The items a text names: by their mark, by a number and the word for one, or named as one.
pub fn items_asked(text: &str) -> Vec<i64> {
  let marked = WRITTEN_ITEM.captures_iter(text).filter_map(|found| found[1].parse().ok());
  let named = NAMED_ITEM.captures_iter(text).filter_map(|found| found[1].parse().ok());

  unique(unique(marked).into_iter().chain(items_by_word(text)).chain(named))
}

// Whether a number stands in a text with no digit either side of it.
fn names_number(text: &str, number: i64) -> bool {
  let wanted = number.to_string();

  text.match_indices(&wanted).any(|(at, _)| {
    let before = text[..at].chars().next_back();
    let after = text[at + wanted.len()..].chars().next();

    !before.is_some_and(|one| one.is_ascii_digit()) && !after.is_some_and(|one| one.is_ascii_digit())
  })
}

/// A batch as the narrowing reads it: its text, who spoke in it, and the names each person is known by.
pub struct Turn {
  pub text: String,
  pub speakers: Vec<i64>,
  pub names: HashMap<i64, Vec<String>>,
}

// The people a batch names: by their number, by a name they are known by, or by a name their address piece
// carries.
fn named_people(rows: &[Row], turn: &Turn) -> HashSet<i64> {
  let owners: HashSet<i64> = rows.iter().filter_map(|row| row.owner).collect();

  owners
    .into_iter()
    .filter(|owner| {
      let addressed = rows
        .iter()
        .filter(|row| row.owner == Some(*owner) && row.slug.ends_with(ADDRESS_TOPIC))
        .flat_map(|row| row.keywords.iter().cloned());
      let names: Vec<String> = turn
        .names
        .get(owner)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .chain(addressed)
        .collect();

      names_number(&turn.text, *owner) || words_carried(&names, &turn.text) > 0
    })
    .collect()
}

/// How many pieces a writer is handed whole on the strength of the batch's own words, beside the ones it is
/// always handed.
pub const WRITER_MATCH_CAP: usize = 24;

/// The rows a writer is handed whole before it writes: every row the caller always hands over, and the rows the
/// batch's text names by an item or a keyword — never a row kept only for its person. Where more are named than
/// the cap, the item matches go first, then the rows carrying more of the text's words, then the open ones whose
/// person is in the batch, then the latest confirmed, then the order they came in. They come back in the order
/// the rows were given.
pub fn rows_to_hand(rows: &[Row], batch: &Turn, always: impl Fn(&Row) -> bool) -> Vec<Row> {
  let asked: HashSet<i64> = items_asked(&batch.text).into_iter().collect();
  // The rows named by an item or a word, by where they stand among the rows given.
  let named: Vec<usize> = (0..rows.len())
    .filter(|&at| !always(&rows[at]))
    .filter(|&at| {
      let row = &rows[at];

      row.items.iter().any(|item| asked.contains(item)) || words_carried(&row.keywords, &batch.text) > 0
    })
    .collect();
  let owned: Vec<Row> = named.iter().map(|&at| rows[at].clone()).collect();
  let present: HashSet<i64> = batch.speakers.iter().copied().chain(named_people(&owned, batch)).collect();
  let mut scored: Vec<(usize, usize, bool, &str, usize)> = named
    .iter()
    .map(|&at| {
      let row = &rows[at];
      let items = row.items.iter().filter(|item| asked.contains(item)).count();
      let words = words_carried(&row.keywords, &batch.text);
      let person = row.status == "open" && row.owner.is_none_or(|owner| present.contains(&owner));

      (items, words, person, row.confirmed_on.as_deref().unwrap_or_default(), at)
    })
    .collect();

  if scored.len() > WRITER_MATCH_CAP {
    scored.sort_by(|left, right| {
      right
        .0
        .cmp(&left.0)
        .then(right.1.cmp(&left.1))
        .then(right.2.cmp(&left.2))
        .then(right.3.cmp(left.3))
        .then(left.4.cmp(&right.4))
    });
    scored.truncate(WRITER_MATCH_CAP);
  }

  let kept: HashSet<usize> = scored.iter().map(|(.., at)| *at).collect();

  rows
    .iter()
    .enumerate()
    .filter(|(at, row)| always(row) || kept.contains(at))
    .map(|(_, row)| row.clone())
    .collect()
}

/// One row as its index line: the slug, the bracket its library carries where it has one, its title, and the
/// months its days span where it carries any.
pub fn index_line(row: &Row) -> String {
  let period = period_of(row.written_on.as_deref(), row.confirmed_on.as_deref());

  format!(
    "- {}{} — {}{}",
    row.slug,
    row.scope.as_deref().map_or_else(String::new, |scope| format!(" {scope}")),
    row.title,
    period.map_or_else(String::new, |period| format!(" · {period}")),
  )
}
