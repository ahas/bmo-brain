//! The write judgement's material and its mechanics (docs/design.md, 「Writing」 and 「Consolidation」): the
//! live memories like a candidate, a learning applied whole, the unconsolidated episodes and their marks.

use bmo_brain_client::{
  Labels, Learned, Learning, Memory, MemoryMove, MemoryStatus, MemoryVersion, NewMemory, SimilarAsk,
  SubjectKey,
};
use sqlx::PgConnection;

use super::brains::{brain_of, given};
use super::misses::Asked;
use super::orgs::{PROPOSAL, heads_only, judged_by_keys, proposes};
use super::recall::Cues;
use super::sheets::written_type;
use super::vectors::fuse;
use super::versions::{HELD_AT, MEMORY_COLUMNS, MemoryRow};
use super::words::{index_words, stems};
use super::{Fault, Held, Store, now};

/// How many likes the judgement reads where the ask names no limit, and at most.
const SIMILAR: i64 = 10;
const SIMILAR_MOST: i64 = 50;

/// The types a candidate is never compared against.
const UNCOMPARED: [&str; 2] = ["episode", "persona"];

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
  /// The live memories most like a candidate: about one of its subjects or holding one of its stems, ordered by
  /// the relevance recall reckons with the candidate's stems and subjects as the cues, at most the limit.
  pub async fn similar(&self, brain_id: i64, ask: &SimilarAsk) -> Held<Vec<Memory>> {
    let limit = match ask.limit {
      None => SIMILAR,
      Some(limit) if (1..=SIMILAR_MOST).contains(&limit) => limit,
      Some(limit) => {
        return Err(Fault::BadRequest(format!("limit is {limit}; it is 1 to {SIMILAR_MOST}.")));
      }
    };
    let asked = stems(&ask.text);
    // The candidate's text embedded before anything of the brain is held; a reading as of a past moment is by
    // words alone, the vectors being the live rows'.
    let query = if ask.as_of.is_none() { self.query_vector(&ask.text).await } else { None };
    let mut conn = self.begin().await?;
    brain_of(&mut conn, brain_id).await?;
    let mut subject_ids = standing_subjects(&mut conn, brain_id, &ask.subjects).await?;
    subject_ids.sort_unstable();
    subject_ids.dedup();
    if asked.is_empty() && subject_ids.is_empty() && query.is_none() {
      self.count_ask(&mut conn, brain_id, Asked::Similar, &ask.text, false).await?;
      self.commit(conn).await?;
      return Ok(vec![]);
    }
    index_words(&mut conn, brain_id).await?;
    let cue_stems: Vec<String> = asked.iter().cloned().collect();
    // The live rows, or those that held at the moment asked for.
    let held = match ask.as_of {
      Some(at) => HELD_AT.replace("$AT", &at.to_string()),
      None => "status = 'active'".to_owned(),
    };
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND {held} AND NOT (type = ANY($2)) \
       AND (words && $3 OR id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = ANY($4))) \
       ORDER BY id DESC LIMIT 500"
    ))
    .bind(brain_id)
    .bind(UNCOMPARED.map(str::to_owned).to_vec())
    .bind(&cue_stems)
    .bind(&subject_ids)
    .fetch_all(&mut *conn)
    .await?;
    let members = self.members(&mut conn, rows).await?;
    let cues = Cues::read(&mut conn, brain_id, &subject_ids, &asked).await?;
    let mut scored: Vec<(f64, Memory)> = members
      .into_iter()
      .filter_map(|memory| {
        let mine = stems(&memory.text);
        let subjects: Vec<i64> = memory.subjects.iter().map(|subject| subject.id).collect();
        cues
          .reaches(&subjects, &mine)
          .then(|| (cues.relevance(&self.rules, &subjects, &mine), memory))
      })
      .collect();
    scored
      .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(b.1.id.cmp(&a.1.id)));
    let word_list: Vec<(i64, f64)> =
      scored.iter().map(|(relevance, memory)| (memory.id, *relevance)).collect();
    let mut rows_by_id: std::collections::HashMap<i64, Memory> =
      scored.into_iter().map(|(_, memory)| (memory.id, memory)).collect();

    // The live rows nearest the candidate's text in meaning, fused with the word list by Reciprocal Rank Fusion.
    let mut vector_list: Vec<(i64, f64)> = Vec::new();
    if let Some(query) = &query {
      let kept = usize::try_from(self.rules.vector_candidates.max(0)).unwrap_or(0);
      let near = self.nearest_live(&mut conn, brain_id, query, kept).await?;
      let ids: Vec<i64> = near.iter().map(|(id, _)| *id).collect();
      let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND id = ANY($2) AND status = 'active' \
         AND NOT (type = ANY($3))"
      ))
      .bind(brain_id)
      .bind(&ids)
      .bind(UNCOMPARED.map(str::to_owned).to_vec())
      .fetch_all(&mut *conn)
      .await?;
      let found = self.members(&mut conn, rows).await?;
      let mut found: std::collections::HashMap<i64, Memory> =
        found.into_iter().map(|memory| (memory.id, memory)).collect();
      for (id, cosine) in near {
        if vector_list.len() >= kept {
          break;
        }
        if let Some(memory) = found.remove(&id) {
          vector_list.push((id, cosine));
          rows_by_id.entry(id).or_insert(memory);
        }
      }
    }
    let fused = fuse(&[&word_list, &vector_list], self.rules.rrf_k);
    let mut ranked: Vec<(f64, Memory)> = rows_by_id
      .into_values()
      .map(|memory| (fused.get(&memory.id).copied().unwrap_or(0.0), memory))
      .collect();
    ranked
      .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(b.1.id.cmp(&a.1.id)));
    let found: Vec<Memory> = ranked.into_iter().take(limit as usize).map(|(_, memory)| memory).collect();
    self
      .count_ask(&mut conn, brain_id, Asked::Similar, &ask.text, !found.is_empty())
      .await?;
    self.commit(conn).await?;
    Ok(found)
  }

  /// A learning applied whole: the memories it confirms confirmed; the one it strengthens given a new
  /// version; the ones it replaces closed, unless it is heard; and the row written as a fact or, heard, as a
  /// rumor. A member's learning in an org brain is a proposal: heard, written as a rumor whatever its type,
  /// and strengthening nothing. One transaction, so a judgement is never half applied.
  pub async fn learn(&self, brain_id: i64, body: &Learning) -> Held<Learned> {
    given("actorName", &body.actor_name)?;
    written_type(&body.kind)?;
    let at = now();
    let mut tx = self.begin().await?;
    let brain = brain_of(&mut tx, brain_id).await?;
    let standing = judged_by_keys(body.standing, &body.subjects, body.shared);
    let proposal = proposes(&brain, standing);
    if body.strengthens.is_some() {
      heads_only(&brain, standing, "learning that strengthens a memory")?;
    }
    let heard = proposal || body.source.is_heard();
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
    let kind = if proposal || (heard && body.kind == "fact") {
      PROPOSAL.to_owned()
    } else {
      body.kind.clone()
    };
    let mut labels = body.labels.clone();
    labels.insert(
      "source".to_owned(),
      serde_json::to_value(body.source)
        .and_then(serde_json::from_value)
        .unwrap_or_else(|_| "unknown".to_owned()),
    );

    // A learning that began earlier than every live row it would close closes none: it is written behind the
    // one of them that began first (docs/design.md, 「Versions」).
    let began = body.valid_from.unwrap_or(at);
    let closing: Vec<i64> = match body.strengthens {
      Some(id) => vec![id],
      None if !heard => body.replaces.clone(),
      None => vec![],
    };
    let mut behind: Option<i64> = None;
    if !closing.is_empty() {
      let spans: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id, valid_from FROM memories WHERE brain_id = $1 AND id = ANY($2) \
         AND status IN ('active', 'pending') ORDER BY valid_from, id",
      )
      .bind(brain_id)
      .bind(&closing)
      .fetch_all(&mut *tx)
      .await?;
      let all_live = spans.len() == closing.iter().collect::<std::collections::BTreeSet<_>>().len();
      if all_live && spans.iter().all(|(_, since)| began < *since) {
        behind = spans.first().map(|(id, _)| *id);
      }
    }

    if let Some(id) = body.strengthens.filter(|_| behind.is_none()) {
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
            valid_from: body.valid_from,
            recall_condition: body.recall_condition.clone().map(Some),
            release_condition: body.release_condition.clone().map(Some),
            actor_name: body.actor_name.clone(),
            labels: Some(labels),
            locked: body.locked,
            internal_only: body.internal_only,
            shared: body.shared,
            standing: body.standing,
          },
          at,
        )
        .await?;
      learned.written = Some(memory);
      learned.versioned = true;
      self.commit(tx).await?;
      return Ok(learned);
    }

    if !heard && behind.is_none() {
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
              reason: None,
              text: None,
              standing: body.standing,
            },
          )
          .await?;
        if moved.status == MemoryStatus::Invalidated {
          learned.invalidated_ids.push(id);
        }
      }
    }
    let memory = self
      .write_in(
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
          valid_from: body.valid_from,
          recall_condition: body.recall_condition.clone(),
          release_condition: body.release_condition.clone(),
          status: None,
          actor_name: body.actor_name.clone(),
          labels,
          locked: body.locked,
          internal_only: body.internal_only,
          shared: body.shared,
          standing: body.standing,
        },
        at,
        behind,
      )
      .await?;
    learned.written = Some(memory);
    self.commit(tx).await?;
    Ok(learned)
  }

  /// Whether a consolidation is due, as a recall says it: the same sum and threshold, read alone, nothing
  /// recorded.
  pub async fn consolidation(&self, brain_id: i64) -> Held<bmo_brain_client::ConsolidationState> {
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let unconsolidated_importance = self.unconsolidated_importance(&mut conn, brain_id).await?;
    let unconsolidated_count: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM memories WHERE brain_id = $1 AND type = 'episode' AND status = 'active' \
       AND consolidated_at IS NULL",
    )
    .bind(brain_id)
    .fetch_one(&mut *conn)
    .await?;
    let threshold = self.rules.consolidation_threshold;
    Ok(bmo_brain_client::ConsolidationState {
      unconsolidated_importance,
      unconsolidated_count,
      threshold,
      consolidation_due: unconsolidated_importance >= threshold,
    })
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
