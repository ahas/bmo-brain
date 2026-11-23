//! Org brains (docs/design.md, 「Org brains」): a unit's memory, holding a person only as the organisation may
//! know them, each memory carrying the marks `locked`, `internalOnly` and `shared`, written by the unit's heads
//! and by the person a shared memory is about, proposed by its members, and read along a member's units as the
//! topics that stand for them.

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{
  Audience, Brain, BrainKind, Memory, OrgTopic, OrgTopicsQuery, Standing, SubjectKey,
};
use sqlx::PgConnection;

use super::brains::brain_of;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::{Fault, Held, Store, listed};

/// The subject type a shared memory of an org brain is about.
pub(crate) const PERSON: &str = "person";

/// The subject type an org memory overrides by.
const TOPIC: &str = "topic";

/// The type a member's proposal is written as.
pub(crate) const PROPOSAL: &str = "rumor";

/// The most org brains one reading names.
const UNITS_MOST: usize = 50;

/// The three marks of an org memory, as one version carries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Marks {
  pub locked: bool,
  pub internal_only: bool,
  pub shared: bool,
}

impl Marks {
  /// What a first version in an org brain carries where its writer says nothing: internal-only, not locked,
  /// not shared.
  pub(crate) const FIRST: Self = Self {
    locked: false,
    internal_only: true,
    shared: false,
  };

  /// What every row of an agent's brain carries.
  const NONE: Self = Self {
    locked: false,
    internal_only: false,
    shared: false,
  };

  /// The marks a row carries.
  pub(crate) fn of(row: &MemoryRow) -> Self {
    Self {
      locked: row.locked,
      internal_only: row.internal_only,
      shared: row.shared,
    }
  }

  /// The marks a write sets in this brain: each the one given, else the fallback's. An agent's brain takes
  /// none, and a write that sets one there is refused.
  pub(crate) fn set(brain: &Brain, given: Given, fallback: Self) -> Held<Self> {
    match brain.kind {
      BrainKind::Org => Ok(Self {
        locked: given.locked.unwrap_or(fallback.locked),
        internal_only: given.internal_only.unwrap_or(fallback.internal_only),
        shared: given.shared.unwrap_or(fallback.shared),
      }),
      BrainKind::Agent
        if given.locked == Some(true)
          || given.internal_only == Some(true)
          || given.shared == Some(true) =>
      {
        Err(Fault::BadRequest(format!(
          "Brain {} is an agent's brain; locked, internalOnly and shared mark an org brain's memories alone.",
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

/// The marks a write names, each none where it names none.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Given {
  pub locked: Option<bool>,
  pub internal_only: Option<bool>,
  pub shared: Option<bool>,
}

/// What a memory is about as the person rule reads it: the person subjects it links, by id, and whether it is
/// shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct About {
  pub people: Vec<i64>,
  pub shared: bool,
}

impl About {
  /// The one person a shared memory is about alone, or none.
  fn one_person(&self) -> Option<i64> {
    match self.people.as_slice() {
      [person] if self.shared => Some(*person),
      _ => None,
    }
  }
}

/// The standing a write is judged by: the person a memory is about stands as a head where every memory the write
/// touches is shared and about that one person alone, and as a member otherwise; every other standing as it is.
pub(crate) fn judged(standing: Option<Standing>, touched: &[&About]) -> Option<Standing> {
  if standing != Some(Standing::Person) {
    return standing;
  }
  let people: Option<BTreeSet<i64>> = touched.iter().map(|about| about.one_person()).collect();
  match people {
    Some(people) if people.len() == 1 => Some(Standing::Head),
    _ => Some(Standing::Member),
  }
}

/// The standing a learning is judged by, from the subjects it names by key before they are made: the person a
/// memory is about stands as a head for a shared memory naming that one person alone, and as a member otherwise.
pub(crate) fn judged_by_keys(
  standing: Option<Standing>,
  subjects: &[SubjectKey],
  shared: Option<bool>,
) -> Option<Standing> {
  if standing != Some(Standing::Person) {
    return standing;
  }
  let people: BTreeSet<&str> = subjects
    .iter()
    .filter(|subject| subject.kind == PERSON)
    .map(|subject| subject.key.as_str())
    .collect();
  if shared == Some(true) && people.len() == 1 {
    Some(Standing::Head)
  } else {
    Some(Standing::Member)
  }
}

/// The person subjects among these subjects, by id.
pub(crate) async fn people_among(conn: &mut PgConnection, subject_ids: &[i64]) -> Held<Vec<i64>> {
  if subject_ids.is_empty() {
    return Ok(vec![]);
  }
  Ok(
    sqlx::query_scalar("SELECT id FROM subjects WHERE id = ANY($1) AND type = $2 ORDER BY id")
      .bind(subject_ids)
      .bind(PERSON)
      .fetch_all(&mut *conn)
      .await?,
  )
}

/// What a row stands about: the person subjects it links and its `shared` mark.
pub(crate) async fn about_row(conn: &mut PgConnection, row: &MemoryRow) -> Held<About> {
  let people: Vec<i64> = sqlx::query_scalar(
    "SELECT s.id FROM memory_subjects ms JOIN subjects s ON s.id = ms.subject_id \
     WHERE ms.memory_id = $1 AND s.type = $2 ORDER BY s.id",
  )
  .bind(row.id)
  .bind(PERSON)
  .fetch_all(&mut *conn)
  .await?;
  Ok(About {
    people,
    shared: row.shared,
  })
}

/// The person rule of an org brain: a memory about a person is shared, and a shared memory is about a person.
/// An agent's brain holds people freely and takes no mark.
pub(crate) fn person_rule(brain: &Brain, about: &About) -> Held<()> {
  if brain.kind != BrainKind::Org {
    return Ok(());
  }
  match (about.people.is_empty(), about.shared) {
    (false, false) => Err(Fault::BadRequest(format!(
      "Brain {} is an org brain, which holds a memory about a person only where it is shared: what the \
       organisation may know of them.",
      brain.id
    ))),
    (true, true) => Err(Fault::BadRequest(format!(
      "Brain {}: shared marks a memory about a person, and this memory links none.",
      brain.id
    ))),
    _ => Ok(()),
  }
}

/// Whether a write by this standing in this brain is a member's proposal: in an org brain, any write but a
/// head's. A person's standing is judged first (`judged`), and one left unjudged proposes.
pub(crate) fn proposes(brain: &Brain, standing: Option<Standing>) -> bool {
  brain.kind == BrainKind::Org && standing.unwrap_or_default() != Standing::Head
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
