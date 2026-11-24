//! The duplicate candidates a sleep reads (docs/design.md, 「Duplicate candidates」): the pairs of live rows about
//! one subject that stand close in meaning or in words, each with its reasons. The brain judges none of them.

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::DuplicateCandidate;

use super::brains::brain_of;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::words::{index_words, overlap};
use super::{Fault, Held, Store};

/// How many candidates a reading answers where the ask names no limit, and at most.
const CANDIDATES: i64 = 50;
const CANDIDATES_MOST: i64 = 500;

/// The types never compared: the raw episodes and the persona's pieces.
const UNCOMPARED: [&str; 2] = ["episode", "persona"];

/// A pair found: the two rows' ids, the older first, with the closeness that made it a candidate.
struct Found {
  first: i64,
  second: i64,
  subjects: Vec<i64>,
  vector: Option<f64>,
  words: f64,
}

impl Found {
  /// How close the pair stands: the greater of its cosine and its word overlap.
  fn closeness(&self) -> f64 {
    self.vector.unwrap_or(0.0).max(self.words)
  }
}

impl Store {
  /// The duplicate candidates of a brain, closest first, at most the limit.
  pub async fn duplicates(&self, brain_id: i64, limit: Option<i64>) -> Held<Vec<DuplicateCandidate>> {
    let limit = match limit {
      None => CANDIDATES,
      Some(limit) if (1..=CANDIDATES_MOST).contains(&limit) => limit,
      Some(limit) => {
        return Err(Fault::BadRequest(format!("limit is {limit}; it is 1 to {CANDIDATES_MOST}.")));
      }
    };
    let rules = &self.rules;
    let per_subject = rules.duplicate_rows_per_subject.max(2);
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    index_words(&mut tx, brain_id).await?;

    // Each subject's live rows, the newest first and at most the bound, with their stems.
    let linked: Vec<(i64, i64, Option<Vec<String>>)> = sqlx::query_as(
      "SELECT subject_id, id, words FROM ( \
         SELECT ms.subject_id, m.id, m.words, \
           ROW_NUMBER() OVER (PARTITION BY ms.subject_id ORDER BY m.id DESC) AS place \
         FROM memory_subjects ms JOIN memories m ON m.id = ms.memory_id \
         WHERE m.brain_id = $1 AND m.status = 'active' AND NOT (m.type = ANY($2)) \
       ) ranked WHERE place <= $3 ORDER BY subject_id, id",
    )
    .bind(brain_id)
    .bind(UNCOMPARED.map(str::to_owned).to_vec())
    .bind(per_subject)
    .fetch_all(&mut *tx)
    .await?;
    let mut by_subject: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut stems_of: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    for (subject, id, words) in linked {
      by_subject.entry(subject).or_default().push(id);
      stems_of.entry(id).or_insert_with(|| words.unwrap_or_default().into_iter().collect());
    }
    let ids: Vec<i64> = stems_of.keys().copied().collect();
    let units = self.units_of(&ids);

    // Every pair within a subject, once, the subjects it shares gathered as it is met again.
    let mut pairs: BTreeMap<(i64, i64), Found> = BTreeMap::new();
    for (subject, rows) in &by_subject {
      for (place, first) in rows.iter().enumerate() {
        for second in &rows[place + 1..] {
          let key = ((*first).min(*second), (*first).max(*second));
          if let Some(found) = pairs.get_mut(&key) {
            found.subjects.push(*subject);
            continue;
          }
          let words = match (stems_of.get(&key.0), stems_of.get(&key.1)) {
            (Some(a), Some(b)) => overlap(a, b),
            _ => 0.0,
          };
          let vector = match (units.get(&key.0), units.get(&key.1)) {
            (Some(a), Some(b)) => Some(f64::from(a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>())),
            _ => None,
          };
          let close = words >= rules.duplicate_word_share
            || vector.is_some_and(|cosine| cosine >= rules.duplicate_similarity);
          if close {
            pairs.insert(
              key,
              Found {
                first: key.0,
                second: key.1,
                subjects: vec![*subject],
                vector,
                words,
              },
            );
          }
        }
      }
    }
    let mut found: Vec<Found> = pairs.into_values().collect();
    found.sort_by(|a, b| {
      b.closeness()
        .partial_cmp(&a.closeness())
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a.first.cmp(&b.first))
        .then(a.second.cmp(&b.second))
    });
    found.truncate(usize::try_from(limit).unwrap_or(usize::MAX));

    let wanted: Vec<i64> = found
      .iter()
      .flat_map(|pair| [pair.first, pair.second])
      .collect::<BTreeSet<_>>()
      .into_iter()
      .collect();
    let rows: Vec<MemoryRow> =
      sqlx::query_as(&format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id = ANY($1) ORDER BY id"))
        .bind(&wanted)
        .fetch_all(&mut *tx)
        .await?;
    let members: BTreeMap<i64, bmo_brain_client::Memory> = self
      .members(&mut tx, rows)
      .await?
      .into_iter()
      .map(|memory| (memory.id, memory))
      .collect();
    self.commit(tx).await?;
    Ok(
      found
        .into_iter()
        .filter_map(|pair| {
          Some(DuplicateCandidate {
            first: members.get(&pair.first)?.clone(),
            second: members.get(&pair.second)?.clone(),
            subject_ids: pair.subjects,
            vector_similarity: pair.vector,
            word_share: pair.words,
          })
        })
        .collect(),
    )
  }
}
