//! The write judgement's material and its mechanics (docs/design.md, 「Writing」 and 「Consolidation」): the
//! live memories like a candidate, a learning applied whole, the unconsolidated episodes and their marks.

use std::collections::BTreeSet;

use bmo_brain_client::{
  Labels, Learned, Learning, Memory, MemoryMove, MemoryStatus, MemoryVersion, NewMemory, SimilarAsk,
  SubjectKey,
};
use sqlx::PgConnection;

use super::brains::{brain_of, given};
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::{Fault, Held, Store, now};

/// How many likes the judgement reads where the ask names no limit, and at most.
const SIMILAR: i64 = 10;
const SIMILAR_MOST: i64 = 50;
/// How many episodes a consolidation reads where the ask names no limit, and at most.
const EPISODES: i64 = 60;
const EPISODES_MOST: i64 = 500;

/// The types a candidate is never compared against.
const UNCOMPARED: [&str; 2] = ["episode", "persona"];

/// The words of a text, lowercased: runs of letters and digits of two characters or more.
fn words(text: &str) -> BTreeSet<String> {
  text
    .split(|c: char| !c.is_alphanumeric())
    .filter(|word| word.chars().count() >= 2)
    .map(|word| word.to_lowercase())
    .collect()
}

/// The ids of the subjects these keys name, where they stand.
async fn standing_subjects(conn: &mut PgConnection, brain_id: i64, named: &[SubjectKey]) -> Held<Vec<i64>> {
  let mut ids = Vec::new();
  for subject in named {
    let found: Option<i64> =
      sqlx::query_scalar("SELECT id FROM subjects WHERE brain_id = $1 AND type = $2 AND key = $3")
        .bind(brain_id)
        .bind(&subject.kind)
        .bind(&subject.key)
        .fetch_optional(&mut *conn)
        .await?;
    ids.extend(found);
  }
  Ok(ids)
}

impl Store {
  /// The live memories most like a candidate: about one of its subjects or holding its words, scored by the
  /// share of the candidate's words they hold and whether they share a subject, at most the limit.
  pub async fn similar(&self, brain_id: i64, ask: &SimilarAsk) -> Held<Vec<Memory>> {
    let limit = match ask.limit {
      None => SIMILAR,
      Some(limit) if (1..=SIMILAR_MOST).contains(&limit) => limit,
      Some(limit) => {
        return Err(Fault::BadRequest(format!("limit is {limit}; it is 1 to {SIMILAR_MOST}.")));
      }
    };
    let asked = words(&ask.text);
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let subject_ids = standing_subjects(&mut conn, brain_id, &ask.subjects).await?;
    if asked.is_empty() && subject_ids.is_empty() {
      return Ok(vec![]);
    }
    let patterns: Vec<String> = asked.iter().map(|word| super::holding(word)).collect();
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' AND NOT (type = ANY($2)) \
       AND (text ILIKE ANY($3) OR id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = ANY($4))) \
       ORDER BY id DESC LIMIT 500"
    ))
    .bind(brain_id)
    .bind(UNCOMPARED.map(str::to_owned).to_vec())
    .bind(&patterns)
    .bind(&subject_ids)
    .fetch_all(&mut *conn)
    .await?;
    let members = self.members(&mut conn, rows).await?;
    let mut scored: Vec<(f64, Memory)> = members
      .into_iter()
      .map(|memory| {
        let mine = words(&memory.text);
        let overlap = if asked.is_empty() {
          0.0
        } else {
          asked.iter().filter(|word| mine.contains(*word)).count() as f64 / asked.len() as f64
        };
        let shared = memory.subjects.iter().any(|s| subject_ids.contains(&s.id));
        (overlap + if shared { self.rules.subject_bonus } else { 0.0 }, memory)
      })
      .collect();
    scored
      .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(b.1.id.cmp(&a.1.id)));
    Ok(
      scored
        .into_iter()
        .filter(|(score, _)| *score > 0.0)
        .take(limit as usize)
        .map(|(_, memory)| memory)
        .collect(),
    )
  }

  /// A learning applied whole: the memories it confirms confirmed; the one it strengthens given a new
  /// version; the ones it replaces closed, unless it is heard; and the row written as a fact or, heard, as a
  /// rumor. One transaction, so a judgement is never half applied.
  pub async fn learn(&self, brain_id: i64, body: &Learning) -> Held<Learned> {
    given("actorName", &body.actor_name)?;
    given("type", &body.kind)?;
    let at = now();
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let heard = body.source.is_heard();
    let mut learned = Learned {
      confirmed_count: 0,
      invalidated_ids: vec![],
      written: None,
      versioned: false,
    };
    let mut confirms = body.confirms.clone();
    confirms.sort_unstable();
    confirms.dedup();
    if !confirms.is_empty() {
      learned.confirmed_count = self.confirm_in(&mut tx, brain_id, &confirms, at).await?;
    }
    let text = body.text.trim();
    let only_confirms =
      text.is_empty() || (!confirms.is_empty() && body.strengthens.is_none() && body.replaces.is_empty());
    if only_confirms {
      self.commit(tx).await?;
      return Ok(learned);
    }
    let mut subjects = body.subjects.clone();
    for ground in &body.grounds {
      let is_episode: Option<i64> =
        sqlx::query_scalar("SELECT id FROM memories WHERE brain_id = $1 AND id = $2 AND type = 'episode'")
          .bind(brain_id)
          .bind(ground)
          .fetch_optional(&mut *tx)
          .await?;
      if is_episode.is_none() {
        return Err(Fault::BadRequest(format!(
          "grounds names {ground}, which is no episode of brain {brain_id}."
        )));
      }
      subjects.push(SubjectKey {
        kind: "episode".to_owned(),
        key: ground.to_string(),
      });
    }
    let kind = if heard && body.kind == "fact" { "rumor".to_owned() } else { body.kind.clone() };
    let mut labels = body.labels.clone();
    labels.insert(
      "source".to_owned(),
      serde_json::to_value(body.source)
        .and_then(serde_json::from_value)
        .unwrap_or_else(|_| "unknown".to_owned()),
    );

    if let Some(id) = body.strengthens {
      let memory = self
        .version_in(
          &mut tx,
          brain_id,
          id,
          &MemoryVersion {
            text: Some(text.to_owned()),
            kind: Some(kind),
            layer_id: body.layer_id.map(Some),
            position: None,
            subject_ids: None,
            subjects: if subjects.is_empty() { None } else { Some(subjects) },
            importance: body.importance,
            valid_from: None,
            actor_name: body.actor_name.clone(),
            labels: Some(labels),
          },
          at,
        )
        .await?;
      learned.written = Some(memory);
      learned.versioned = true;
      self.commit(tx).await?;
      return Ok(learned);
    }

    if !heard {
      let mut replaces = body.replaces.clone();
      replaces.sort_unstable();
      replaces.dedup();
      for id in replaces {
        let moved = self
          .move_in(
            &mut tx,
            brain_id,
            id,
            &MemoryMove {
              status: MemoryStatus::Invalidated,
              at: Some(at),
              actor_name: body.actor_name.clone(),
            },
          )
          .await?;
        if moved.status == MemoryStatus::Invalidated {
          learned.invalidated_ids.push(id);
        }
      }
    }
    let memory = self
      .add_in(
        &mut tx,
        brain_id,
        &NewMemory {
          key: body.key.clone(),
          kind,
          text: text.to_owned(),
          layer_id: body.layer_id,
          position: None,
          subject_ids: vec![],
          subjects,
          importance: body.importance,
          valid_from: None,
          status: None,
          actor_name: body.actor_name.clone(),
          labels,
        },
        at,
      )
      .await?;
    learned.written = Some(memory);
    self.commit(tx).await?;
    Ok(learned)
  }

  /// The episodes not yet consolidated, most important first, at most the limit.
  pub async fn unconsolidated(&self, brain_id: i64, limit: Option<i64>) -> Held<Vec<Memory>> {
    let limit = match limit {
      None => EPISODES,
      Some(limit) if (1..=EPISODES_MOST).contains(&limit) => limit,
      Some(limit) => {
        return Err(Fault::BadRequest(format!("limit is {limit}; it is 1 to {EPISODES_MOST}.")));
      }
    };
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND type = 'episode' AND status = 'active' \
       AND consolidated_at IS NULL ORDER BY importance DESC, id LIMIT $2"
    ))
    .bind(brain_id)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    self.members(&mut conn, rows).await
  }

  /// The sum of the importance of a brain's unconsolidated episodes, read within a transaction.
  pub(crate) async fn unconsolidated_importance(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
  ) -> Held<i64> {
    let sum: Option<i64> = sqlx::query_scalar(
      "SELECT SUM(importance)::bigint FROM memories WHERE brain_id = $1 AND type = 'episode' AND status = 'active' \
       AND consolidated_at IS NULL",
    )
    .bind(brain_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(sum.unwrap_or(0))
  }

  /// Episodes marked consolidated; how many the ids reached.
  pub async fn consolidated(&self, brain_id: i64, episode_ids: &[i64]) -> Held<i64> {
    if episode_ids.is_empty() {
      return Err(Fault::BadRequest("episodeIds names no row.".to_owned()));
    }
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let marked: Vec<i64> = sqlx::query_scalar(
      "UPDATE memories SET consolidated_at = $3 WHERE brain_id = $1 AND id = ANY($2) AND type = 'episode' \
       AND consolidated_at IS NULL RETURNING id",
    )
    .bind(brain_id)
    .bind(episode_ids)
    .bind(now())
    .fetch_all(&mut *tx)
    .await?;
    self.commit(tx).await?;
    Ok(marked.len() as i64)
  }
}

/// Labels with a learning's source, for a test or a caller that builds them.
pub fn source_labels(source: &str) -> Labels {
  Labels::from([("source".to_owned(), source.to_owned())])
}
