//! The base (docs/design.md, 「The base」 and 「Raising a memory to the base」): the org brain keyed by the
//! platform, found or made, and the candidates of it read from a source brain, sifted by their subjects and
//! written into it as pending rows the administrator decides.

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{
  BrainKind, Labels, Memory, MemoryStatus, NewBrain, NewMemory, Standing, SubjectKey,
};

use super::brains::brain_of;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::{Fault, Held, Store, now};

/// The account key and the agent key the base is filed under.
pub const BASE_KEY: &str = "platform";

/// The name the base is made with.
pub const BASE_NAME: &str = "base";

/// The types a candidate is drawn from.
pub const CANDIDATE_TYPES: [&str; 3] = ["tip", "check", "knowledge"];

/// Who writes a candidate.
pub const REVIEWER: &str = "base-review";

/// The labels a candidate carries: its source's brain, memory and lineage, and the model that judged it with the
/// words it gave.
pub const SOURCE_BRAIN_LABEL: &str = "sourceBrainId";
pub const SOURCE_MEMORY_LABEL: &str = "sourceMemoryId";
pub const SOURCE_LINEAGE_LABEL: &str = "sourceLineageId";
pub const MODEL_LABEL: &str = "candidateModel";
pub const REASON_LABEL: &str = "candidateReason";

/// The subject types no candidate is about, whatever the command is told.
const NEVER_ABOUT: [&str; 2] = ["person", "room"];

/// The subject type of a learning's grounds, which a candidate does not carry into the base.
const EPISODE: &str = "episode";

/// What a sifting passes over beside a person and a room: the subject types, and the beginnings of subject keys,
/// case set aside.
#[derive(Clone, Debug, Default)]
pub struct PassOver {
  pub types: Vec<String>,
  pub key_prefixes: Vec<String>,
}

/// Why a memory was passed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Passed {
  PersonOrRoom,
  Type,
  Key,
  Raised,
}

/// What a sifting of a source brain came to: how many memories it read, how many it passed over and why, the
/// subject types that passed them over, and the memories left for judgement, oldest first.
#[derive(Clone, Debug, Default)]
pub struct Sifted {
  pub considered: usize,
  pub person_or_room: usize,
  pub by_type: usize,
  pub by_key: usize,
  pub raised: usize,
  pub passed_over: BTreeMap<String, usize>,
  pub left: Vec<Memory>,
}

impl PassOver {
  /// Why a memory about these subjects is passed over, and the subject type that passed it, or none.
  fn passes(&self, memory: &Memory) -> Option<(Passed, String)> {
    let prefixes: Vec<String> = self.key_prefixes.iter().map(|prefix| prefix.to_lowercase()).collect();
    for subject in &memory.subjects {
      if NEVER_ABOUT.contains(&subject.kind.as_str()) {
        return Some((Passed::PersonOrRoom, subject.kind.clone()));
      }
    }
    for subject in &memory.subjects {
      if self.types.iter().any(|kind| kind == &subject.kind) {
        return Some((Passed::Type, subject.kind.clone()));
      }
    }
    for subject in &memory.subjects {
      let key = subject.key.to_lowercase();
      if prefixes.iter().any(|prefix| key.starts_with(prefix.as_str())) {
        return Some((Passed::Key, subject.kind.clone()));
      }
    }
    None
  }
}

impl Store {
  /// The base's id, where it stands.
  pub async fn base_id(&self) -> Held<Option<i64>> {
    let mut conn = self.connection().await?;
    Ok(
      sqlx::query_scalar("SELECT id FROM brains WHERE kind = $1 AND account_key = $2 AND agent_key = $2")
        .bind(BrainKind::Org.name())
        .bind(BASE_KEY)
        .fetch_optional(&mut *conn)
        .await?,
    )
  }

  /// The base's id, the base made where none stands.
  pub async fn base_made(&self) -> Held<i64> {
    if let Some(id) = self.base_id().await? {
      return Ok(id);
    }
    let made = self
      .brain_add(&NewBrain {
        account_key: BASE_KEY.to_owned(),
        agent_key: BASE_KEY.to_owned(),
        kind: Some(BrainKind::Org),
        name: Some(BASE_NAME.to_owned()),
      })
      .await?;
    Ok(made.id)
  }

  /// The source brain's live candidate memories, oldest first and at most `limit` of them, each passed over
  /// where a subject or a raised candidate says so, and the rest left for judgement.
  pub async fn base_sift(&self, from: i64, pass_over: &PassOver, limit: Option<usize>) -> Held<Sifted> {
    let mut conn = self.connection().await?;
    let source = brain_of(&mut conn, from).await?;
    if source.kind != BrainKind::Agent {
      return Err(Fault::BadRequest(format!(
        "Brain {from} is no agent's brain; the base is raised from one."
      )));
    }
    let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND status = 'active' AND type = ANY($2) \
       ORDER BY id"
    ))
    .bind(from)
    .bind(CANDIDATE_TYPES.map(str::to_owned).to_vec())
    .fetch_all(&mut *conn)
    .await?;
    let rows: Vec<MemoryRow> = match limit {
      Some(most) => rows.into_iter().take(most).collect(),
      None => rows,
    };
    let raised: BTreeSet<String> = sqlx::query_scalar(
      "SELECT m.labels->>$2 FROM memories m JOIN brains b ON b.id = m.brain_id \
       WHERE b.kind = $3 AND b.account_key = $4 AND b.agent_key = $4 AND m.labels->>$1 = $5 \
       AND m.labels ? $2",
    )
    .bind(SOURCE_BRAIN_LABEL)
    .bind(SOURCE_LINEAGE_LABEL)
    .bind(BrainKind::Org.name())
    .bind(BASE_KEY)
    .bind(from.to_string())
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect::<Vec<Option<String>>>()
    .into_iter()
    .flatten()
    .collect();
    let memories = self.members(&mut conn, rows).await?;
    let mut sifted = Sifted {
      considered: memories.len(),
      ..Sifted::default()
    };
    for memory in memories {
      let passed = pass_over.passes(&memory).or_else(|| {
        raised.contains(&memory.lineage_id.to_string()).then(|| (Passed::Raised, String::new()))
      });
      match passed {
        None => sifted.left.push(memory),
        Some((why, kind)) => {
          match why {
            Passed::PersonOrRoom => sifted.person_or_room += 1,
            Passed::Type => sifted.by_type += 1,
            Passed::Key => sifted.by_key += 1,
            Passed::Raised => sifted.raised += 1,
          }
          if why != Passed::Raised {
            *sifted.passed_over.entry(kind).or_default() += 1;
          }
        }
      }
    }
    Ok(sifted)
  }

  /// One candidate written into the base as a pending row: its source's type, words, conditions, importance and
  /// subjects but its grounds, labelled with its source and its judgement, as the administrator's word.
  pub async fn base_raise(&self, base: i64, source: &Memory, model: &str, reason: &str) -> Held<Memory> {
    let mut labels = Labels::new();
    labels.insert(SOURCE_BRAIN_LABEL.to_owned(), source.brain_id.to_string());
    labels.insert(SOURCE_MEMORY_LABEL.to_owned(), source.id.to_string());
    labels.insert(SOURCE_LINEAGE_LABEL.to_owned(), source.lineage_id.to_string());
    labels.insert(MODEL_LABEL.to_owned(), model.to_owned());
    if !reason.trim().is_empty() {
      labels.insert(REASON_LABEL.to_owned(), reason.trim().to_owned());
    }
    let subjects: Vec<SubjectKey> = source
      .subjects
      .iter()
      .filter(|subject| subject.kind != EPISODE)
      .map(|subject| SubjectKey {
        kind: subject.kind.clone(),
        key: subject.key.clone(),
      })
      .collect();
    let mut tx = self.begin().await?;
    let memory = self
      .add_in(
        &mut tx,
        base,
        &NewMemory {
          key: None,
          kind: source.kind.clone(),
          text: source.text.clone(),
          layer_id: None,
          position: None,
          subject_ids: vec![],
          subjects,
          importance: Some(source.importance),
          valid_from: Some(source.valid_from),
          recall_condition: source.recall_condition.clone(),
          release_condition: source.release_condition.clone(),
          status: Some(MemoryStatus::Pending),
          actor_name: REVIEWER.to_owned(),
          labels,
          locked: Some(false),
          internal_only: Some(false),
          shared: Some(false),
          standing: Some(Standing::Head),
        },
        now(),
      )
      .await?;
    self.commit(tx).await?;
    Ok(memory)
  }
}
