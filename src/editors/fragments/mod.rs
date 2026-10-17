//! What an editor batch and a shipped library's owner ask of the fragment table (docs/fragments.md, 「An editor
//! writes the row through the fragment tool」, 「Finding a piece before writing one」 and 「The reconcile」): the
//! pieces its writer is handed whole, its row writes held against what it wrote on disk, which of them reach the
//! rows every room reads, and — once the batch stands — its writes applied, the table brought up to date and the
//! index files drawn out of it; and a shipped library's rows brought to the entries its owner hands in.

pub mod libraries;
pub mod narrow;
pub mod store;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;

use brain_support::database::Fault;
use fragment_desk::desk::{FragmentFilter, FragmentWrite, Names};
use fragment_desk::meta::{
  body_facts, body_hash, item_bracket_problem, keyword_problem, title_shape_problem,
};
use sea_orm::{DatabaseConnection, TransactionTrait};

use self::libraries::{Brain, LIBRARIES, Said, brain_kind};
use self::narrow::{Turn, rows_to_hand};
use self::store::{NewRow, Row, ShippedEntry, now_seconds, of_kind};
use crate::Failure;
use crate::policy::COMMON_WORDS;
use crate::ports::agency::Agency;
use crate::ports::job::Job;
use crate::ports::receiver::Receiver;

/// The fragment table and the brain it follows, as one batch stands on them.
pub struct Table<'a> {
  pub db: &'a DatabaseConnection,
  pub agency: &'a dyn Agency,
  pub receiver: &'a dyn Receiver,
  /// Whether the items a piece is tied to are finished.
  pub job: &'a dyn Job,
  pub brain: Brain,
  /// The names the write checks read.
  pub names: Names,
}

/// One batch's runs' writing, as far as their row writes are held against it.
pub struct Writing<'a> {
  /// The row writes the runs made through the fragment tool.
  pub writes: &'a [FragmentWrite],
  /// The brain paths the runs created or changed.
  pub changed: &'a [String],
  /// Whether a path is one the runs were allowed to write.
  pub allowed: &'a dyn Fn(&str) -> bool,
  /// What a path holds on disk now, relative to the brain, or none where nothing stands there.
  pub body: &'a dyn Fn(&str) -> Option<String>,
  /// Where a path will stand once the batch is kept, for a run that writes beside the book rather than into it.
  pub seen: &'a dyn Fn(&str) -> String,
}

/// What one library's reconcile did, or why the library could not be read at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciled {
  pub kind: String,
  /// What the library now holds, which is what the rows were brought to.
  pub entries: u64,
  pub sync: store::Sync,
  /// How many rows had their body read again, and how many had their status settled otherwise.
  pub reread: u64,
  pub settled: u64,
  /// Why the library was passed over, where it was. Its rows are left exactly as they stand.
  pub trouble: Option<String>,
}

impl Reconciled {
  /// What the reconcile did as one line of a reading.
  pub fn line(&self) -> String {
    match &self.trouble {
      Some(trouble) => format!("{}: passed over — {trouble}", self.kind),
      None => format!(
        "{}: {} entries · {} added · {} updated · {} raised · {} taken down · {} bodies read · {} settled",
        self.kind,
        self.entries,
        self.sync.added,
        self.sync.updated,
        self.sync.revived,
        self.sync.retired,
        self.reread,
        self.settled
      ),
    }
  }
}

/// The word a screen calls every kind by: the brain's own three as each library answers it, then every shipped
/// kind as its owner last handed it in, by kind.
pub async fn kind_labels(db: &DatabaseConnection) -> Result<Vec<(String, String)>, Failure> {
  let mut labels: Vec<(String, String)> = LIBRARIES
    .iter()
    .map(|library| (library.kind().to_owned(), library.label().to_owned()))
    .collect();

  labels.extend(store::kind_labels(db).await?);

  Ok(labels)
}

/// A piece a writer is handed whole before it writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandedPiece {
  pub path: String,
  pub text: String,
}

/// What a batch's text is narrowed by: its words and who spoke in it.
pub struct BatchText<'a> {
  pub text: &'a str,
  pub speakers: &'a [i64],
}

/// Why a shipped library's entries were not taken: the kind is one of the brain's own libraries, whose rows only
/// its editors write, or the brain's own work stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShippedFailure {
  BrainLibrary,
  Failed(Failure),
}

impl fmt::Display for ShippedFailure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::BrainLibrary => f.write_str("the kind is one of the brain's own libraries"),
      Self::Failed(failure) => failure.fmt(f),
    }
  }
}

impl std::error::Error for ShippedFailure {}

impl From<Failure> for ShippedFailure {
  fn from(failure: Failure) -> Self {
    Self::Failed(failure)
  }
}

impl From<Fault> for ShippedFailure {
  fn from(fault: Fault) -> Self {
    Self::Failed(fault.into())
  }
}

/// The names the write checks read: the name the brain root's agent calls itself by, its registration key, and
/// the common words: the policy's, the chat agent's key and label, and the owner's names the configuration holds.
pub fn names(brain: &Brain, chat_key: &str, owner_names: &[String]) -> Names {
  let agent = brain.writer_name(brain.root_agent());
  let named = [chat_key, agent.as_str()].into_iter().chain(owner_names.iter().map(String::as_str));

  Names {
    common_words: COMMON_WORDS.with_names(named),
    agent,
    agent_key: chat_key.to_owned(),
  }
}

impl<'a> Table<'a> {
  /// The table on a database, with every awake agent's brain folder and label as the agency registers them;
  /// `chat_key` is the chat agent's registration key, whose memory the brain root is, and `owner_names` the names
  /// the owner goes by, which the checks count among the common words.
  pub async fn of(
    db: &'a DatabaseConnection,
    agency: &'a dyn Agency,
    receiver: &'a dyn Receiver,
    job: &'a dyn Job,
    brain_root: &Path,
    chat_key: &str,
    owner_names: &[String],
  ) -> Result<Self, Failure> {
    let brain = Brain::of(agency, brain_root, chat_key).await?;
    let names = names(&brain, chat_key, owner_names);

    Ok(Self {
      db,
      agency,
      receiver,
      job,
      brain,
      names,
    })
  }

  /// The rows a filter asks for.
  pub async fn rows(&self, filter: &FragmentFilter) -> Result<Vec<Row>, Failure> {
    Ok(store::rows(self.db, filter).await?)
  }

  // A body as it stands under the brain, none where no file is at the row's path.
  fn written_body(&self, path: &str) -> Option<String> {
    fs::read_to_string(self.brain.locate(path)).ok()
  }

  // The names the receiver knows each of these people by, none for a person it names nothing.
  async fn display_names(
    &self,
    people: impl IntoIterator<Item = i64>,
  ) -> Result<HashMap<i64, Option<String>>, Failure> {
    let people: Vec<i64> = people.into_iter().collect::<HashSet<_>>().into_iter().collect();
    let mut names: HashMap<i64, Option<String>> = people.iter().map(|person| (*person, None)).collect();

    if people.is_empty() {
      return Ok(names);
    }

    for named in self.receiver.display_names(&people).await? {
      names.insert(named.person, named.name);
    }

    Ok(names)
  }

  /// The pieces the table hands a writer about to write: whole, the rows the batch names by an item or a word
  /// together with the ones the caller always hands over; a row whose body is not on disk is left out.
  pub async fn pieces_for_writer(
    &self,
    rows: &[Row],
    batch: &BatchText<'_>,
    always: impl Fn(&Row) -> bool,
  ) -> Result<Vec<HandedPiece>, Failure> {
    let mut bodies = HashMap::new();
    let mut held = vec![];

    for row in rows {
      if let Some(body) = brain_kind(&row.kind).then(|| self.written_body(&row.path)).flatten() {
        bodies.insert(row.id, body);
        held.push(row.clone());
      }
    }

    let known = self
      .display_names(held.iter().filter_map(|row| row.owner))
      .await?
      .into_iter()
      .map(|(person, name)| (person, name.into_iter().collect()))
      .collect();
    let turn = Turn {
      text: batch.text.to_owned(),
      speakers: batch.speakers.to_vec(),
      names: known,
    };
    let always_ids: HashSet<i64> = held.iter().filter(|row| always(row)).map(|row| row.id).collect();

    Ok(
      rows_to_hand(&held, &turn, |row| always_ids.contains(&row.id))
        .into_iter()
        .map(|row| HandedPiece {
          text: bodies.remove(&row.id).unwrap_or_default(),
          path: row.path,
        })
        .collect(),
    )
  }

  /// Why a run's row writes do not stand, or nothing where they do: a write naming no piece or a piece the run
  /// may not write, a write for a piece not on disk and not marked removed, a piece whose day brackets do not name
  /// their slots, a title or keywords that do not say what the piece is, and a piece the run created or changed
  /// with no write of its own.
  pub fn write_problems(&self, writing: &Writing<'_>) -> Vec<String> {
    write_problems(&self.brain, writing, &self.names)
  }

  // The row standing at a path of a library, up before down.
  fn row_at<'r>(rows: &'r [Row], path: &str) -> Option<&'r Row> {
    rows
      .iter()
      .find(|row| row.path == path && row.active)
      .or_else(|| rows.iter().find(|row| row.path == path))
  }

  /// The paths of shared rows a batch's writes make, change or take down, which only a batch of the owner's own
  /// notes may do. Asked of the table before the writes are applied.
  pub async fn shared_write_paths(
    &self,
    writes: &[FragmentWrite],
    room: Option<i64>,
  ) -> Result<Vec<String>, Failure> {
    let mut shared = vec![];

    for write in writes {
      let Some(library) = self.brain.library_owning(&write.path) else { continue };
      let rows = self.rows(&of_kind(library.kind())).await?;
      let standing = Self::row_at(&rows, &write.path);
      let place = if write.removed {
        None
      } else {
        self.brain.place(
          library,
          &write.path,
          &Said {
            shared: write.shared,
            room,
            standing,
          },
        )
      };
      let was_shared =
        standing.is_some_and(|row| row.active && row.chat_id.is_none() && row.owner.is_none());
      let is_shared = place.is_some_and(|place| place.chat_id.is_none() && place.owner.is_none());

      if was_shared || is_shared {
        shared.push(write.path.clone());
      }
    }

    Ok(shared)
  }

  /// The rows a batch's writes make once the batch stands: a write for a piece writes its row — made where none
  /// stands, and at the end of its index — and a piece whose file is gone goes down whether or not its write says
  /// so. A write that moves a piece to another scope takes the row it stood under down. The last write naming a
  /// path is the one that stands.
  pub async fn apply_writes(
    &self,
    writes: &[FragmentWrite],
    room: Option<i64>,
  ) -> Result<Vec<Row>, Failure> {
    let mut last: Vec<&FragmentWrite> = vec![];

    for write in writes {
      match last.iter_mut().find(|held| held.path == write.path) {
        Some(held) => *held = write,
        None => last.push(write),
      }
    }

    let held = self.db.begin().await?;
    let mut standing_rows: HashMap<&'static str, Vec<Row>> = HashMap::new();
    let mut written = vec![];
    let now = now_seconds();

    for write in last {
      let Some(library) = self.brain.library_owning(&write.path) else { continue };

      if !standing_rows.contains_key(library.kind()) {
        standing_rows.insert(library.kind(), store::rows(&held, &of_kind(library.kind())).await?);
      }

      let rows = standing_rows.get_mut(library.kind()).expect("read above");
      let standing = Self::row_at(rows, &write.path).cloned();
      let body = self.written_body(&write.path);
      let keep = |rows: &mut Vec<Row>, row: Row| {
        rows.retain(|one| one.id != row.id);
        rows.push(row);
      };

      let Some(body) = body.filter(|_| !write.removed) else {
        if let Some(standing) = standing {
          store::retire_row(&held, standing.id).await?;
          keep(
            rows,
            Row {
              active: standing.hand == Some(true),
              ..standing
            },
          );
        }

        continue;
      };
      let Some(place) = self.brain.place(
        library,
        &write.path,
        &Said {
          shared: write.shared,
          room,
          standing: standing.as_ref(),
        },
      ) else {
        continue;
      };
      let agent = place.agent.clone().unwrap_or_else(|| self.brain.root_agent().to_owned());
      let moved = standing.as_ref().is_some_and(|row| {
        row.chat_id != place.chat_id
          || row.owner != place.owner
          || row.slug != place.slug
          || row.agent != agent
      });

      if moved && let Some(standing) = &standing {
        store::retire_row(&held, standing.id).await?;
        keep(
          rows,
          Row {
            active: standing.hand == Some(true),
            ..standing.clone()
          },
        );
      }

      let index = self.brain.index_of(library, place.chat_id, place.owner, &agent);
      let position = match &standing {
        Some(row) if !moved => row.position,
        _ => {
          rows
            .iter()
            .filter(|row| self.brain.index_of(library, row.chat_id, row.owner, &row.agent) == index)
            .map(|row| row.position)
            .fold(-1, i64::max)
            + 1
        }
      };
      let row = store::write_row(
        &held,
        &NewRow {
          kind: library.kind().to_owned(),
          chat_id: place.chat_id,
          owner: place.owner,
          agent,
          all_agents: write.all_agents,
          slug: place.slug,
          title: write.title.clone(),
          path: write.path.clone(),
          scope: place.scope,
          position,
          keywords: write.keywords.clone(),
          items: write.items.clone(),
        },
        now,
      )
      .await?;
      let facts = body_facts(&body);
      let hash = body_hash(&body);
      let items = store::note_body(&held, row.id, &hash, &facts).await?;
      let stands = Row {
        body_hash: Some(hash),
        written_on: facts.written_on.clone(),
        confirmed_on: facts.confirmed_on.clone(),
        items,
        ..row
      };

      keep(rows, stands.clone());
      written.push(stands);
    }

    held.commit().await?;

    Ok(written)
  }

  // The rows of a library one reconcile or drawing answers for: of a brain library the named agent's alone, and
  // every row where no agent is named or the library is a shipped one.
  fn answered(kind: &str, written: bool, agent: Option<&str>) -> FragmentFilter {
    FragmentFilter {
      kind: Some(kind.to_owned()),
      agent: agent.filter(|_| written).map(str::to_owned),
      ..FragmentFilter::default()
    }
  }

  // Every row of a brain library has what its body says about itself read again where the body changed, and a
  // row whose body is gone goes down. Answers how many were read again and how many went down.
  async fn reread(&self, kind: &str, agent: Option<&str>) -> Result<(u64, u64), Failure> {
    let (mut reread, mut retired) = (0, 0);

    for row in self.rows(&Self::answered(kind, true, agent)).await? {
      let Ok(body) = fs::read_to_string(self.brain.locate(&row.path)) else {
        if row.active {
          store::retire_row(self.db, row.id).await?;
          retired += 1;
        }

        continue;
      };
      let hash = body_hash(&body);

      if row.body_hash.as_deref() != Some(hash.as_str()) {
        store::note_body(self.db, row.id, &hash, &body_facts(&body)).await?;
        reread += 1;
      }
    }

    Ok((reread, retired))
  }

  // Whether each row's matter is finished, as the agency's items now say: a row of a library whose pieces settle
  // is done when it is tied to items and every one of them is finished, and every other row is open.
  async fn settle(
    &self,
    kind: &str,
    settles: bool,
    written: bool,
    agent: Option<&str>,
  ) -> Result<u64, Failure> {
    let rows = self.rows(&Self::answered(kind, written, agent)).await?;
    let items: Vec<i64> = rows
      .iter()
      .flat_map(|row| row.items.iter().copied())
      .collect::<HashSet<_>>()
      .into_iter()
      .collect();
    let finished: HashSet<i64> = if settles && !items.is_empty() {
      self
        .job
        .item_states(&items)
        .await?
        .into_iter()
        .filter(|state| state.done)
        .map(|state| state.id)
        .collect()
    } else {
      HashSet::new()
    };
    let statuses = rows
      .iter()
      .map(|row| {
        let done = settles && !row.items.is_empty() && row.items.iter().all(|item| finished.contains(item));

        (row.id, if done { "done" } else { "open" })
      })
      .collect();

    Ok(store::settle_status(self.db, &statuses).await?)
  }

  /// The brain libraries' rows brought up to date with what stands under the brain, one library at a time: every
  /// row's body read again where it changed, a row whose body is gone taken down, and every row's status settled.
  /// A brain that cannot be read as the brain is passed over and its rows left as they stand. Where an agent is
  /// named, only that agent's rows are reached. Answers what each library's reconcile did, in the order they are
  /// reconciled.
  pub async fn reconcile(&self, agent: Option<&str>) -> Result<Vec<Reconciled>, Failure> {
    let mut done = vec![];

    for library in LIBRARIES {
      let kind = library.kind().to_owned();

      if let Some(fault) = self.brain.fault() {
        eprintln!("fragments: the {kind} library is passed over: {fault}");
        done.push(Reconciled {
          kind,
          trouble: Some(fault),
          ..Reconciled::default()
        });
        continue;
      }

      let (reread, retired) = self.reread(&kind, agent).await?;
      let settled = self.settle(&kind, library.settles(), true, agent).await?;
      let entries = self
        .rows(&FragmentFilter {
          active: Some(true),
          ..Self::answered(&kind, true, agent)
        })
        .await?
        .len() as u64;

      done.push(Reconciled {
        kind,
        entries,
        sync: store::Sync {
          retired,
          ..store::Sync::default()
        },
        reread,
        settled,
        trouble: None,
      });
    }

    Ok(done)
  }

  /// A shipped library's rows, the chat agent's, brought to the entries its owner hands in, and every one of its
  /// rows settled open: a shipped piece is never finished. An empty list takes the library's rows down. The word a
  /// screen calls the kind by is kept as handed in. A kind that is one of the brain's own libraries is refused.
  pub async fn reconcile_shipped(
    &self,
    kind: &str,
    label: &str,
    entries: &[ShippedEntry],
  ) -> Result<Reconciled, ShippedFailure> {
    if brain_kind(kind) {
      return Err(ShippedFailure::BrainLibrary);
    }

    let now = now_seconds();
    let (sync, reread) = store::set_rows(self.db, kind, self.brain.root_agent(), entries, now).await?;

    store::set_kind_label(self.db, kind, label, now).await?;

    let settled = self.settle(kind, false, false, None).await?;

    Ok(Reconciled {
      kind: kind.to_owned(),
      entries: entries.len() as u64,
      sync,
      reread,
      settled,
      trouble: None,
    })
  }

  /// Each brain library's index files drawn out of its rows and written where their text changed; where an agent
  /// is named, only the index files in that agent's folder, out of its own rows. Answers the paths written,
  /// relative to the brain root.
  pub async fn draw_indexes(&self, agent: Option<&str>) -> Result<Vec<String>, Failure> {
    let mut written = vec![];

    if self.brain.fault().is_some() {
      return Ok(written);
    }

    for library in LIBRARIES {
      let rows = self.rows(&Self::answered(library.kind(), true, agent)).await?;
      let names = self.display_names(rows.iter().filter_map(|row| row.owner)).await?;

      for (path, text) in self.brain.draw(library, &rows, &names) {
        if agent.is_some_and(|agent| self.brain.agent_of(&path).0 != agent) {
          continue;
        }

        let file = self.brain.locate(&path);

        if fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()) {
          continue;
        }

        if let Some(dir) = file.parent() {
          fs::create_dir_all(dir).map_err(|why| Failure::Disk(format!("{}: {why}", dir.display())))?;
        }

        fs::write(&file, &text).map_err(|why| Failure::Disk(format!("{}: {why}", file.display())))?;
        written.push(path);
      }
    }

    Ok(written)
  }

  /// What an editor's batch ends with, before it commits: its row writes applied to the table, and the table
  /// brought up to date — bodies read again, statuses settled, index files drawn — so the next turn reads the rows
  /// the batch meant and the commit carries the index files drawn from them. A batch names its agent, and only that
  /// agent's rows and index files are brought up to date.
  pub async fn follow_write(
    &self,
    writes: &[FragmentWrite],
    room: Option<i64>,
    agent: &str,
  ) -> Result<(), Failure> {
    self.apply_writes(writes, room).await?;

    for done in self.reconcile(Some(agent)).await? {
      if done.trouble.is_some() {
        eprintln!("fragments: the {} rows were left as they stand", done.kind);
      }
    }

    self.draw_indexes(Some(agent)).await?;

    Ok(())
  }
}

/// Why a run's row writes do not stand, or nothing where they do: a write naming no piece or a piece the run may
/// not write, a write for a piece not on disk and not marked removed, a piece whose day brackets do not name their
/// slots, a title or keywords that do not say what the piece is, and a piece the run created or changed with no
/// write of its own. It reads the brain's libraries alone, never the table.
pub fn write_problems(brain: &Brain, writing: &Writing<'_>, names: &Names) -> Vec<String> {
  let mut problems = vec![];
  let mut written = HashSet::new();

  for write in writing.writes {
    if brain.library_owning(&(writing.seen)(&write.path)).is_none() {
      problems.push(format!("the row write of {} names no piece", write.path));
      continue;
    }

    if !(writing.allowed)(&write.path) {
      problems
        .push(format!("the row write of {} names a piece outside where the run may write", write.path));
      continue;
    }

    if write.removed {
      continue;
    }

    let Some(body) = (writing.body)(&write.path) else {
      problems.push(format!("the row write of {} names a piece that is not on disk", write.path));
      continue;
    };

    if let Some(brackets) = item_bracket_problem(&body) {
      problems.push(format!("{} {brackets}", write.path));
      continue;
    }

    if let Some(shape) = title_shape_problem(&write.title, names) {
      problems.push(format!("the title of {} {shape}: {}", write.path, write.title));
      continue;
    }

    if let Some(keywords) = keyword_problem(write, names) {
      problems.push(format!("{} {keywords}", write.path));
      continue;
    }

    written.insert(write.path.clone());
  }

  for path in writing.changed {
    if brain.library_owning(&(writing.seen)(path)).is_some() && !written.contains(path) {
      problems.push(format!("{path} was written without its row"));
    }
  }

  problems
}

/// A library's rows by the filters a batch's reach names, in the order the filters are given.
pub async fn rows_in_order(table: &Table<'_>, filters: &[FragmentFilter]) -> Result<Vec<Row>, Failure> {
  let mut rows = vec![];

  for filter in filters {
    rows.extend(table.rows(filter).await?);
  }

  Ok(rows)
}
