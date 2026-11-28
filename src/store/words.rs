//! The words recall reads by (docs/design.md, 「Recall」): a text's words lowercased, English stop words left
//! out, each reduced to its Snowball stem, and a run of Hangul taken as its syllable pairs; the stems of a brain's
//! rows kept on the rows; and the Jaccard overlap of two rows' stems.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use rust_stemmers::{Algorithm, Stemmer};
use sqlx::PgConnection;

use super::Held;

/// The English stop words of the Snowball project's list, contractions among them.
const STOP_WORDS: [&str; 174] = [
  "i",
  "me",
  "my",
  "myself",
  "we",
  "our",
  "ours",
  "ourselves",
  "you",
  "your",
  "yours",
  "yourself",
  "yourselves",
  "he",
  "him",
  "his",
  "himself",
  "she",
  "her",
  "hers",
  "herself",
  "it",
  "its",
  "itself",
  "they",
  "them",
  "their",
  "theirs",
  "themselves",
  "what",
  "which",
  "who",
  "whom",
  "this",
  "that",
  "these",
  "those",
  "am",
  "is",
  "are",
  "was",
  "were",
  "be",
  "been",
  "being",
  "have",
  "has",
  "had",
  "having",
  "do",
  "does",
  "did",
  "doing",
  "would",
  "should",
  "could",
  "ought",
  "i'm",
  "you're",
  "he's",
  "she's",
  "it's",
  "we're",
  "they're",
  "i've",
  "you've",
  "we've",
  "they've",
  "i'd",
  "you'd",
  "he'd",
  "she'd",
  "we'd",
  "they'd",
  "i'll",
  "you'll",
  "he'll",
  "she'll",
  "we'll",
  "they'll",
  "isn't",
  "aren't",
  "wasn't",
  "weren't",
  "hasn't",
  "haven't",
  "hadn't",
  "doesn't",
  "don't",
  "didn't",
  "won't",
  "wouldn't",
  "shan't",
  "shouldn't",
  "can't",
  "cannot",
  "couldn't",
  "mustn't",
  "let's",
  "that's",
  "who's",
  "what's",
  "here's",
  "there's",
  "when's",
  "where's",
  "why's",
  "how's",
  "a",
  "an",
  "the",
  "and",
  "but",
  "if",
  "or",
  "because",
  "as",
  "until",
  "while",
  "of",
  "at",
  "by",
  "for",
  "with",
  "about",
  "against",
  "between",
  "into",
  "through",
  "during",
  "before",
  "after",
  "above",
  "below",
  "to",
  "from",
  "up",
  "down",
  "in",
  "out",
  "on",
  "off",
  "over",
  "under",
  "again",
  "further",
  "then",
  "once",
  "here",
  "there",
  "when",
  "where",
  "why",
  "how",
  "all",
  "any",
  "both",
  "each",
  "few",
  "more",
  "most",
  "other",
  "some",
  "such",
  "no",
  "nor",
  "not",
  "only",
  "own",
  "same",
  "so",
  "than",
  "too",
  "very",
];

static STOPPED: LazyLock<BTreeSet<&'static str>> = LazyLock::new(|| STOP_WORDS.into_iter().collect());

static STEMMER: LazyLock<Stemmer> = LazyLock::new(|| Stemmer::create(Algorithm::English));

/// Whether a character is a Hangul syllable.
fn is_hangul(c: char) -> bool {
  ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

/// The syllable pairs of each run of Hangul in a word, each pair of neighbouring syllables once: a Korean word
/// carries its particles and endings on its stem (`봉보스퀜은`, `봉보스퀜을`), and the pairs its stem holds are
/// the ones every form of it shares, which no English stemmer reaches. A run of one syllable gives none.
pub fn hangul_pairs(word: &str) -> Vec<String> {
  let mut pairs = Vec::new();
  let mut previous: Option<char> = None;
  for c in word.chars() {
    if is_hangul(c) {
      if let Some(before) = previous {
        pairs.push([before, c].iter().collect());
      }
      previous = Some(c);
    } else {
      previous = None;
    }
  }
  pairs
}

/// The stems of a text: its runs of letters, digits and inner apostrophes, lowercased, the stop words and the
/// one-character words left out, each reduced to its Snowball stem, each once; a run of Hangul gives its syllable
/// pairs in place of a stem.
pub fn stems(text: &str) -> BTreeSet<String> {
  let lowered = text.to_lowercase().replace('\u{2019}', "'");
  let mut said = BTreeSet::new();
  for word in lowered.split(|c: char| !(c.is_alphanumeric() || c == '\'')) {
    said.extend(hangul_pairs(word));
    // What stands outside the Hangul: the word's other runs, each read as an English word is.
    for rest in word.split(is_hangul) {
      let rest = rest.trim_matches('\'');
      if rest.chars().count() < 2 || STOPPED.contains(rest) {
        continue;
      }
      let stem = STEMMER.stem(rest).replace('\'', "");
      if !stem.is_empty() {
        said.insert(stem);
      }
    }
  }
  said
}

/// The Jaccard overlap of two stem sets: what they share over what either holds; nothing where both are empty.
pub fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
  let union = a.union(b).count();
  if union == 0 {
    return 0.0;
  }
  a.intersection(b).count() as f64 / union as f64
}

/// The stems of every row of a brain not indexed yet, kept on the rows: the live ones a recall reaches, and the
/// closed ones a reading as of a past moment reaches.
pub(crate) async fn index_words(conn: &mut PgConnection, brain_id: i64) -> Held<()> {
  let rows: Vec<(i64, String)> =
    sqlx::query_as("SELECT id, text FROM memories WHERE brain_id = $1 AND words IS NULL")
      .bind(brain_id)
      .fetch_all(&mut *conn)
      .await?;
  if rows.is_empty() {
    return Ok(());
  }
  let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
  let joined: Vec<String> = rows
    .iter()
    .map(|(_, text)| stems(text).into_iter().collect::<Vec<_>>().join(" "))
    .collect();
  sqlx::query(
    "UPDATE memories m SET words = string_to_array(stemmed.joined, ' ') \
     FROM UNNEST($1::bigint[], $2::text[]) AS stemmed (id, joined) WHERE m.id = stemmed.id",
  )
  .bind(&ids)
  .bind(&joined)
  .execute(&mut *conn)
  .await?;
  Ok(())
}
