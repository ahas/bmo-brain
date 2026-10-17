//! The person-book digest (docs/people-book.md, 「Who writes what」 through 「Records」): one agent's book of the
//! rooms it answers, written by the `people-digest` editor key a batch of messages at a time, cut out of older
//! single documents, written again from the messages already folded, sorted into its common and room-only parts,
//! and its standing sheets written by the gate's sheet judgement — every run in that agent's folder, under its
//! editing lock, held to where it may write and put back on any failure.

pub mod batches;
pub mod catchup;
pub mod prompts;
pub mod rooms;
pub mod sort;
pub mod trim;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use brain_support::js;
use fragment_desk::desk::{Beside, FragmentFilter, FragmentWrite};

use super::DocumentBackup;
use super::archive::own_number_in;
use super::book::{CHUNKS_DIR, KnownIdentifiers, PersonPiece, people_dir_of, said_length};
use super::desk::{DeskScope, ScopedDesk};
use super::documents::{
  RunClaim, known_identifiers, restore_documents, room_book_being_cut, run_touched, write_problems,
  write_problems_cause,
};
use super::editor::{Answer, EditorAsk, Limit, answer_at_desk, editor_cap_seconds};
use super::fragments::Writing;
use super::fragments::store::Row;
use super::ground::Ground;
use super::lock::{BrainLockAsk, FollowKeep, HOLD_POLL, HeldLock, QUEUE_POLL, take_brain_lock};
use super::prompts::PromptFailure;
use super::recollection::Recollection;
use super::runs::Step;
use super::settle::{clean_documents, commit_paths, reload_written, unknown_cause};
use super::sheets::{Layer, PieceRow, SheetFor, standing_pieces, write_common_sheet, write_standing_sheet};
use crate::Failure;

/// The editor key the person book is written on.
pub const KEY: &str = "people-digest";

/// The writer's own cap on one run, which a batch waiting for the lock waits past by its slack.
pub const CAP_SECONDS: u64 = 1_800;

/// The most messages one batch takes, and how many batches one room's run takes on.
pub const BATCH_SIZE: i64 = 400;
pub const BATCHES_PER_RUN: i64 = 10;

/// The scheduled launcher waits until a room has said this much for its book to be worth a batch.
pub const MINIMUM: i64 = 10;

/// The launcher's own unit, and the sort's, which a room's worker and an agent's sort are named from.
pub const LAUNCHER_UNIT: &str = "bmo-people-digest";
pub const SORT_UNIT: &str = "bmo-people-sort";

/// The failure log under the data root, for a failure the owner could not be told of.
pub const FAILURE_LOG: &str = "people-digest-failures.log";

// How long past a run's cap a batch waits for the lock: a batch that holds it is done within its own cap, or
// within the keep the agency gave it and the same slack.
const LOCK_SLACK_SECONDS: u64 = 300;

/// The unit one agent's book of one room is written in, which is the mutex for it.
pub fn worker_unit(chat_id: i64, agent: &str) -> String {
  format!("{LAUNCHER_UNIT}-{agent}-{chat_id}")
}

/// The unit one agent's room pieces are sorted in.
pub fn sort_unit(agent: &str) -> String {
  format!("{SORT_UNIT}-{agent}")
}

/// The rooms with enough waiting, the smallest backlog first: every room's book is current soon, and the room
/// with years of history takes its long turn last.
pub fn chats_to_digest(counts: &BTreeMap<i64, i64>) -> Vec<i64> {
  let mut rooms: Vec<(i64, i64)> =
    counts.iter().filter(|(_, count)| **count >= MINIMUM).map(|(a, b)| (*a, *b)).collect();

  rooms.sort_by_key(|(_, count)| *count);
  rooms.into_iter().map(|(chat_id, _)| chat_id).collect()
}

/// What stopped a run short of an ending of its own: a usage limit, after which the run stops without another try,
/// or a failure nobody named a cause for, in its own words.
#[derive(Debug)]
pub enum Stopped {
  Limit(Limit),
  Failed(String),
}

impl From<Failure> for Stopped {
  fn from(why: Failure) -> Self {
    Self::Failed(why.to_string())
  }
}

impl From<PromptFailure> for Stopped {
  fn from(why: PromptFailure) -> Self {
    Self::Failed(why.to_string())
  }
}

/// What one run of the writer, or one batch, came to: the row writes it made, or why it failed.
#[derive(Debug, PartialEq)]
pub enum Wrote {
  Stood(Vec<FragmentWrite>),
  Failed(String),
}

/// What a run of the writer may reach through the fragment tool, the brain as the run finds it, and where the files
/// its own file tools write are gathered.
pub struct WriterDesk<'d> {
  pub room: i64,
  pub places: Vec<String>,
  pub reach: Vec<FragmentFilter>,
  pub beside: Option<Beside>,
  pub carried: Vec<FragmentWrite>,
  pub before: &'d DocumentBackup,
  pub named: &'d mut BTreeSet<PathBuf>,
}

/// One agent's person-book digest, as one process of it runs.
pub struct PeopleDigest<'a> {
  pub ground: Ground<'a>,
  pub recollection: Recollection,
  /// Whether a room is worked until the backlog it started with is folded.
  pub catch_up: bool,
  /// The rooms of the agent's folder that keep no room part, whose pieces nothing here writes, sorts or trims.
  pub unkept_rooms: BTreeSet<i64>,
}

impl PeopleDigest<'_> {
  /// The folder every path of the run is relative to.
  pub fn brain(&self) -> &Path {
    &self.recollection.brain
  }

  /// The editing lock of the agent's folder, taken while no job holds the brain; none where it could not be had.
  pub async fn lock(&self, who: &str) -> Result<Option<HeldLock>, Failure> {
    let ground = &self.ground;
    let clock_run = ground.clock_run(KEY);
    let ask = BrainLockAsk {
      who,
      wait_seconds: CAP_SECONDS + LOCK_SLACK_SECONDS,
      folder: &self.recollection.folder,
      own_run: ground.own_run,
      holder: &clock_run,
      hold_poll: HOLD_POLL,
      queue_poll: QUEUE_POLL,
    };
    let keep = FollowKeep {
      agency: ground.agency,
      slack_ms: i64::try_from(LOCK_SLACK_SECONDS * 1000).unwrap_or(i64::MAX),
    };

    take_brain_lock(ground.db, ground.agency, ground.data_root, &ask, &keep)
      .await
      .map_err(|why| Failure::Disk(format!("the person book's lock: {why}")))
  }

  /// The rows of a person's book a run of theirs may reach: the room part where the book is read from the table,
  /// and the common part every room shares.
  pub fn book_reach(&self, chat_id: i64, people: &[i64], room: bool) -> Vec<FragmentFilter> {
    let agent = Some(self.recollection.agent.clone());
    let mut reach = vec![];

    if room {
      reach.push(FragmentFilter {
        kind: Some("person-book".to_owned()),
        chat_id: Some(Some(chat_id)),
        owners: Some(people.to_vec()),
        agent: agent.clone(),
        active: Some(true),
        ..FragmentFilter::default()
      });
    }

    reach.push(FragmentFilter {
      kind: Some("person-book".to_owned()),
      chat_id: Some(None),
      owners: Some(people.to_vec()),
      agent,
      active: Some(true),
      ..FragmentFilter::default()
    });

    reach
  }

  /// The rows of the agent's person book a filter names, as the run names their paths.
  pub async fn book_rows(&self, filter: FragmentFilter) -> Result<Vec<Row>, Failure> {
    let table = self.ground.table().await?;
    let filter = FragmentFilter {
      agent: Some(self.recollection.agent.clone()),
      ..filter
    };

    Ok(
      table
        .rows(&filter)
        .await?
        .into_iter()
        .map(|row| Row {
          path: self.recollection.inward(&row.path),
          ..row
        })
        .collect(),
    )
  }

  /// What a run wrote applied to the table under the paths the table names it by, and the index files drawn.
  pub async fn follow_writes(&self, writes: &[FragmentWrite], chat_id: i64) -> Result<(), Failure> {
    let outward: Vec<FragmentWrite> = writes
      .iter()
      .map(|write| FragmentWrite {
        path: self.recollection.outward(&write.path),
        ..write.clone()
      })
      .collect();

    self
      .ground
      .table()
      .await?
      .follow_write(&outward, Some(chat_id), &self.recollection.agent)
      .await
  }

  /// Who the agent is in a room: the number its own words stand under there, about which no book is written.
  pub async fn own_id_in(&self, chat_id: i64) -> Result<i64, Failure> {
    own_number_in(self.ground.receiver, &self.recollection.agent, chat_id).await
  }

  /// One run of the person-book key against one prompt: the desk, the session, its clock and the success marker are
  /// the same whatever the writer is asked to do.
  pub async fn run_writer(
    &self,
    chat_id: i64,
    prompt: &str,
    done: &str,
    desk: WriterDesk<'_>,
    people_dir: &str,
    step: Option<Step>,
  ) -> Result<Wrote, Stopped> {
    let ground = &self.ground;

    // The agent has file tools and no shell, so the directories it writes into exist beforehand.
    let _ = std::fs::create_dir_all(self.brain().join(people_dir).join(CHUNKS_DIR));

    let table = ground.table().await?;
    let cap_seconds = editor_cap_seconds(ground.agency, KEY, &self.recollection.agent).await?;
    let scoped = ScopedDesk {
      scope: DeskScope {
        room: Some(desk.room),
        agent: Some(self.recollection.agent.clone()),
        folder: self.recollection.folder.clone(),
        places: desk.places,
        reach: desk.reach,
        beside: desk.beside,
        carried: desk.carried,
      },
      dir: ground.desks(),
      db: ground.database,
      brain: ground.brain_root,
      before: desk.before,
      names: &table.names,
    };
    let answered = answer_at_desk(
      ground.agency,
      EditorAsk {
        key: KEY,
        agent: &self.recollection.agent,
        chat_id,
        prompt,
        unit: ground.unit,
        cap_seconds,
        step,
        desk: &scoped,
        registration: self.recollection.registration.as_deref(),
      },
    )
    .await;

    match answered {
      Err(limit) => Err(Stopped::Limit(limit)),
      Ok(Answer::Failed(reason)) => Ok(Wrote::Failed(reason)),
      Ok(Answer::Answered {
        answer,
        run,
        writes,
        wrote,
        ..
      }) => {
        desk.named.extend(wrote);

        if js::trim_end(&answer).ends_with(prompts::SUCCESS_MARKER) {
          println!("{done}");
          return Ok(Wrote::Stood(writes));
        }

        let chars: Vec<char> = answer.chars().collect();
        let tail: String = chars[chars.len().saturating_sub(300)..].iter().collect();

        eprintln!("people digest: chat {chat_id} ended without its success marker: {tail}");

        Ok(Wrote::Failed(format!(
          "The model run ended without saying it had finished its work, so what it wrote was not trusted. {}",
          unknown_cause(&format!("the agency's record of run {run}"))
        )))
      }
    }
  }

  /// The pieces whose file name begins with this, in one people directory: one person's whole book when the
  /// prefix is theirs, one stretch of it when the prefix carries a number.
  pub fn pieces_named(&self, people_dir: &str, prefix: &str) -> Vec<PersonPiece> {
    let dir = self.brain().join(people_dir).join(CHUNKS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    let mut names: Vec<String> = entries
      .flatten()
      .map(|entry| entry.file_name().to_string_lossy().into_owned())
      .filter(|name| name.starts_with(prefix) && name.ends_with(".md"))
      .collect();

    names.sort();
    names
      .into_iter()
      .filter_map(|name| {
        Some(PersonPiece {
          text: std::fs::read_to_string(dir.join(&name)).ok()?,
          path: format!("{people_dir}/{CHUNKS_DIR}/{name}"),
        })
      })
      .collect()
  }

  /// The pieces of one person in a people directory.
  pub fn chunks_of(&self, people_dir: &str, user_id: i64) -> Vec<PersonPiece> {
    self.pieces_named(people_dir, &format!("p{user_id}-"))
  }

  /// How much these pieces say between them, counted the way a stretch is measured against its pieces.
  pub fn said_in(pieces: &[PersonPiece]) -> usize {
    pieces.iter().map(|piece| said_length(&piece.text)).sum()
  }

  /// Whether every one of these pieces stands as a row that is up: a piece with no row is one no picker can name.
  pub async fn all_rowed(&self, pieces: &[PersonPiece]) -> Result<bool, Failure> {
    let offered: HashSet<String> = self
      .book_rows(FragmentFilter {
        kind: Some("person-book".to_owned()),
        active: Some(true),
        ..FragmentFilter::default()
      })
      .await?
      .into_iter()
      .map(|row| row.path)
      .collect();

    Ok(pieces.iter().all(|piece| offered.contains(&piece.path)))
  }

  /// What a run wrote held against where it was allowed to write, what a common document may name, and the row
  /// writes it made: a run that does not stand has everything it wrote put back, and one that stands hands its
  /// writes on. `seen` is where a path will stand once the batch is kept.
  pub async fn hold_writes(
    &self,
    backup: &DocumentBackup,
    claim: &RunClaim,
    writers: Option<&[i64]>,
    writes: Vec<FragmentWrite>,
    seen: &dyn Fn(&str) -> String,
  ) -> Result<Wrote, Failure> {
    let brain = self.brain();
    let known = known_identifiers(self.ground.agency, self.ground.receiver).await?;
    let problems = write_problems(brain, backup, claim, &known, writers);
    let touched = run_touched(brain, backup, claim);
    let allowed = |path: &str| claim.places.iter().any(|dir| path.starts_with(&format!("{dir}/")));
    let body = |path: &str| std::fs::read_to_string(brain.join(path)).ok();
    let table = self.ground.table().await?;
    let unlined = table.write_problems(&Writing {
      writes: &writes,
      changed: &touched.changed,
      allowed: &allowed,
      body: &body,
      seen,
    });

    if problems.is_empty() && unlined.is_empty() {
      return Ok(Wrote::Stood(writes));
    }

    restore_documents(brain, backup, claim);
    eprintln!(
      "people digest: writes put back: {}",
      problems.iter().chain(&unlined).cloned().collect::<Vec<_>>().join("; ")
    );

    Ok(Wrote::Failed(if problems.is_empty() {
      format!(
        "The rows the run wrote through the fragment tool did not match what it wrote on disk ({}{}), so everything it wrote was put back.",
        unlined[0],
        if unlined.len() > 1 {
          format!(" and {} more", unlined.len() - 1)
        } else {
          String::new()
        }
      )
    } else {
      write_problems_cause(&problems)
    }))
  }

  /// The second half of a pass: for each person, the sheet judgement writes that person's standing sheet of each
  /// part again from that part's pieces standing on disk now. A room sheet is not written over a book still being
  /// cut, which is what the cut reads from, unless the cut itself writes it; `common` leaves the common sheet alone.
  pub async fn write_standing_sheets(
    &self,
    chat_id: i64,
    user_ids: &[i64],
    people_dir: Option<&str>,
    overwrite_book: bool,
    common: bool,
  ) -> Result<(), Failure> {
    let people_dir = people_dir.map_or_else(|| people_dir_of(chat_id), str::to_owned);
    let mut seen = HashSet::new();

    for user_id in user_ids.iter().filter(|user_id| seen.insert(**user_id)) {
      let known: KnownIdentifiers = known_identifiers(self.ground.agency, self.ground.receiver).await?;
      let common_rows = if common {
        self
          .book_rows(FragmentFilter {
            kind: Some("person-book".to_owned()),
            chat_id: Some(None),
            owners: Some(vec![*user_id]),
            ..FragmentFilter::default()
          })
          .await?
      } else {
        vec![]
      };
      let ask = |layer| SheetFor {
        gate: self.ground.gate,
        layer,
        chat_id,
        user_id: *user_id,
        agent: &self.recollection.label,
        registration: self.recollection.registration.as_deref(),
      };
      let room_ask = ask(Layer::Room);
      let common_ask = ask(Layer::Common);
      let rows: Vec<PieceRow> = common_rows
        .into_iter()
        .map(|row| PieceRow {
          path: row.path,
          status: Some(row.status),
        })
        .collect();
      // The two parts' sheets read and write files of their own, so they are asked side by side.
      let room = self.room_sheet(&room_ask, &people_dir, overwrite_book);
      let common_sheet = async {
        if common {
          write_common_sheet(&common_ask, self.brain(), &known, &rows).await;
        }
      };
      let (room, ()) = futures_util::join!(room, common_sheet);

      room?;
    }

    Ok(())
  }

  async fn room_sheet(
    &self,
    ask: &SheetFor<'_>,
    people_dir: &str,
    overwrite_book: bool,
  ) -> Result<(), Failure> {
    let chunks = self.chunks_of(people_dir, ask.user_id);

    if chunks.is_empty() {
      return Ok(());
    }

    if !overwrite_book && room_book_being_cut(self.brain(), people_dir, ask.user_id) {
      println!(
        "people standing: chat {} person {} is still being cut; the room sheet waits for the cut",
        ask.chat_id, ask.user_id
      );
      return Ok(());
    }

    // A matter that is done is left to the search rather than read into the sheet.
    let rows: Vec<PieceRow> = self
      .book_rows(FragmentFilter {
        kind: Some("person-book".to_owned()),
        chat_id: Some(Some(ask.chat_id)),
        owners: Some(vec![ask.user_id]),
        ..FragmentFilter::default()
      })
      .await?
      .into_iter()
      .map(|row| PieceRow {
        path: row.path,
        status: Some(row.status),
      })
      .collect();
    let standing = standing_pieces(chunks, &rows);

    if standing.is_empty() {
      return Ok(());
    }

    if let Some(sheet) = write_standing_sheet(ask, &standing).await {
      let path = self.brain().join(people_dir).join(format!("{}.md", ask.user_id));

      std::fs::write(&path, &sheet).map_err(|why| Failure::Disk(format!("{}: {why}", path.display())))?;
      println!(
        "people standing: chat {} wrote {}.md ({} characters)",
        ask.chat_id,
        ask.user_id,
        js::units(js::trim(&sheet))
      );
    }

    Ok(())
  }

  /// What the run wrote cleaned before anybody reads it, the sessions holding a stale copy let go of it and told,
  /// and the paths committed into the brain where they hold anything.
  pub async fn settle_written(&self, written: &[String], committed: &[String], message: &str) {
    let ground = &self.ground;
    let brain = self.brain();
    let paths: Vec<PathBuf> = written.iter().map(|path| brain.join(path)).collect();

    clean_documents(ground.gate, ground.brain_root, &paths).await;
    reload_written(ground.db, ground.agency, ground.receiver, &self.recollection, &paths).await;

    let standing: Vec<String> =
      committed.iter().filter(|path| brain.join(path).exists()).cloned().collect();

    commit_paths(&standing, message, brain).await;
  }

  /// The run's own documents changed since the backup, by path relative to the agent's folder.
  pub fn changed_since(&self, backup: &DocumentBackup, places: &[String]) -> Vec<String> {
    run_touched(
      self.brain(),
      backup,
      &RunClaim {
        places: places.to_vec(),
        named: BTreeSet::new(),
      },
    )
    .changed
  }
}
