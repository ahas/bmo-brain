//! What a holder leaves in the brains of a deployment (docs/design.md, 「What a holder leaves」): one key's
//! standing set declared whole, written into every agent's brain and into each brain as it is made, refused to
//! every write that would change or close a row of it, and withdrawn by that key alone.

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{
  BrainsHeld, Declaration, Declared, DeclaredMemory, Holder, HoldersQuery, Labels, NewMemory, Page,
  SubjectKey, SubjectRef,
};
use sqlx::PgConnection;

use super::brains::event_scope;
use super::sheets::written_type;
use super::versions::{DEFAULT_IMPORTANCE, MemoryRow, importance, subjects_by_row};
use super::{Fault, Held, Store, TEXT_MOST};
use super::{key as key_rule, now, publish, text as text_rule};

/// The reason a withdrawal records on every row it closes, and the reason a declaration records on a row of its
/// own holder that the new set no longer stands for.
pub const HOLDER_WITHDRAWN: &str = "holder-withdrawn";
pub const HOLDER_REPLACED: &str = "holder-replaced";

/// The most rows one holder may declare at once.
const DECLARED_MOST: usize = 100;

/// The row a declared set stands for in one brain, with its subjects as the holder's words for them.
struct Standing {
  id: i64,
  kind: String,
  text: String,
  importance: i16,
  recall_condition: Option<String>,
  release_condition: Option<String>,
  labels: Labels,
  subjects: Vec<(String, String)>,
}

/// Why a write that would change or close a row a holder left is refused; nothing where the row is the brain's
/// own. Every write that versions, merges, moves or restores a row asks here (docs/design.md,
/// 「What a holder leaves」).
pub(crate) fn held(row: &MemoryRow, what: &str) -> Held<()> {
  let Some(holder) = &row.held_by else {
    return Ok(());
  };
  Err(Fault::Conflict(format!(
    "Memory {id} is held by {holder:?}, which {what} may neither change nor close; only that holder's next \
     declaration or a withdrawal of its key ends it.",
    id = row.id
  )))
}

/// Whether the row already stands as the holder declares it: its words, its type, its weight, its conditions,
/// its labels and the subjects it is about. When it does, a declaration leaves it where it stands. A weight the
/// holder left out is asked for by the same default a row is written with, so a set declared twice answers that
/// nothing changed.
fn stands_as(declared: &DeclaredMemory, row: &Standing) -> bool {
  // The pairs as a set: the holder gives them in any order, and a row stands with each one once.
  let as_set = |pairs: Vec<(String, String)>| {
    let mut sorted = pairs;
    sorted.sort();
    sorted.dedup();
    sorted
  };
  let holds = as_set(row.subjects.clone());
  let asked = as_set(
    declared
      .subjects
      .iter()
      .map(|subject| (subject.kind.clone(), subject.key.clone()))
      .collect(),
  );

  declared.kind == row.kind
    && declared.text == row.text
    && declared.importance.unwrap_or(i64::from(DEFAULT_IMPORTANCE)) == i64::from(row.importance)
    && declared.recall_condition == row.recall_condition
    && declared.release_condition == row.release_condition
    && declared.labels == row.labels
    && asked == holds
}

/// The shape a declaration's body must answer before anything is stood: one holder key, its rows keyed once
/// each, and every field of every row within what a row takes.
fn declared_rule(body: &Declaration) -> Held<()> {
  key_rule("heldBy", &body.held_by)?;
  if body.memories.is_empty() || body.memories.len() > DECLARED_MOST {
    return Err(Fault::BadRequest(format!(
      "memories holds {}; a declaration holds 1 to {DECLARED_MOST}.",
      body.memories.len()
    )));
  }

  let mut named: BTreeSet<&str> = BTreeSet::new();
  for memory in &body.memories {
    key_rule("key", &memory.key)?;
    if !named.insert(memory.key.as_str()) {
      return Err(Fault::BadRequest(format!("memories names the key {:?} twice.", memory.key)));
    }
    written_type(&memory.kind)?;
    text_rule("text", &memory.text, TEXT_MOST)?;
    for (field, words) in [
      ("recallCondition", memory.recall_condition.as_deref()),
      ("releaseCondition", memory.release_condition.as_deref()),
    ] {
      if let Some(words) = words {
        text_rule(field, words, super::CONDITION_MOST)?;
      }
    }
    importance(memory.importance)?;
    if let Some(key) = &memory.actor_name {
      text_rule("actorName", key, TEXT_MOST)?;
    }
    for subject in &memory.subjects {
      key_rule("subjects.type", &subject.kind)?;
      key_rule("subjects.key", &subject.key)?;
    }
  }
  Ok(())
}

impl Store {
  /// A holder's whole set stood in every agent's brain: what stands as declared is left, what does not stand is
  /// written, and what the set no longer carries or stands otherwise is closed. The act is the same again where
  /// nothing differs.
  pub async fn declare(&self, body: &Declaration) -> Held<Declared> {
    declared_rule(body)?;

    let holder = body.held_by.as_str();
    let at = now();
    let mut asked: BTreeMap<&str, &DeclaredMemory> = BTreeMap::new();
    for memory in &body.memories {
      asked.insert(memory.key.as_str(), memory);
    }

    let mut tx = self.begin().await?;
    let brains: Vec<i64> = sqlx::query_scalar("SELECT id FROM brains WHERE kind = 'agent' ORDER BY id")
      .fetch_all(&mut *tx)
      .await?;
    if brains.is_empty() {
      return Err(Fault::Conflict(
        "No brain of the kind agent stands, so nothing holds a declaration; it is declared again once one does."
          .to_owned(),
      ));
    }

    let mut declared = Declared {
      held_by: body.held_by.clone(),
      brains: brains.len() as i64,
      ..Default::default()
    };
    for brain_id in brains {
      let standing = self.standing_of(&mut tx, brain_id, &body.held_by).await?;
      let mut asked: BTreeMap<&str, &DeclaredMemory> = BTreeMap::new();
      for memory in &body.memories {
        asked.insert(memory.key.as_str(), memory);
      }

      for (key, row) in &standing {
        let keep = asked.get(key.as_str()).is_some_and(|memory| stands_as(memory, row));
        if !keep {
          let reason = if asked.contains_key(key.as_str()) { HOLDER_REPLACED } else { HOLDER_WITHDRAWN };
          self.close_held(&mut tx, brain_id, row.id, reason, at).await?;
          declared.closed += 1;
        }
      }

      for memory in &body.memories {
        if standing.get(memory.key.as_str()).is_some_and(|row| stands_as(memory, row)) {
          continue;
        }
        let written = NewMemory {
          key: Some(memory.key.clone()),
          kind: memory.kind.clone(),
          text: memory.text.clone(),
          actor_name: memory.actor_name.clone().unwrap_or_else(|| holder.to_owned()),
          subjects: memory.subjects.clone(),
          importance: Some(i64::from(importance(memory.importance)?)),
          valid_from: memory.valid_from,
          recall_condition: memory.recall_condition.clone(),
          release_condition: memory.release_condition.clone(),
          labels: memory.labels.clone(),
          ..Default::default()
        };
        self.add_held_in(&mut tx, brain_id, &written, at, holder).await?;
        declared.written += 1;
      }
    }

    declared.memories = sqlx::query_scalar::<_, i64>(
      "SELECT COUNT(*) FROM memories WHERE held_by = $1 AND status IN ('active', 'pending')",
    )
    .bind(&body.held_by)
    .fetch_one(&mut *tx)
    .await?;
    self.commit(tx).await?;
    Ok(declared)
  }

  /// The holders with live rows, by key.
  pub async fn holders(&self, query: &HoldersQuery) -> Held<Page<Holder>> {
    let limit = super::limit(query.limit)?;
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
      "SELECT held_by, COUNT(DISTINCT brain_id)::BIGINT, COUNT(*)::BIGINT FROM memories \
       WHERE held_by IS NOT NULL AND status IN ('active', 'pending') \
         AND ($1::TEXT IS NULL OR held_by ILIKE $2) AND ($3::TEXT IS NULL OR held_by > $3) \
       GROUP BY held_by ORDER BY held_by LIMIT $4",
    )
    .bind(query.q.as_deref())
    .bind(query.q.as_deref().map(super::holding))
    .bind(query.after.as_deref())
    .bind(limit + 1)
    .fetch_all(&mut *self.connection().await?)
    .await?;
    let items = rows
      .into_iter()
      .map(|(held_by, brains, memories)| Holder {
        held_by,
        brains,
        memories,
      })
      .collect();
    Ok(super::page(items, limit))
  }

  /// What withdrawing one holder's key would close now, nothing changed.
  pub async fn holder_held(&self, holder: &str) -> Held<BrainsHeld> {
    key_rule("holderKey", holder)?;
    let (brains, memories): (i64, i64) = sqlx::query_as(
      "SELECT COUNT(DISTINCT brain_id)::BIGINT, COUNT(*)::BIGINT FROM memories \
       WHERE held_by = $1 AND status IN ('active', 'pending')",
    )
    .bind(holder)
    .fetch_one(&mut *self.connection().await?)
    .await?;
    Ok(BrainsHeld { brains, memories })
  }

  /// Every live row of one holder's key closed in every brain, each with its own `memory.moved` event, in one
  /// transaction; where nothing stands, nothing is done and zeros are answered.
  pub async fn holder_withdraw(&self, holder: &str) -> Held<BrainsHeld> {
    key_rule("holderKey", holder)?;
    let at = now();
    let mut tx = self.begin().await?;
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {} FROM memories WHERE held_by = $1 AND status IN ('active', 'pending') ORDER BY brain_id, id",
      super::versions::MEMORY_COLUMNS
    ))
    .bind(holder)
    .fetch_all(&mut *tx)
    .await?;
    let mut closed = BrainsHeld::default();
    let mut brains: BTreeSet<i64> = BTreeSet::new();
    for row in rows {
      self.close_held(&mut tx, row.brain_id, row.id, HOLDER_WITHDRAWN, at).await?;
      brains.insert(row.brain_id);
      closed.memories += 1;
    }
    closed.brains = brains.len() as i64;
    self.commit(tx).await?;
    Ok(closed)
  }

  /// The rows every standing declaration leaves, written into a brain as it is made, so an agent arriving after
  /// a holder declared knows what the holder left.
  pub(crate) async fn standing_rows_in(&self, tx: &mut PgConnection, brain_id: i64, at: i64) -> Held<()> {
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT DISTINCT ON (held_by, key) {} FROM memories \
       WHERE held_by IS NOT NULL AND status IN ('active', 'pending') ORDER BY held_by, key, id",
      super::versions::MEMORY_COLUMNS
    ))
    .fetch_all(&mut *tx)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let subjects = subjects_by_row(tx, &ids).await?;
    for row in rows {
      let Some(holder) = &row.held_by else {
        continue;
      };
      let body = NewMemory {
        key: row.key.clone(),
        kind: row.kind.clone(),
        text: row.text.clone(),
        subjects: subjects
          .get(&row.id)
          .cloned()
          .unwrap_or_default()
          .into_iter()
          .map(|subject| SubjectKey {
            kind: subject.kind,
            key: subject.key,
          })
          .collect(),
        importance: Some(i64::from(row.importance)),
        valid_from: Some(row.valid_from),
        recall_condition: row.recall_condition.clone(),
        release_condition: row.release_condition.clone(),
        labels: row.labels.0.clone(),
        actor_name: row.actor_name.clone(),
        ..Default::default()
      };
      self.add_held_in(tx, brain_id, &body, at, holder).await?;
    }
    Ok(())
  }

  /// The holder's live rows in one brain, by the key the holder gave each.
  async fn standing_of(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    holder: &str,
  ) -> Held<BTreeMap<String, Standing>> {
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {} FROM memories WHERE brain_id = $1 AND held_by = $2 AND status IN ('active', 'pending') \
       ORDER BY key, id",
      super::versions::MEMORY_COLUMNS
    ))
    .bind(brain_id)
    .bind(holder)
    .fetch_all(&mut *tx)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let mut subjects = subjects_by_row(tx, &ids).await?;
    let mut standing: BTreeMap<String, Standing> = BTreeMap::new();
    for row in rows {
      let Some(key) = row.key.clone() else {
        continue;
      };
      let mine: Vec<(String, String)> = subjects
        .remove(&row.id)
        .unwrap_or_default()
        .into_iter()
        .map(|subject: SubjectRef| (subject.kind, subject.key))
        .collect();
      standing.insert(
        key,
        Standing {
          id: row.id,
          kind: row.kind,
          text: row.text,
          importance: row.importance,
          recall_condition: row.recall_condition,
          release_condition: row.release_condition,
          labels: row.labels.0,
          subjects: mine,
        },
      );
    }
    Ok(standing)
  }

  /// One held row written, within a transaction: the first version's whole shape with the holder's key on it.
  async fn add_held_in(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    body: &NewMemory,
    at: i64,
    holder: &str,
  ) -> Held<()> {
    self.write_in(tx, brain_id, body, at, None, Some(holder)).await?;
    Ok(())
  }

  /// One held row closed: invalidated, holding until this moment, with its own `memory.moved` event.
  async fn close_held(
    &self,
    tx: &mut PgConnection,
    brain_id: i64,
    id: i64,
    reason: &str,
    at: i64,
  ) -> Held<()> {
    let closed: Option<MemoryRow> = sqlx::query_as::<_, MemoryRow>(&format!(
      "UPDATE memories SET status = 'invalidated', valid_to = $2, superseded_at = $2, reason = $3 \
       WHERE id = $1 AND status IN ('active', 'pending') RETURNING {}",
      super::versions::MEMORY_COLUMNS
    ))
    .bind(id)
    .bind(at)
    .bind(reason)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = closed else {
      return Ok(());
    };
    let ids = [row.id];
    let subjects = subjects_by_row(tx, &ids).await?;
    let member = row.member(subjects.get(&ids[0]).cloned().unwrap_or_default(), vec![])?;
    publish(tx, "memory.moved", Some(&event_scope(brain_id)), &member).await?;
    Ok(())
  }
}
