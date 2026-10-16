//! What can be read about a fragment without asking anybody: off its body, off its title, and off the
//! line its writer is shown.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::desk::{FragmentWrite, Names};
use crate::policy::TITLES;

/// A row as the writer's line shows it.
pub struct LineRow<'a> {
  pub slug: &'a str,
  pub scope: Option<&'a str>,
  pub title: &'a str,
  pub written_on: Option<&'a str>,
  pub confirmed_on: Option<&'a str>,
  pub path: &'a str,
  pub keywords: &'a [String],
  pub items: &'a [i64],
  pub status: &'a str,
}

static ITEM_BRACKET: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\[(?:date|basis)\s+([^\]·]*)·\s*confirmed\s+([^\]]*)\]").unwrap());
static DAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2}").unwrap());
static WRITTEN_ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#([0-9]{1,5})(?-u:\b)").unwrap());
static TRAILING_PERIOD: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\s+·\s+(?:[0-9]{4}-[0-9]{2}(?:~[0-9]{4}-[0-9]{2})?|undated)\s*$").unwrap());
static OPENING_BRACKET: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?m)^\s*[-*]\s+\[([^\]]*)\]").unwrap());
static DAY_BRACKET: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"·\s*confirmed(?-u:\b)|^\s*(?:[0-9]{4}-[0-9]{2}-[0-9]{2}|unknown)(?-u:\b)").unwrap()
});
static NAMED_BRACKET: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^(?:date|basis)\s+[^·]*·\s*confirmed\s+\S").unwrap());
// The policy's situation words whole and in any case, or its stems wherever they stand.
static SITUATION_CUE: LazyLock<Regex> = LazyLock::new(|| {
  let alternatives =
    |words: &[String]| words.iter().map(|word| regex::escape(word)).collect::<Vec<_>>().join("|");

  Regex::new(&format!(
    r"(?i)(?-u:\b)({})(?-u:\b)|{}",
    alternatives(&TITLES.situation_words),
    alternatives(&TITLES.situation_stems),
  ))
  .unwrap()
});
// A title's words: a Latin word, or a run of Hangul syllables (U+AC00 to U+D7A3).
static TITLE_WORDS: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[a-z][a-z0-9-]*|[\x{AC00}-\x{D7A3}]+").unwrap());
static HANGUL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\x{AC00}-\x{D7A3}]").unwrap());

fn unique<T: Clone + Eq + std::hash::Hash>(values: impl IntoIterator<Item = T>) -> Vec<T> {
  let mut seen = HashSet::new();

  values.into_iter().filter(|value| seen.insert(value.clone())).collect()
}

/// A body's digest: the first sixteen hex digits of its SHA-256.
pub fn body_hash(text: &str) -> String {
  Sha256::digest(text.as_bytes())
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect::<String>()[..16]
    .to_owned()
}

/// The items a text names by their mark, in the order it first names them.
pub fn items_named(text: &str) -> Vec<i64> {
  unique(WRITTEN_ITEM.captures_iter(text).filter_map(|found| found[1].parse().ok()))
}

/// What a body says about itself: the earliest and latest day its item brackets carry, and its items.
pub struct BodyFacts {
  pub written_on: Option<String>,
  pub confirmed_on: Option<String>,
  pub items: Vec<i64>,
}

pub fn body_facts(text: &str) -> BodyFacts {
  let mut days: Vec<String> = Vec::new();

  for bracket in ITEM_BRACKET.captures_iter(text) {
    days.extend(DAY.find_iter(&bracket[1]).map(|day| day.as_str().to_owned()));
    days.extend(DAY.find_iter(&bracket[2]).map(|day| day.as_str().to_owned()));
  }

  days.sort();

  BodyFacts {
    written_on: days.first().cloned(),
    confirmed_on: days.last().cloned(),
    items: items_named(text),
  }
}

/// The months a piece's days span, as an index line shows them.
pub fn period_of(written_on: Option<&str>, confirmed_on: Option<&str>) -> Option<String> {
  let from = written_on.or(confirmed_on)?.get(..7)?;
  let to = confirmed_on.or(written_on)?.get(..7)?;

  Some(if from == to { from.to_owned() } else { format!("{from}~{to}") })
}

/// One row as a writer is shown it: the index line, then where its body stands and what the table
/// knows of it beyond the line.
pub fn writer_line(row: &LineRow<'_>) -> String {
  let period = period_of(row.written_on, row.confirmed_on);
  let index = format!(
    "- {}{} — {}{}",
    row.slug,
    row.scope.map_or_else(String::new, |scope| format!(" {scope}")),
    row.title,
    period.map_or_else(String::new, |period| format!(" · {period}")),
  );
  let mut facts = vec![format!("path {}", row.path)];

  if !row.keywords.is_empty() {
    facts.push(format!("keywords {}", row.keywords.join(", ")));
  }

  if !row.items.is_empty() {
    facts.push(format!(
      "items {}",
      row.items.iter().map(|item| format!("#{item}")).collect::<Vec<_>>().join(", ")
    ));
  }

  if row.status == "done" {
    facts.push("done".to_owned());
  }

  format!("{index} ({})", facts.join(" · "))
}

/// Why a piece's items cannot stand, or none where they can: a day bracket whose slots are not named.
pub fn item_bracket_problem(text: &str) -> Option<String> {
  let unnamed: Vec<String> = OPENING_BRACKET
    .captures_iter(text)
    .map(|found| found[1].trim().to_owned())
    .filter(|inside| DAY_BRACKET.is_match(inside) && !NAMED_BRACKET.is_match(inside))
    .collect();
  let first = unnamed.first()?;

  Some(format!(
    "has {} item{} whose day bracket does not name its slots, such as [{first}]: every item opens with [date D · confirmed D], and a judgement with [basis D · confirmed D]",
    unnamed.len(),
    if unnamed.len() == 1 { "" } else { "s" },
  ))
}

/// A title as a row keeps it: trimmed, and without a period standing at its end.
pub fn plain_title(title: &str) -> String {
  TRAILING_PERIOD.replace(title, "").trim().to_owned()
}

/// Why a title cannot stand as a row's, or none where it can.
pub fn title_shape_problem(title: &str, names: &Names) -> Option<String> {
  let plain = plain_title(title);
  let length = plain.encode_utf16().count();

  if length < 40 {
    return Some("is too short to say what the piece is and when it is needed".into());
  }

  if length > 320 {
    return Some("is longer than 320 characters".into());
  }

  let parts: Vec<&str> =
    plain.split([',', '·', ';']).map(str::trim).filter(|part| !part.is_empty()).collect();
  let bare = parts.iter().filter(|part| part.split_whitespace().count() <= 3).count();

  if parts.len() >= 4 && bare as f64 / parts.len() as f64 > 0.6 {
    return Some("is a list of nouns rather than what the piece is and when it is needed".into());
  }

  let lower = plain.to_lowercase();
  let words: Vec<&str> = TITLE_WORDS.find_iter(&lower).map(|word| word.as_str()).collect();
  let common = words
    .iter()
    .filter(|word| {
      names
        .common_words
        .iter()
        .any(|one| **word == one.as_str() || (HANGUL.is_match(one) && word.starts_with(one.as_str())))
    })
    .count();

  if !words.is_empty() && common as f64 / words.len() as f64 > 0.4 {
    return Some("leans on the words every turn carries rather than on what the piece is about".into());
  }

  let agent = names.agent.to_lowercase();

  if words.iter().any(|word| word.starts_with(&agent)) {
    return Some(format!(
      "carries {}, the name every turn that calls the agent carries; call it the assistant's name and leave the spelling to the body",
      names.agent,
    ));
  }

  if !SITUATION_CUE.is_match(&plain) {
    return Some("does not say when the piece is needed".into());
  }

  None
}

/// How many of these words a text carries: a Hangul word where it stands, any other word whole.
pub fn words_carried(words: &[String], text: &str) -> usize {
  let lower = text.to_lowercase();
  let asked = unique(
    words
      .iter()
      .map(|one| one.trim().to_lowercase())
      .filter(|one| one.encode_utf16().count() >= 2),
  );

  asked
    .iter()
    .filter(|word| {
      if HANGUL.is_match(word) {
        lower.contains(word.as_str())
      } else {
        Regex::new(&format!("(?:^|[^a-z0-9]){}(?:[^a-z0-9]|$)", regex::escape(word)))
          .is_ok_and(|whole| whole.is_match(&lower))
      }
    })
    .count()
}

/// A path as a write names it: with no leading `./`.
pub fn write_path(path: &str) -> String {
  let trimmed = path.trim();

  trimmed.strip_prefix("./").unwrap_or(trimmed).to_owned()
}

/// What a writer said of a piece, as a row write keeps it.
pub fn fragment_write(
  path: &str,
  title: &str,
  keywords: &[String],
  items: &[i64],
  shared: Option<bool>,
  all_agents: Option<bool>,
) -> FragmentWrite {
  FragmentWrite {
    path: write_path(path),
    removed: false,
    title: plain_title(title),
    keywords: unique(keywords.iter().map(|word| word.trim().to_owned()).filter(|word| !word.is_empty())),
    items: unique(items.iter().copied().filter(|item| *item > 0)),
    shared,
    all_agents,
  }
}

/// Why a write's keywords cannot stand, or none where they can: a word nearly every turn carries.
pub fn keyword_problem(write: &FragmentWrite, names: &Names) -> Option<String> {
  if write.path.strip_suffix(".md").unwrap_or(&write.path).ends_with("-address") {
    return None;
  }

  let common: Vec<&str> = write
    .keywords
    .iter()
    .filter(|keyword| names.common_words.contains(&keyword.to_lowercase()))
    .map(String::as_str)
    .collect();

  (!common.is_empty()).then(|| {
    format!("carries {} as keywords, words nearly every turn carries; name the words particular to this piece instead", common.join(", "))
  })
}

/// The write that takes a piece away.
pub fn removal_write(path: &str) -> FragmentWrite {
  FragmentWrite {
    path: write_path(path),
    removed: true,
    title: String::new(),
    keywords: Vec::new(),
    items: Vec::new(),
    shared: None,
    all_agents: None,
  }
}
