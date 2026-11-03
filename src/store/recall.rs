//! What a turn is handed (docs/design.md, 「Recall」), a session's fold, and the persona rendered
//! (docs/design.md, 「Persona and lore」).

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{
  Fold, Memory, MemoryStatus, Recall, RecallAsk, RecallChoice, RecallPart, SubjectKey,
};
use sqlx::PgConnection;

use super::brains::{brain_of, event_scope, given};
use super::rules::{Rules, tokens};
use super::sheets::sheet_rows;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::words::{index_words, overlap, stems};
use super::{Fault, Held, Store, now, publish};

/// The types the searched part never offers: the persona goes into a prompt by its own route, and an episode
/// is consolidation's material.
const UNSEARCHED: [&str; 2] = ["persona", "episode"];

/// The most candidates the searched part reads before scoring.
const CANDIDATES_MOST: i64 = 500;

/// The words of a text, lowercased, as a condition is matched by: runs of letters and digits.
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

/// A turn's cues and how many live memories of the brain each is linked to (docs/design.md, 「Recall」): the
/// subjects it names that stand and the stems its words hold, read once for a recall or a similar ask, with
/// the number of the brain's live memories and their mean number of stems.
pub(crate) struct Cues {
  subjects: BTreeMap<i64, i64>,
  stems: BTreeMap<String, i64>,
  live: i64,
  mean_length: f64,
}

impl Cues {
  /// The cues and their fans: for a subject the live rows about it, for a stem the live rows holding it.
  pub(crate) async fn read(
    conn: &mut PgConnection,
    brain_id: i64,
    subjects: &[i64],
    stems: &BTreeSet<String>,
  ) -> Held<Self> {
    let mut by_subject: BTreeMap<i64, i64> = subjects.iter().map(|id| (*id, 0)).collect();
    let fans: Vec<(i64, i64)> = sqlx::query_as(
      "SELECT ms.subject_id, COUNT(*) FROM memory_subjects ms JOIN memories m ON m.id = ms.memory_id \
       WHERE ms.subject_id = ANY($1) AND m.brain_id = $2 AND m.status IN ('active', 'pending') \
       GROUP BY ms.subject_id",
    )
    .bind(subjects)
    .bind(brain_id)
    .fetch_all(&mut *conn)
    .await?;
    by_subject.extend(fans);
    let asked: Vec<String> = stems.iter().cloned().collect();
    let mut by_stem: BTreeMap<String, i64> = asked.iter().map(|stem| (stem.clone(), 0)).collect();
    let fans: Vec<(String, i64)> = sqlx::query_as(
      "SELECT stem, COUNT(*) FROM memories, UNNEST(words) AS stem WHERE brain_id = $1 \
       AND status IN ('active', 'pending') AND words && $2 AND stem = ANY($2) GROUP BY stem",
    )
    .bind(brain_id)
    .bind(&asked)
    .fetch_all(&mut *conn)
    .await?;
    by_stem.extend(fans);
    let (live, mean_length): (i64, f64) = sqlx::query_as(
      "SELECT COUNT(*), COALESCE(AVG(COALESCE(cardinality(words), 0)), 0)::float8 FROM memories \
       WHERE brain_id = $1 AND status IN ('active', 'pending')",
    )
    .bind(brain_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Self {
      subjects: by_subject,
      stems: by_stem,
      live,
      mean_length,
    })
  }

  /// The most a cue set gives a row: the log of the brain's live memories, as though every cue reached that
  /// row alone.
  pub(crate) fn most(&self) -> f64 {
    (self.live.max(1) as f64).ln()
  }

  /// The fans of the subject cues a memory about these subjects is linked to.
  fn subject_fans<'a>(&'a self, subjects: &'a [i64]) -> impl Iterator<Item = i64> + 'a {
    subjects.iter().filter_map(|id| self.subjects.get(id).copied())
  }

  /// The fans of the stem cues a memory holding these stems is linked to.
  fn stem_fans<'a>(&'a self, stems: &'a BTreeSet<String>) -> impl Iterator<Item = i64> + 'a {
    stems.iter().filter_map(|stem| self.stems.get(stem).copied())
  }

  /// Whether any cue reaches such a memory.
  pub(crate) fn reaches(&self, subjects: &[i64], stems: &BTreeSet<String>) -> bool {
    self.subject_fans(subjects).next().is_some() || self.stem_fans(stems).next().is_some()
  }

  /// The relevance of such a memory: each cue linked to it gives its share of the attention times the log of
  /// the brain's live memories over its fan, never below zero; a stem's weighed by BM25's length term for a
  /// row of this many stems against the brain's mean.
  pub(crate) fn relevance(&self, rules: &Rules, subjects: &[i64], stems: &BTreeSet<String>) -> f64 {
    let count = self.subjects.len() + self.stems.len();
    if count == 0 {
      return 0.0;
    }
    let share = 1.0 / count as f64;
    let live = self.live.max(1);
    let weight = |fan: i64| share * (live as f64 / fan.clamp(1, live) as f64).ln();
    let relative = if self.mean_length > 0.0 { stems.len() as f64 / self.mean_length } else { 1.0 };
    let length =
      (rules.bm25_k1 + 1.0) / (1.0 + rules.bm25_k1 * (1.0 - rules.bm25_b + rules.bm25_b * relative));
    self.subject_fans(subjects).map(weight).sum::<f64>()
      + self.stem_fans(stems).map(|fan| weight(fan) * length).sum::<f64>()
  }
}

/// A row as recall weighs it: its stems, its relevance to the turn, and whether the turn says its recall
/// condition or a cue reaches it.
struct Weighed {
  memory: Memory,
  stems: BTreeSet<String>,
  relevance: f64,
  reached: bool,
}

/// The order of the searched part: relevance, highest first; then importance, highest first; then the latest
/// use, a row never used after the used ones; then the id.
fn before(a: &Weighed, b: &Weighed) -> std::cmp::Ordering {
  b.relevance
    .partial_cmp(&a.relevance)
    .unwrap_or(std::cmp::Ordering::Equal)
    .then(b.memory.importance.cmp(&a.memory.importance))
    .then(b.memory.last_used_at.cmp(&a.memory.last_used_at))
    .then(a.memory.id.cmp(&b.memory.id))
}

impl Store {
  /// A row weighed for the turn: what the cues spread to it, the most a cue set gives where the turn says its
  /// condition.
  fn weigh(&self, memory: Memory, cues: &Cues, asked: &BTreeSet<String>) -> Weighed {
    let rules = &self.rules;
    let stems = stems(&memory.text);
    let said = memory
      .recall_condition
      .as_deref()
      .is_some_and(|condition| holds(condition, asked, rules.condition_share));
    let subjects: Vec<i64> = memory.subjects.iter().map(|subject| subject.id).collect();
    let (relevance, reached) = if said {
      (cues.most(), true)
    } else {
      (cues.relevance(rules, &subjects, &stems), cues.reaches(&subjects, &stems))
    };
    Weighed {
      memory,
      stems,
      relevance,
      reached,
    }
  }

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
    let mut named: Vec<i64> = room.into_iter().chain(speakers.iter().copied()).collect();
    named.sort_unstable();
    named.dedup();
    let asked = words(&ask.words);
    let turn_stems = stems(&ask.words);

    // The situations this turn holds, and with them the rows they end: a row whose release condition the turn
    // says is not handed out, and is moved to invalidated with that condition as its reason.
    let released = self.release_in(&mut tx, brain_id, &asked, at).await?;
    index_words(&mut tx, brain_id).await?;

    // The core: the room's situation rules and the head of each speaker's sheet, at most the cap of their facts;
    // the room's rules and the rest are candidates of the searched part like any other row.
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
    // holding one of the turn's stems, carrying a recall condition one of its words is in, or about a subject
    // the turn names.
    let mut candidates: Vec<MemoryRow> = Vec::new();
    if !turn_stems.is_empty() || !asked.is_empty() || !named.is_empty() {
      let patterns: Vec<String> = asked.iter().map(|word| super::holding(word)).collect();
      let cue_stems: Vec<String> = turn_stems.iter().cloned().collect();
      candidates = sqlx::query_as(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' \
         AND NOT (type = ANY($2)) AND ($3::bigint IS NULL OR created_at <= $3) \
         AND (words && $4 OR recall_condition ILIKE ANY($5) \
              OR id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = ANY($6))) \
         ORDER BY last_hit_at DESC NULLS LAST, id DESC LIMIT $7"
      ))
      .bind(brain_id)
      .bind(UNSEARCHED.map(str::to_owned).to_vec())
      .bind(ask.session_opened_at)
      .bind(&cue_stems)
      .bind(&patterns)
      .bind(&named)
      .bind(CANDIDATES_MOST)
      .fetch_all(&mut *tx)
      .await?;
    }
    candidates.retain(|row| !core_ids.contains(&row.id));
    let candidates = self.members(&mut tx, candidates).await?;

    // Each candidate's relevance, from the cues that reach it or the condition the turn says; a row neither
    // reaches is no candidate. The candidates then stand in the searched part's order.
    let cues = Cues::read(&mut tx, brain_id, &named, &turn_stems).await?;
    let mut standing: Vec<Weighed> = candidates
      .into_iter()
      .map(|memory| self.weigh(memory, &cues, &asked))
      .filter(|weighed| weighed.reached)
      .collect();
    standing.sort_by(before);

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

    // The block: the core first, then the searched part by Maximal Marginal Relevance over that order, under
    // the budget and a subject's cap.
    let mut lines: Vec<String> = Vec::new();
    let mut chosen: Vec<RecallChoice> = Vec::new();
    let mut spent = 0;
    let mut per_subject: BTreeMap<i64, i64> = BTreeMap::new();
    let mut chosen_stems: Vec<BTreeSet<String>> = Vec::new();
    for memory in core {
      let weighed = self.weigh(memory, &cues, &asked);
      let memory = &weighed.memory;
      let is_marked = marked.contains(&memory.id);
      let line = self.line(&mut tx, memory, is_marked, at).await?;
      let cost = tokens(&line);
      if spent + cost > budget {
        continue;
      }
      spent += cost;
      lines.push(line);
      chosen.push(RecallChoice {
        memory_id: memory.id,
        kind: memory.kind.clone(),
        part: RecallPart::Core,
        score: weighed.relevance,
        marked: is_marked,
        token_count: cost,
      });
      chosen_stems.push(weighed.stems);
    }
    let (least, most) = span(standing.iter().map(|weighed| weighed.relevance));
    let rescaled = |relevance: f64| if most > least { (relevance - least) / (most - least) } else { 1.0 };
    let lambda = self.rules.mmr_lambda;
    while !standing.is_empty() {
      let marginal = |weighed: &Weighed| {
        let likest = chosen_stems.iter().map(|stems| overlap(&weighed.stems, stems)).fold(0.0, f64::max);
        lambda * rescaled(weighed.relevance) - (1.0 - lambda) * likest
      };
      // The first of the order with the greatest marginal value: the order breaks every tie.
      let mut next = 0;
      let mut best = marginal(&standing[0]);
      for (index, weighed) in standing.iter().enumerate().skip(1) {
        let value = marginal(weighed);
        if value > best {
          next = index;
          best = value;
        }
      }
      let weighed = standing.remove(next);
      let memory = &weighed.memory;
      if memory
        .subjects
        .iter()
        .any(|s| per_subject.get(&s.id).copied().unwrap_or(0) >= self.rules.subject_cap)
      {
        continue;
      }
      let is_marked = marked.contains(&memory.id);
      let line = self.line(&mut tx, memory, is_marked, at).await?;
      let cost = tokens(&line);
      if spent + cost > budget {
        continue;
      }
      spent += cost;
      for subject in &memory.subjects {
        *per_subject.entry(subject.id).or_default() += 1;
      }
      lines.push(line);
      chosen.push(RecallChoice {
        memory_id: memory.id,
        kind: memory.kind.clone(),
        part: RecallPart::Searched,
        score: weighed.relevance,
        marked: is_marked,
        token_count: cost,
      });
      chosen_stems.push(weighed.stems);
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
      released,
    })
  }

  /// The rows this turn ends: each live row whose release condition the turn's words hold, moved to
  /// `invalidated` with the moment and the condition, in the recall's own transaction.
  async fn release_in(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
    asked: &BTreeSet<String>,
    at: i64,
  ) -> Held<Vec<i64>> {
    if asked.is_empty() {
      return Ok(Vec::new());
    }
    let patterns: Vec<String> = asked.iter().map(|word| super::holding(word)).collect();
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' \
       AND release_condition IS NOT NULL AND release_condition ILIKE ANY($2) ORDER BY id"
    ))
    .bind(brain_id)
    .bind(&patterns)
    .fetch_all(&mut *conn)
    .await?;
    let mut released = Vec::new();
    for row in rows {
      let Some(condition) = row.release_condition.as_deref() else {
        continue;
      };
      if !holds(condition, asked, self.rules.condition_share) {
        continue;
      }
      sqlx::query(
        "UPDATE memories SET status = 'invalidated', valid_to = $2, superseded_at = $2, reason = $3 WHERE id = $1",
      )
        .bind(row.id)
        .bind(at)
        .bind(condition)
        .execute(&mut *conn)
        .await?;
      let memory = self.member_of(conn, brain_id, row.id).await?;
      publish(conn, "memory.moved", Some(&event_scope(brain_id)), &memory).await?;
      released.push(row.id);
    }
    Ok(released)
  }

  /// One memory as a line of the block: its id, its type, since when it holds, its words, and what it
  /// replaced lately; a marked one carries its mark and no words.
  async fn line(&self, conn: &mut PgConnection, memory: &Memory, marked: bool, at: i64) -> Held<String> {
    let until = memory.valid_to.map(|to| format!(" until {}", day(to))).unwrap_or_default();
    let head = format!("- [#{} {} since {}{until}]", memory.id, memory.kind, day(memory.valid_from));
    if marked {
      return Ok(format!("{head} (handed to this session already)"));
    }
    let heard = if memory.kind == "rumor" { "heard: " } else { "" };
    let mut line = format!("{head} {heard}{}", memory.text.trim());
    let window = at - self.rules.previous_value_days * 86_400_000;
    let previous: Option<(String, i64, String)> = sqlx::query_as(
      "SELECT text, valid_to, status FROM memories WHERE superseded_by = $1 AND valid_to >= $2 \
       AND status IN ('superseded', 'retracted') ORDER BY id DESC LIMIT 1",
    )
    .bind(memory.id)
    .bind(window)
    .fetch_optional(&mut *conn)
    .await?;
    match previous {
      Some((text, _, status)) if status == MemoryStatus::Retracted.name() => {
        line.push_str(&format!(" (before, retracted: {})", text.trim()));
      }
      Some((text, until, _)) => line.push_str(&format!(" (before, until {}: {})", day(until), text.trim())),
      None => {}
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

/// Whether the turn's words hold a condition: at least `share` of the condition's words are among them.
fn holds(condition: &str, asked: &BTreeSet<String>, share: f64) -> bool {
  let mine = words(condition);
  if mine.is_empty() {
    return false;
  }
  let held = mine.intersection(asked).count() as f64 / mine.len() as f64;
  held >= share
}
