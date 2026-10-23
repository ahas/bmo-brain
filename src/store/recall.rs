//! What a turn is handed (docs/design.md, 「Recall」), a session's fold, and the persona rendered
//! (docs/design.md, 「Persona and lore」).

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{
  ALWAYS_LABEL, ALWAYS_VALUE, Fold, Labels, Memory, MemoryStatus, Recall, RecallAsk, RecallChoice,
  RecallPart, SubjectKey,
};
use sqlx::PgConnection;
use sqlx::types::Json;

use super::brains::{brain_of, given};
use super::rules::tokens;
use super::sheets::sheet_rows;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::{Fault, Held, Store, now};

/// The types the searched part never offers: the persona goes into a prompt by its own route, and an episode
/// is consolidation's material.
const UNSEARCHED: [&str; 2] = ["persona", "episode"];

/// The most candidates the searched part reads before scoring.
const CANDIDATES_MOST: i64 = 500;

/// The words of a text, lowercased, for relevance: runs of letters and digits.
fn words(text: &str) -> BTreeSet<String> {
  text
    .split(|c: char| !c.is_alphanumeric())
    .filter(|word| word.chars().count() >= 2)
    .map(|word| word.to_lowercase())
    .collect()
}

/// A date as the block and a sheet write it.
pub(crate) fn day(ms: i64) -> String {
  chrono::DateTime::from_timestamp_millis(ms)
    .map(|at| at.format("%Y-%m-%d").to_string())
    .unwrap_or_else(|| ms.to_string())
}

/// The subject the type and key name, where it stands.
async fn subject_id(conn: &mut PgConnection, brain_id: i64, named: &SubjectKey) -> Held<Option<i64>> {
  Ok(
    sqlx::query_scalar("SELECT id FROM subjects WHERE brain_id = $1 AND type = $2 AND key = $3")
      .bind(brain_id)
      .bind(&named.kind)
      .bind(&named.key)
      .fetch_optional(&mut *conn)
      .await?,
  )
}

struct Candidate {
  memory: Memory,
  relevance: f64,
}

impl Store {
  /// A turn's recall: the core that stands on every turn of the room, and the searched part under the budget.
  pub async fn recall(&self, brain_id: i64, ask: &RecallAsk) -> Held<Recall> {
    if let Some(session) = &ask.session_key {
      given("sessionKey", session)?;
    }
    let budget = match ask.budget_tokens {
      Some(budget) if budget >= 1 => budget,
      Some(budget) => {
        return Err(Fault::BadRequest(format!("budgetTokens is {budget}; it is at least 1.")));
      }
      None => self.rules.budget_tokens,
    };
    let at = now();
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let room = match &ask.room {
      Some(room) => subject_id(&mut tx, brain_id, room).await?,
      None => None,
    };
    let mut speakers = Vec::new();
    for speaker in &ask.speakers {
      if let Some(id) = subject_id(&mut tx, brain_id, speaker).await? {
        speakers.push(id);
      }
    }
    let named: Vec<i64> = room.into_iter().chain(speakers.iter().copied()).collect();

    // The core: the room's situation rules, its `always` rules at most to their cap in a sheet's order, and the
    // head of each speaker's sheet, at most the cap of their facts; the rest are candidates of the searched part
    // like any other row.
    let mut core: Vec<MemoryRow> = Vec::new();
    if let Some(room) = room {
      core.extend(
        sqlx::query_as::<_, MemoryRow>(&format!(
          "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' \
           AND type = 'situation' AND id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = $2) \
           ORDER BY importance DESC, id"
        ))
        .bind(brain_id)
        .bind(room)
        .fetch_all(&mut *tx)
        .await?,
      );
      core.extend(
        sqlx::query_as::<_, MemoryRow>(&format!(
          "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' \
           AND type = 'rule' AND labels @> $3 \
           AND id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = $2) \
           ORDER BY importance DESC, valid_from DESC, last_confirmed_at DESC NULLS LAST, id DESC LIMIT $4"
        ))
        .bind(brain_id)
        .bind(room)
        .bind(Json(Labels::from([(ALWAYS_LABEL.to_owned(), ALWAYS_VALUE.to_owned())])))
        .bind(self.rules.core_rule_cap.max(0))
        .fetch_all(&mut *tx)
        .await?,
      );
    }
    let mut core_ids: BTreeSet<i64> = core.iter().map(|row| row.id).collect();
    let cap = Some(self.rules.core_fact_cap.max(0));
    for speaker in &speakers {
      for row in sheet_rows(&mut tx, brain_id, *speaker, room, cap).await? {
        if core_ids.insert(row.id) {
          core.push(row);
        }
      }
    }
    let core = self.members(&mut tx, core).await?;

    // The searched part's candidates: live rows of the searched types, not of this session's own making,
    // holding one of the turn's words or about a subject the turn names.
    let asked = words(&ask.words);
    let mut candidates: Vec<MemoryRow> = Vec::new();
    if !asked.is_empty() || !named.is_empty() {
      let patterns: Vec<String> = asked.iter().map(|word| super::holding(word)).collect();
      candidates = sqlx::query_as(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' \
         AND NOT (type = ANY($2)) AND ($3::bigint IS NULL OR created_at <= $3) \
         AND (text ILIKE ANY($4) OR id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = ANY($5))) \
         ORDER BY last_hit_at DESC NULLS LAST, id DESC LIMIT $6"
      ))
      .bind(brain_id)
      .bind(UNSEARCHED.map(str::to_owned).to_vec())
      .bind(ask.session_opened_at)
      .bind(&patterns)
      .bind(&named)
      .bind(CANDIDATES_MOST)
      .fetch_all(&mut *tx)
      .await?;
    }
    candidates.retain(|row| !core_ids.contains(&row.id));
    let candidates = self.members(&mut tx, candidates).await?;
    let mut scored: Vec<Candidate> = candidates
      .into_iter()
      .map(|memory| {
        let mine = words(&memory.text);
        let overlap = asked.iter().filter(|word| mine.contains(*word)).count();
        let mut relevance = if asked.is_empty() { 0.0 } else { overlap as f64 / asked.len() as f64 };
        if memory.subjects.iter().any(|subject| named.contains(&subject.id)) {
          relevance += self.rules.subject_bonus;
        }
        Candidate { memory, relevance }
      })
      .collect();
    let (rel_lo, rel_hi) = span(scored.iter().map(|c| c.relevance));
    let (imp_lo, imp_hi) = span(scored.iter().map(|c| c.memory.importance as f64));
    let (rec_lo, rec_hi) = span(scored.iter().map(|c| c.memory.retrieval_strength));
    let rules = &self.rules;
    let mut ranked: Vec<(f64, Candidate)> = scored
      .drain(..)
      .map(|c| {
        let score = rules.weight_relevance * norm(c.relevance, rel_lo, rel_hi)
          + rules.weight_importance * norm(c.memory.importance as f64, imp_lo, imp_hi)
          + rules.weight_recency * norm(c.memory.retrieval_strength, rec_lo, rec_hi);
        (score, c)
      })
      .collect();
    ranked.sort_by(|a, b| {
      b.0
        .partial_cmp(&a.0)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a.1.memory.id.cmp(&b.1.memory.id))
    });

    // The marks: what this session was handed before, in these versions.
    let marked: BTreeSet<i64> = match &ask.session_key {
      Some(session) => {
        sqlx::query_scalar("SELECT memory_id FROM handouts WHERE brain_id = $1 AND session_key = $2")
          .bind(brain_id)
          .bind(session)
          .fetch_all(&mut *tx)
          .await?
          .into_iter()
          .collect()
      }
      None => BTreeSet::new(),
    };

    // The block: the core first, then the searched part by score, under the budget, with a subject's cap and
    // no near-double of a row already in.
    let mut lines: Vec<String> = Vec::new();
    let mut chosen: Vec<RecallChoice> = Vec::new();
    let mut spent = 0;
    let mut per_subject: BTreeMap<i64, i64> = BTreeMap::new();
    let mut seen_words: Vec<BTreeSet<String>> = Vec::new();
    for memory in core {
      let is_marked = marked.contains(&memory.id);
      let line = self.line(&mut tx, &memory, is_marked, at).await?;
      let cost = tokens(&line);
      if spent + cost > budget {
        continue;
      }
      spent += cost;
      lines.push(line);
      seen_words.push(words(&memory.text));
      chosen.push(RecallChoice {
        memory_id: memory.id,
        kind: memory.kind,
        part: RecallPart::Core,
        score: memory.importance as f64,
        marked: is_marked,
        token_count: cost,
      });
    }
    for (score, candidate) in ranked {
      let memory = candidate.memory;
      if candidate.relevance <= 0.0 {
        continue;
      }
      if memory
        .subjects
        .iter()
        .any(|s| per_subject.get(&s.id).copied().unwrap_or(0) >= rules.subject_cap)
      {
        continue;
      }
      let mine = words(&memory.text);
      if seen_words.iter().any(|seen| near(seen, &mine)) {
        continue;
      }
      let is_marked = marked.contains(&memory.id);
      let line = self.line(&mut tx, &memory, is_marked, at).await?;
      let cost = tokens(&line);
      if spent + cost > budget {
        continue;
      }
      spent += cost;
      for subject in &memory.subjects {
        *per_subject.entry(subject.id).or_default() += 1;
      }
      lines.push(line);
      seen_words.push(mine);
      chosen.push(RecallChoice {
        memory_id: memory.id,
        kind: memory.kind,
        part: RecallPart::Searched,
        score,
        marked: is_marked,
        token_count: cost,
      });
    }

    if ask.record.unwrap_or(true) {
      let handed: Vec<i64> = chosen.iter().filter(|c| !c.marked).map(|c| c.memory_id).collect();
      if !handed.is_empty() {
        self.hit(&mut tx, brain_id, &handed, ask.session_key.as_deref(), at).await?;
      }
    }
    let unconsolidated_importance = self.unconsolidated_importance(&mut tx, brain_id).await?;
    self.commit(tx).await?;
    Ok(Recall {
      block: lines.join("\n"),
      chosen,
      token_count: spent,
      budget_tokens: budget,
      unconsolidated_importance,
      consolidation_due: unconsolidated_importance >= self.rules.consolidation_threshold,
    })
  }

  /// One memory as a line of the block: its id, its type, since when it holds, its words, and what it
  /// replaced lately; a marked one carries its mark and no words.
  async fn line(&self, conn: &mut PgConnection, memory: &Memory, marked: bool, at: i64) -> Held<String> {
    let head = format!("- [#{} {} since {}]", memory.id, memory.kind, day(memory.valid_from));
    if marked {
      return Ok(format!("{head} (handed to this session already)"));
    }
    let heard = if memory.kind == "rumor" { "heard: " } else { "" };
    let mut line = format!("{head} {heard}{}", memory.text.trim());
    let window = at - self.rules.previous_value_days * 86_400_000;
    let previous: Option<(String, i64)> = sqlx::query_as(
      "SELECT text, valid_to FROM memories WHERE superseded_by = $1 AND valid_to >= $2 AND status = 'superseded'",
    )
    .bind(memory.id)
    .bind(window)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((text, until)) = previous {
      line.push_str(&format!(" (before, until {}: {})", day(until), text.trim()));
    }
    Ok(line)
  }

  /// A session folded or handed off: its marks cleared, so what it knew may attach to the next.
  pub async fn fold(&self, brain_id: i64, body: &Fold) -> Held<i64> {
    given("sessionKey", &body.session_key)?;
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let cleared = sqlx::query("DELETE FROM handouts WHERE brain_id = $1 AND session_key = $2")
      .bind(brain_id)
      .bind(&body.session_key)
      .execute(&mut *tx)
      .await?
      .rows_affected();
    self.commit(tx).await?;
    Ok(cleared as i64)
  }

  /// The persona rendered: the active persona pieces in their layers' order and their own, joined by a blank
  /// line, the same bytes until a piece changes; the pieces of the layers named alone where any are.
  pub async fn persona(&self, brain_id: i64, layers: &[String]) -> Held<String> {
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let pieces: Vec<String> = if layers.is_empty() {
      sqlx::query_scalar(
        "SELECT m.text FROM memories m LEFT JOIN layers l ON l.id = m.layer_id \
         WHERE m.brain_id = $1 AND m.type = 'persona' AND m.status = $2 \
         ORDER BY l.position NULLS LAST, m.layer_id NULLS LAST, m.position, m.id",
      )
      .bind(brain_id)
      .bind(MemoryStatus::Active.name())
      .fetch_all(&mut *conn)
      .await?
    } else {
      let standing: Vec<String> =
        sqlx::query_scalar("SELECT name FROM layers WHERE brain_id = $1 AND name = ANY($2)")
          .bind(brain_id)
          .bind(layers)
          .fetch_all(&mut *conn)
          .await?;
      for name in layers {
        if !standing.contains(name) {
          return Err(Fault::NotFound(format!("Brain {brain_id} has no layer {name:?}.")));
        }
      }
      sqlx::query_scalar(
        "SELECT m.text FROM memories m JOIN layers l ON l.id = m.layer_id \
         WHERE m.brain_id = $1 AND m.type = 'persona' AND m.status = $2 AND l.name = ANY($3) \
         ORDER BY l.position, m.layer_id, m.position, m.id",
      )
      .bind(brain_id)
      .bind(MemoryStatus::Active.name())
      .bind(layers)
      .fetch_all(&mut *conn)
      .await?
    };
    Ok(pieces.iter().map(|piece| piece.trim_end()).collect::<Vec<_>>().join("\n\n"))
  }
}

/// The least and the greatest of these values.
fn span(values: impl Iterator<Item = f64>) -> (f64, f64) {
  values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

/// A value scaled into 0 to 1 between the least and the greatest; one half where they are the same.
fn norm(value: f64, lo: f64, hi: f64) -> f64 {
  if hi <= lo {
    return 0.5;
  }
  (value - lo) / (hi - lo)
}

/// Whether two word sets are near-doubles: more than six tenths of the smaller is in the other.
fn near(a: &BTreeSet<String>, b: &BTreeSet<String>) -> bool {
  let smaller = a.len().min(b.len());
  if smaller == 0 {
    return false;
  }
  let shared = a.intersection(b).count();
  shared as f64 / smaller as f64 > 0.6
}
