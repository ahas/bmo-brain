//! Org brains (docs/design.md, 「Org brains」): a unit's memory, holding no person, each memory carrying the
//! marks `locked` and `internalOnly`, written by the unit's heads and proposed by its members, and read along a
//! member's units as the topics that stand for them.

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{Audience, Brain, BrainKind, Memory, OrgTopic, OrgTopicsQuery, Standing};
use sqlx::PgConnection;

use super::brains::brain_of;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::{Fault, Held, Store, listed};

/// The subject type an org brain holds none of.
const PERSON: &str = "person";

/// The subject type an org memory overrides by.
const TOPIC: &str = "topic";

/// The type a member's proposal is written as.
pub(crate) const PROPOSAL: &str = "rumor";

/// The most org brains one reading names.
const UNITS_MOST: usize = 50;

/// The two marks of an org memory, as one version carries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Marks {
  pub locked: bool,
  pub internal_only: bool,
}

impl Marks {
  /// What a first version in an org brain carries where its writer says nothing: internal-only, not locked.
  pub(crate) const FIRST: Self = Self {
    locked: false,
    internal_only: true,
  };

  /// What every row of an agent's brain carries.
  const NONE: Self = Self {
    locked: false,
    internal_only: false,
  };

  /// The marks a row carries.
  pub(crate) fn of(row: &MemoryRow) -> Self {
    Self {
      locked: row.locked,
      internal_only: row.internal_only,
    }
  }

  /// The marks a write sets in this brain: each the one given, else the fallback's. An agent's brain takes
  /// neither, and a write that sets one there is refused.
  pub(crate) fn set(
    brain: &Brain,
    locked: Option<bool>,
    internal_only: Option<bool>,
    fallback: Self,
  ) -> Held<Self> {
    match brain.kind {
      BrainKind::Org => Ok(Self {
        locked: locked.unwrap_or(fallback.locked),
        internal_only: internal_only.unwrap_or(fallback.internal_only),
      }),
      BrainKind::Agent if locked == Some(true) || internal_only == Some(true) => {
        Err(Fault::BadRequest(format!(
          "Brain {} is an agent's brain; locked and internalOnly mark an org brain's memories alone.",
          brain.id
        )))
      }
      BrainKind::Agent => Ok(Self::NONE),
    }
  }

  /// The marks a row read from a brain file carries into this kind of brain: an agent's brain none, an org
  /// brain the file's, or a first version's where the file holds none.
  pub(crate) fn injected(kind: BrainKind, in_file: Option<Self>) -> Self {
    match kind {
      BrainKind::Agent => Self::NONE,
      BrainKind::Org => in_file.unwrap_or(Self::FIRST),
    }
  }
}

/// Whether a write by this standing in this brain is a member's proposal: in an org brain, any write but a
/// head's.
pub(crate) fn proposes(brain: &Brain, standing: Option<Standing>) -> bool {
  brain.kind == BrainKind::Org && standing.unwrap_or_default() == Standing::Member
}

/// A write an org brain takes from a head alone, refused to a member.
pub(crate) fn heads_only(brain: &Brain, standing: Option<Standing>, what: &str) -> Held<()> {
  if proposes(brain, standing) {
    return Err(Fault::Forbidden(format!(
      "Brain {} is an org brain, which takes a {what} from a head of its unit alone; a member proposes by a \
       first version or a learning.",
      brain.id
    )));
  }
  Ok(())
}

/// A subject of type `person` refused in an org brain, which holds no memory about a person.
pub(crate) async fn no_person_in(conn: &mut PgConnection, brain_id: i64, subject_type: &str) -> Held<()> {
  if subject_type != PERSON {
    return Ok(());
  }
  let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM brains WHERE id = $1")
    .bind(brain_id)
    .fetch_optional(&mut *conn)
    .await?;
  if kind.as_deref() == Some(BrainKind::Org.name()) {
    return Err(Fault::BadRequest(format!(
      "Brain {brain_id} is an org brain, which holds no memory about a person; no subject of type person \
       stands in it."
    )));
  }
  Ok(())
}

/// The org brains a reading names, in its order, or why they are refused.
fn units_named(value: &str) -> Held<Vec<i64>> {
  let mut ids = Vec::new();
  for id in listed(Some(value)) {
    let id: i64 = id
      .parse()
      .map_err(|_| Fault::BadRequest(format!("brainIds names {id:?}, which is no id.")))?;
    if ids.contains(&id) {
      return Err(Fault::BadRequest(format!("brainIds names brain {id} twice.")));
    }
    ids.push(id);
  }
  if ids.is_empty() || ids.len() > UNITS_MOST {
    return Err(Fault::BadRequest(format!(
      "brainIds names {} brains; it names 1 to {UNITS_MOST}.",
      ids.len()
    )));
  }
  Ok(ids)
}

/// One topic's standing rows in one brain, and whether any of them is locked.
struct TopicRows {
  rows: Vec<MemoryRow>,
  locked: bool,
}

impl Store {
  /// What stands for a member of an organisation: for each topic the org brains named hold, the standing
  /// memories of the one brain whose word on it holds, the first named holding a locked one, else the last
  /// holding one; read for an external audience, the internal-only ones left out after that.
  pub async fn org_topics(&self, query: &OrgTopicsQuery) -> Held<Vec<OrgTopic>> {
    let units = units_named(&query.brain_ids)?;
    let mut conn = self.connection().await?;
    for id in &units {
      let brain = brain_of(&mut conn, *id).await?;
      if brain.kind != BrainKind::Org {
        return Err(Fault::BadRequest(format!("brainIds names brain {id}, which is no org brain.")));
      }
    }
    let links: Vec<(i64, String)> = sqlx::query_as(
      "SELECT ms.memory_id, s.key FROM memory_subjects ms JOIN subjects s ON s.id = ms.subject_id \
       JOIN memories m ON m.id = ms.memory_id \
       WHERE m.brain_id = ANY($1) AND m.status = 'active' AND m.type NOT IN ('rumor', 'episode') AND s.type = $2",
    )
    .bind(&units)
    .bind(TOPIC)
    .fetch_all(&mut *conn)
    .await?;
    let ids: Vec<i64> = links.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>().into_iter().collect();
    let rows: Vec<MemoryRow> =
      sqlx::query_as(&format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id = ANY($1)"))
        .bind(&ids)
        .fetch_all(&mut *conn)
        .await?;
    let rows: BTreeMap<i64, MemoryRow> = rows.into_iter().map(|row| (row.id, row)).collect();

    // Each topic's rows, by the place their brain stands at among those named.
    let mut topics: BTreeMap<String, BTreeMap<usize, TopicRows>> = BTreeMap::new();
    for (id, topic) in &links {
      let Some(row) = rows.get(id) else { continue };
      let Some(place) = units.iter().position(|unit| *unit == row.brain_id) else { continue };
      let held = topics.entry(topic.clone()).or_default().entry(place).or_insert(TopicRows {
        rows: vec![],
        locked: false,
      });
      held.locked |= row.locked;
      held.rows.push(row.clone());
    }

    let external = query.audience.unwrap_or_default() == Audience::External;
    let mut standing: Vec<(usize, String, bool, Vec<MemoryRow>)> = Vec::new();
    for (topic, mut by_place) in topics {
      // The farthest holding a locked row of the topic, else the nearest holding one.
      let holder = by_place
        .iter()
        .find(|(_, held)| held.locked)
        .or_else(|| by_place.iter().next_back())
        .map(|(place, _)| *place);
      let Some(held) = holder.and_then(|place| by_place.remove(&place).map(|held| (place, held))) else {
        continue;
      };
      let (place, held) = held;
      let mut kept: Vec<MemoryRow> =
        held.rows.into_iter().filter(|row| !(external && row.internal_only)).collect();
      if kept.is_empty() {
        continue;
      }
      kept.sort_by(|a, b| {
        b.importance
          .cmp(&a.importance)
          .then(b.valid_from.cmp(&a.valid_from))
          .then(b.id.cmp(&a.id))
      });
      standing.push((place, topic, held.locked, kept));
    }
    standing.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut answered: BTreeMap<i64, MemoryRow> = BTreeMap::new();
    for (.., kept) in &standing {
      for row in kept {
        answered.entry(row.id).or_insert_with(|| row.clone());
      }
    }
    let members: BTreeMap<i64, Memory> = self
      .members(&mut conn, answered.into_values().collect())
      .await?
      .into_iter()
      .map(|member| (member.id, member))
      .collect();
    Ok(
      standing
        .into_iter()
        .map(|(place, topic, locked, kept)| OrgTopic {
          topic,
          brain_id: units[place],
          locked,
          memories: kept.iter().filter_map(|row| members.get(&row.id).cloned()).collect(),
        })
        .collect(),
    )
  }
}
