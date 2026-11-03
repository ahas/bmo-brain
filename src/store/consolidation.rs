//! The order a consolidation reads its episodes in (docs/design.md, 「Consolidation」): each unconsolidated
//! episode's priority, gain × (1 + need), reckoned at the reading and never stored, and the episodes of a tag's
//! room written around it read out directly after it.

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{Memory, UnconsolidatedEpisode};

use super::brains::brain_of;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::words::{index_words, stems};
use super::{Fault, Held, Store};

/// How many episodes a consolidation reads where the ask names no limit, and at most.
const EPISODES: i64 = 60;
const EPISODES_MOST: i64 = 500;

/// The subject type that names a room.
const ROOM: &str = "room";

/// An episode as the order weighs it.
struct Weighed {
  memory: Memory,
  priority: f64,
  gain: f64,
  need: f64,
}

/// The order of two weighed episodes: priority, highest first; then importance, highest first; then the older.
fn before(a: &Weighed, b: &Weighed) -> std::cmp::Ordering {
  b.priority
    .partial_cmp(&a.priority)
    .unwrap_or(std::cmp::Ordering::Equal)
    .then(b.memory.importance.cmp(&a.memory.importance))
    .then(a.memory.id.cmp(&b.memory.id))
}

/// The share of an episode's stems another row holds; nothing where the episode has no stem.
fn contained(mine: &BTreeSet<String>, theirs: &BTreeSet<String>) -> f64 {
  if mine.is_empty() {
    return 0.0;
  }
  mine.intersection(theirs).count() as f64 / mine.len() as f64
}

/// The room subjects an episode is linked to.
fn rooms(memory: &Memory) -> BTreeSet<i64> {
  memory
    .subjects
    .iter()
    .filter(|subject| subject.kind == ROOM)
    .map(|subject| subject.id)
    .collect()
}

impl Store {
  /// The episodes not yet consolidated, in priority order with a tag's neighbours after it, at most the limit.
  pub async fn unconsolidated(
    &self,
    brain_id: i64,
    limit: Option<i64>,
  ) -> Held<Vec<UnconsolidatedEpisode>> {
    let limit = match limit {
      None => EPISODES,
      Some(limit) if (1..=EPISODES_MOST).contains(&limit) => limit,
      Some(limit) => {
        return Err(Fault::BadRequest(format!("limit is {limit}; it is 1 to {EPISODES_MOST}.")));
      }
    };
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    index_words(&mut tx, brain_id).await?;
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND type = 'episode' AND status = 'active' \
       AND consolidated_at IS NULL ORDER BY id"
    ))
    .bind(brain_id)
    .fetch_all(&mut *tx)
    .await?;
    let episodes = self.members(&mut tx, rows).await?;
    let episode_stems: Vec<BTreeSet<String>> =
      episodes.iter().map(|episode| stems(&episode.text)).collect();
    // Each subject's episodes' stems together: the rows of that subject worth comparing hold one of them.
    let mut asked: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    for (episode, mine) in episodes.iter().zip(&episode_stems) {
      for subject in &episode.subjects {
        asked.entry(subject.id).or_default().extend(mine.iter().cloned());
      }
    }
    let subjects: Vec<i64> = asked.keys().copied().collect();

    // The live rows that are no episode about each subject, holding one of its episodes' stems, read once a
    // subject through the words index, for the novelty; a row holding none contains none of them.
    let mut about: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut stems_of: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    for (subject, wanted) in &asked {
      if wanted.is_empty() {
        continue;
      }
      let wanted: Vec<String> = wanted.iter().cloned().collect();
      let linked: Vec<(i64, Option<Vec<String>>)> = sqlx::query_as(
        "SELECT m.id, m.words FROM memory_subjects ms JOIN memories m ON m.id = ms.memory_id \
         WHERE ms.subject_id = $1 AND m.brain_id = $2 AND m.status IN ('active', 'pending') \
         AND m.type <> 'episode' AND m.words && $3",
      )
      .bind(subject)
      .bind(brain_id)
      .bind(&wanted)
      .fetch_all(&mut *tx)
      .await?;
      for (memory, words) in linked {
        about.entry(*subject).or_default().push(memory);
        stems_of
          .entry(memory)
          .or_insert_with(|| words.unwrap_or_default().into_iter().collect());
      }
    }
    // The use of the live rows about each subject, added up, for the need.
    let used: BTreeMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
      "SELECT ms.subject_id, COALESCE(SUM(m.hits), 0)::bigint FROM memory_subjects ms \
       JOIN memories m ON m.id = ms.memory_id WHERE ms.subject_id = ANY($1) AND m.brain_id = $2 \
       AND m.status IN ('active', 'pending') GROUP BY ms.subject_id",
    )
    .bind(&subjects)
    .bind(brain_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    self.commit(tx).await?;

    let mut weighed: Vec<Weighed> = episodes
      .into_iter()
      .zip(episode_stems)
      .map(|(memory, mine)| {
        let held = memory
          .subjects
          .iter()
          .flat_map(|subject| about.get(&subject.id).into_iter().flatten())
          .filter_map(|row| stems_of.get(row))
          .map(|theirs| contained(&mine, theirs))
          .fold(0.0, f64::max);
        let gain = memory.importance as f64 * (1.0 + (1.0 - held));
        let need: f64 = memory
          .subjects
          .iter()
          .map(|subject| (1.0 + used.get(&subject.id).copied().unwrap_or(0) as f64).ln())
          .sum();
        Weighed {
          priority: gain * (1.0 + need),
          gain,
          need,
          memory,
        }
      })
      .collect();
    weighed.sort_by(before);

    // A tag pulls the episodes of its rooms written within a session of it, below it and no tags themselves,
    // to directly after it, in their own order.
    let tag = self.rules.tag_importance;
    let window = self.rules.same_session_ms;
    let mut placed = vec![false; weighed.len()];
    let mut order: Vec<(usize, Option<i64>)> = Vec::with_capacity(weighed.len());
    for rank in 0..weighed.len() {
      if placed[rank] {
        continue;
      }
      placed[rank] = true;
      order.push((rank, None));
      let tagging = &weighed[rank].memory;
      if tagging.importance < tag {
        continue;
      }
      let its_rooms = rooms(tagging);
      if its_rooms.is_empty() {
        continue;
      }
      for later in rank + 1..weighed.len() {
        let episode = &weighed[later].memory;
        let pulled = !placed[later]
          && episode.importance < tag
          && (episode.created_at - tagging.created_at).abs() <= window
          && !rooms(episode).is_disjoint(&its_rooms);
        if pulled {
          placed[later] = true;
          order.push((later, Some(tagging.id)));
        }
      }
    }
    order.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let mut slots: Vec<Option<Weighed>> = weighed.into_iter().map(Some).collect();
    Ok(
      order
        .into_iter()
        .filter_map(|(rank, tagged_by)| {
          slots[rank].take().map(|weighed| UnconsolidatedEpisode {
            memory: weighed.memory,
            priority: weighed.priority,
            gain: weighed.gain,
            need: weighed.need,
            tagged_by,
          })
        })
        .collect(),
    )
  }
}
