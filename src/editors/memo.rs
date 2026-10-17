//! The memo digest (docs/knowledge.md, 「From a note to a document」 and 「The digest run」): each agent's pending notes
//! taken into its own long-term memory, room by room, the notes that can be taken as the owner's in a batch of their
//! own. One batch is one run of the `digest` editor key at a desk of its own, in the agent's folder, under that
//! folder's editing lock; what it wrote is held to where it may write — the room's rulebook, the knowledge library
//! and the common person books of the notes' authors — to what a common book may name, and to what only a batch of
//! the owner's own notes may do to the rules every room reads. A batch that fails any of it, or fails at all, has
//! everything it wrote put back and deletes no note; one that stands deletes exactly the notes it was handed.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use brain_support::ceiling::count_text;
use brain_support::js;
use fragment_desk::desk::{FragmentFilter, FragmentWrite, Names};
use serde_json::{Map, Value, json};

use super::DocumentBackup;
use super::archive::{
  Note, chat_title, chat_titles, delete_notes, display_name, pending_notes, people_counts,
};
use super::book::{CHUNKS_DIR, KNOWLEDGE_ROOT, PEOPLE_ROOT, common_dir_of, room_rules_dir_of};
use super::desk::{DeskScope, ScopedDesk};
use super::documents::{
  RunClaim, backup_documents, known_identifiers, make_common_dirs, prune_common_dirs, restore_documents,
  run_touched, write_problems, write_problems_cause,
};
use super::editor::{Answer, EditorAsk, Limit, answer_at_desk, editor_cap_seconds};
use super::fragments::{BatchText, Writing, rows_in_order};
use super::ground::Ground;
use super::lock::{
  BrainLockAsk, FollowKeep, HOLD_POLL, QUEUE_POLL, brain_lock_path, take_brain_lock, wait_for_brain,
};
use super::people::MINIMUM as PEOPLE_MINIMUM;
use super::prompts::{PromptFailure, Prompts, forbidden_titles_text, fragment_writer, seoul_timestamp};
use super::recollection::{Recollection, recollections};
use super::settle::{
  PausedBacklog, PausedRoom, clean_documents, commit_paths, digest_paused, notice_paused_digests,
  reload_written, resting, tell_failure, unknown_cause,
};
use super::sheets::{Layer, PieceRow, SheetFor, write_common_sheet};
use crate::Failure;
use crate::ports::receiver::OwnerNotice;

/// The editor key the digest runs on.
pub const KEY: &str = "digest";

/// What a run of the digest must end its answer with for what it wrote to be trusted.
pub const SUCCESS_MARKER: &str = "DIGEST OK";

/// How long before a note the owner must have spoken in that room for the note to count as the owner's.
pub const OWNER_WINDOW_SECONDS: i64 = 3_600;

/// The code a room's failed digest is told to the owner by, and the one a new candidate rule is.
pub const DIGEST_FAILED: &str = "digest-failed";
pub const RULE_CANDIDATE: &str = "rule-candidate";

/// The failure log under the data root, for a failure the owner could not be told of.
pub const FAILURE_LOG: &str = "digest-failures.log";

// How long a batch waits for its folder's editing lock, and the slack a wait behind a kept run goes on for.
const LOCK_WAIT_SECONDS: u64 = 1_200;
const LOCK_SLACK_MS: i64 = 5 * 60_000;

// The common pieces the digest files notes into, which it is handed whole for every author.
const FILED_TOPICS: [&str; 2] = ["requests", "told"];

/// One batch of a room's notes, and whether every note in it counts as the owner's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteBatch {
  pub owner_batch: bool,
  pub notes: Vec<Note>,
}

/// A room's notes as the batches they are filed in: the notes that count as the owner's, and every other note,
/// each a batch of its own and the owner's first.
pub fn batches_by_owner(notes: &[Note], counts: impl Fn(&Note) -> bool) -> Vec<NoteBatch> {
  let (owner, others): (Vec<Note>, Vec<Note>) = notes.iter().cloned().partition(|note| counts(note));

  [(true, owner), (false, others)]
    .into_iter()
    .filter(|(_, notes)| !notes.is_empty())
    .map(|(owner_batch, notes)| NoteBatch { owner_batch, notes })
    .collect()
}

/// The candidates a batch added to the candidates document: its item lines that were not there before.
pub fn added_candidates(before: &str, after: &str) -> Vec<String> {
  let items = |text: &str| -> Vec<String> {
    text
      .split('\n')
      .map(js::trim)
      .filter(|line| line.starts_with("- "))
      .map(str::to_owned)
      .collect()
  };
  let standing: HashSet<String> = items(before).into_iter().collect();

  items(after).into_iter().filter(|line| !standing.contains(line)).collect()
}

/// What a run did to the rules every room reads that only the owner may do: make, change or remove a knowledge
/// chunk every room reads, or take a candidate off. Nothing where the batch is the owner's own.
pub fn owner_only_problems(
  before: &str,
  after: &str,
  shared_touched: &[String],
  owner_batch: bool,
) -> Vec<String> {
  if owner_batch {
    return vec![];
  }

  let mut problems = vec![];

  if !shared_touched.is_empty() {
    problems.push(format!(
      "made, changed or removed {} rules for every room on notes that are not the owner's: {}",
      shared_touched.len(),
      shared_touched.join("; ")
    ));
  }

  let removed = added_candidates(after, before);

  if !removed.is_empty() {
    problems.push(format!(
      "took {} candidates off on notes that are not the owner's: {}",
      removed.len(),
      removed.join("; ")
    ));
  }

  problems
}

/// One room's notes as the digest's editor is told them.
pub struct DigestNotes<'a> {
  /// What the agent's task calls it.
  pub agent: &'a str,
  pub chat_id: i64,
  pub notes: &'a [Note],
  /// The notes the code counts as the owner's.
  pub owners: &'a HashSet<i64>,
  /// Every room title, which a common document may not carry.
  pub forbidden_titles: &'a [String],
  /// The authors' display names.
  pub names: &'a BTreeMap<i64, String>,
  /// The documents it is handed whole, as its prompt shows them.
  pub documents: &'a str,
}

/// What the digest's editor is told for one room's notes: every note with its author, whether the code counts that
/// author as the owner, and whether its writer kept it for every room; the authors' names, the documents it is
/// handed whole, and the room titles a common document may not carry.
pub async fn digest_prompt(
  prompts: Prompts<'_>,
  digest: &DigestNotes<'_>,
) -> Result<String, PromptFailure> {
  let mut authors: Vec<i64> = vec![];

  for author in digest.notes.iter().filter_map(|note| note.author) {
    if !authors.contains(&author) {
      authors.push(author);
    }
  }

  let lines: Vec<String> = digest
    .notes
    .iter()
    .map(|note| {
      format!(
        "{} | {} | {} | {} | {} | {}",
        note.id,
        note.author.map_or_else(|| "unknown".to_owned(), |author| author.to_string()),
        if digest.owners.contains(&note.id) { "owner" } else { "member" },
        if note.shared { "every room" } else { "this chat" },
        seoul_timestamp(note.created_at),
        note.text
      )
    })
    .collect();
  let author_names = if authors.is_empty() {
    "none".to_owned()
  } else {
    authors
      .iter()
      .map(|id| format!("{id} \"{}\"", digest.names.get(id).map_or("unknown", String::as_str)))
      .collect::<Vec<_>>()
      .join(", ")
  };

  fragment_writer(
    prompts,
    "digest",
    "task.md",
    &[
      ("agent", digest.agent),
      ("chatId", &digest.chat_id.to_string()),
      ("count", &digest.notes.len().to_string()),
      ("notes", &lines.join("\n")),
      ("authorNames", &author_names),
      ("commonRoot", PEOPLE_ROOT),
      ("documents", digest.documents),
      ("forbiddenTitles", &forbidden_titles_text(digest.forbidden_titles)),
      ("successMarker", SUCCESS_MARKER),
    ],
  )
  .await
}

/// Why a run's writes were put back, one sentence for each way they fell short.
pub fn put_back_reason(writes: &[String], row_problems: &[String], owner_only: &[String]) -> String {
  let mut reasons = vec![];

  if !writes.is_empty() {
    reasons.push(write_problems_cause(writes));
  }

  if let Some(first) = row_problems.first() {
    reasons.push(format!(
      "The rows the run wrote through the fragment tool did not match what it wrote on disk ({first}{}), so everything it wrote was put back.",
      if row_problems.len() > 1 { format!(" and {} more", row_problems.len() - 1) } else { String::new() }
    ));
  }

  if !owner_only.is_empty() {
    reasons.push(
      "The run changed the rules every room reads on notes that are not the owner's, which only the owner's own notes may do, so everything it wrote was put back."
        .to_owned(),
    );
  }

  reasons.join(" ")
}

/// What the owner is told of a room's digest that did not stand: the room, whose memory it was, how many notes
/// were left pending, and why.
pub fn failure_notice(
  chat_id: i64,
  title: Option<&str>,
  agent: &str,
  notes: usize,
  cause: &str,
) -> OwnerNotice {
  let mut params = Map::new();

  params.insert("chatId".to_owned(), json!(chat_id));
  params.insert("title".to_owned(), title.map_or(Value::Null, |title| json!(title)));
  params.insert("agent".to_owned(), json!(agent));
  params.insert("notes".to_owned(), json!(notes));
  params.insert("cause".to_owned(), json!(cause));

  OwnerNotice {
    code: DIGEST_FAILED.to_owned(),
    params,
  }
}

/// What the owner is told of a candidate rule a batch wrote: whose rules it would join, and the item itself.
pub fn candidate_notice(agent: &str, candidate: &str) -> OwnerNotice {
  let mut params = Map::new();

  params.insert("agent".to_owned(), json!(agent));
  params.insert("candidate".to_owned(), json!(candidate.strip_prefix("- ").unwrap_or(candidate)));

  OwnerNotice {
    code: RULE_CANDIDATE.to_owned(),
    params,
  }
}

/// What one batch came to.
enum Filed {
  Stood,
  Waiting,
  Failed(String),
}

/// What stopped a batch short of an ending of its own: a usage limit, after which the run stops without another
/// try, or a failure nobody named a cause for, in its own words.
enum Stopped {
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

/// What one editor run of a batch came to: the row writes it made and the files its own tools wrote, or why it
/// failed.
pub enum Ran {
  Stood {
    writes: Vec<FragmentWrite>,
    wrote: BTreeSet<PathBuf>,
  },
  Failed(String),
}

/// The digest of one process, on its ground.
pub struct MemoDigest<'a> {
  pub ground: Ground<'a>,
}

impl MemoDigest<'_> {
  // What waits behind the owner's pause: the notes, and each agent's person book of a room with enough unread.
  async fn backlog(&self) -> Result<PausedBacklog, Failure> {
    let ground = &self.ground;
    let mut rooms = vec![];

    for recollection in recollections(ground.agency, ground.brain_root, ground.chat_key).await? {
      for (chat_id, messages) in people_counts(ground.db, ground.receiver, &recollection.agent).await? {
        if messages >= PEOPLE_MINIMUM {
          rooms.push(PausedRoom {
            chat_id,
            title: chat_title(ground.receiver, chat_id).await?,
            agent: recollection.label.clone(),
            messages,
          });
        }
      }
    }

    rooms.sort_by_key(|room| std::cmp::Reverse(room.messages));

    Ok(PausedBacklog {
      notes: i64::try_from(pending_notes(ground.db, None).await?.len()).unwrap_or(i64::MAX),
      rooms,
    })
  }

  /// The hourly digest, or one room's where a chat is named: the process's exit code.
  pub async fn run(&self, chat_filter: Option<i64>) -> Result<u8, Failure> {
    let ground = &self.ground;
    let db = ground.db;

    // While the digests are paused nothing starts, and what the pause holds back is said to the owner once.
    if digest_paused(db).await? {
      notice_paused_digests(db, ground.receiver, async || self.backlog().await).await?;
      println!("digest: paused, nothing started");
      return Ok(0);
    }

    // The digest is a full high-effort session; during a rest it must not start.
    if resting(ground.receiver).await? {
      return Ok(0);
    }

    // A job editing the brain is waited out before any note is taken on, and the rest and the pause asked again.
    if !pending_notes(db, None).await?.is_empty()
      && wait_for_brain(db, ground.agency, KEY, ground.own_run, HOLD_POLL).await
      && (resting(ground.receiver).await? || digest_paused(db).await?)
    {
      return Ok(0);
    }

    // Each agent's notes are taken into its own long-term memory alone, room by room.
    let mut work: Vec<(Recollection, i64, Vec<Note>)> = vec![];

    for recollection in recollections(ground.agency, ground.brain_root, ground.chat_key).await? {
      let mut by_chat: Vec<(i64, Vec<Note>)> = vec![];

      for note in pending_notes(db, Some(&recollection.agent)).await? {
        if chat_filter.is_some_and(|chat| chat != note.chat_id) {
          continue;
        }

        match by_chat.iter_mut().find(|(chat, _)| *chat == note.chat_id) {
          Some((_, notes)) => notes.push(note),
          None => by_chat.push((note.chat_id, vec![note])),
        }
      }

      work.extend(by_chat.into_iter().map(|(chat, notes)| (recollection.clone(), chat, notes)));
    }

    let mut failed = false;

    for (recollection, chat_id, notes) in work {
      let mut owned = HashSet::new();

      // An author counts as the owner only where the receiver says so, and the owner spoke in the room within the
      // hour before the note: the room session fills the author in, so its value alone proves nothing.
      for note in &notes {
        let Some(author) = note.author else { continue };
        let spoke = ground
          .receiver
          .owner_spoke(note.chat_id, author, note.created_at, OWNER_WINDOW_SECONDS)
          .await?;

        if spoke.owner && spoke.spoke {
          owned.insert(note.id);
        }
      }

      for batch in batches_by_owner(&notes, |note| owned.contains(&note.id)) {
        match self.file_notes(&recollection, chat_id, &batch).await {
          Ok(Filed::Stood) => {}
          Ok(Filed::Waiting) => println!(
            "digest: chat {chat_id} of {} waits for the person book's lock; its notes stay for the next run",
            recollection.agent
          ),
          Ok(Filed::Failed(reason)) => {
            eprintln!(
              "digest: chat {chat_id} of {} failed, its notes stay pending: {reason}",
              recollection.agent
            );
            self.report_failure(&recollection, chat_id, &batch.notes, &reason).await;
            failed = true;
          }
          // The agency has rested the login and told the owner; the notes wait for a later hour.
          Err(Stopped::Limit(limit)) => {
            eprintln!("digest: run {} reached a usage limit, stopping; the notes stay pending", limit.run);
            return Ok(1);
          }
          Err(Stopped::Failed(why)) => {
            eprintln!(
              "digest: chat {chat_id} of {} failed, its notes stay pending: {why}",
              recollection.agent
            );

            let cause = format!(
              "The run stopped on an error nobody anticipated. {}",
              unknown_cause(&ground.record())
            );

            self.report_failure(&recollection, chat_id, &batch.notes, &cause).await;
            failed = true;
          }
        }
      }
    }

    Ok(u8::from(failed))
  }

  // The order is fixed: the notes stay pending first, then the owner is told, and only when telling fails does the
  // failure log take over as the last resort.
  async fn report_failure(&self, recollection: &Recollection, chat_id: i64, notes: &[Note], reason: &str) {
    let ground = &self.ground;
    let title = chat_title(ground.receiver, chat_id).await.ok().flatten();
    let notice = failure_notice(chat_id, title.as_deref(), &recollection.label, notes.len(), reason);
    let ids: Vec<String> = notes.iter().map(|note| note.id.to_string()).collect();

    tell_failure(ground.receiver, &notice, &ground.failure_log(FAILURE_LOG), |why| {
      [
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        format!("chat {chat_id}"),
        format!("notes left pending: {}", ids.join(",")),
        format!("digest failure: {reason}"),
        format!("owner notification failed: {why}"),
      ]
      .join(" | ")
    })
    .await;
  }

  // One batch of one agent's notes of a room, from the owner check's verdicts to the documents committed, in that
  // agent's folder and from its own view.
  async fn file_notes(
    &self,
    recollection: &Recollection,
    chat_id: i64,
    batch: &NoteBatch,
  ) -> Result<Filed, Stopped> {
    let ground = &self.ground;
    let mut authors: Vec<i64> = vec![];

    for author in batch.notes.iter().filter_map(|note| note.author) {
      if !authors.contains(&author) {
        authors.push(author);
      }
    }

    // The lock is taken whatever the notes are: what this run wrote is told apart from the rest of the agent's
    // folder by comparing the whole of it before and after.
    let clock_run = ground.clock_run(KEY);
    let who = format!("digest: chat {chat_id}");
    let ask = BrainLockAsk {
      who: &who,
      wait_seconds: LOCK_WAIT_SECONDS,
      folder: &recollection.folder,
      own_run: ground.own_run,
      holder: &clock_run,
      hold_poll: HOLD_POLL,
      queue_poll: QUEUE_POLL,
    };
    let keep = FollowKeep {
      agency: ground.agency,
      slack_ms: LOCK_SLACK_MS,
    };
    let lock =
      take_brain_lock(ground.db, ground.agency, ground.data_root, &ask, &keep)
        .await
        .map_err(|why| {
          Stopped::Failed(format!(
            "{}: {why}",
            brain_lock_path(ground.data_root, &recollection.folder).display()
          ))
        })?;
    let Some(lock) = lock else { return Ok(Filed::Waiting) };
    let brain = recollection
      .stand()
      .map_err(|why| Stopped::Failed(format!("{}: {why}", recollection.brain.display())))?;
    let backup = backup_documents(&brain);
    let filed = self.filed(recollection, chat_id, batch, &authors, &brain, &backup).await;

    prune_common_dirs(&brain, &authors);
    lock.release();

    filed
  }

  #[allow(clippy::too_many_lines)]
  async fn filed(
    &self,
    recollection: &Recollection,
    chat_id: i64,
    batch: &NoteBatch,
    authors: &[i64],
    brain: &Path,
    backup: &DocumentBackup,
  ) -> Result<Filed, Stopped> {
    let ground = &self.ground;
    let notes = &batch.notes;
    let rules_dir = room_rules_dir_of(chat_id);
    let candidates = brain.join(KNOWLEDGE_ROOT).join("candidates.md");
    let before = std::fs::read_to_string(&candidates).unwrap_or_default();
    let places: Vec<String> = [rules_dir.clone(), KNOWLEDGE_ROOT.to_owned()]
      .into_iter()
      .chain(authors.iter().map(|author| common_dir_of(*author)))
      .collect();
    let claim = RunClaim {
      places: places.clone(),
      named: BTreeSet::new(),
    };
    let filter = |kind: &str, chat_id: Option<i64>, owners: Option<Vec<i64>>| FragmentFilter {
      kind: Some(kind.to_owned()),
      chat_id: Some(chat_id),
      owners,
      agent: Some(recollection.agent.clone()),
      active: Some(true),
      ..FragmentFilter::default()
    };
    let mut reach = vec![
      filter("room-rule", Some(chat_id), None),
      filter("library-knowledge", Some(chat_id), None),
      filter("library-knowledge", None, None),
    ];

    if !authors.is_empty() {
      reach.push(filter("person-book", None, Some(authors.to_vec())));
    }

    let table = ground.table().await?;
    let text = notes.iter().map(|note| note.text.as_str()).collect::<Vec<_>>().join("\n");
    let handed = table
      .pieces_for_writer(
        &rows_in_order(&table, &reach).await?,
        &BatchText {
          text: &text,
          speakers: authors,
        },
        |row| {
          row.kind == "person-book"
            && FILED_TOPICS.iter().any(|topic| row.slug.ends_with(&format!("-{topic}")))
        },
      )
      .await?;

    // The agent has file tools and no shell, so the directories it writes into exist beforehand.
    let _ = std::fs::create_dir_all(brain.join(&rules_dir));
    let _ = std::fs::create_dir_all(brain.join(KNOWLEDGE_ROOT));
    make_common_dirs(brain, authors);

    let restore = || restore_documents(brain, backup, &claim);
    let mut names = BTreeMap::new();

    for author in authors {
      if let Some(name) = display_name(ground.receiver, *author).await? {
        names.insert(*author, name);
      }
    }

    let documents = if handed.is_empty() {
      "None.".to_owned()
    } else {
      handed
        .iter()
        .map(|piece| format!("[{}]\n{}", recollection.inward(&piece.path), js::trim(&piece.text)))
        .collect::<Vec<_>>()
        .join("\n\n")
    };
    let owners: HashSet<i64> = if batch.owner_batch {
      notes.iter().map(|note| note.id).collect()
    } else {
      HashSet::new()
    };
    let titles = chat_titles(ground.receiver).await?;
    let prompt = digest_prompt(
      ground.prompts(),
      &DigestNotes {
        agent: &recollection.label,
        chat_id,
        notes,
        owners: &owners,
        forbidden_titles: &titles,
        names: &names,
        documents: &documents,
      },
    )
    .await?;
    let scope = DeskScope {
      room: Some(chat_id),
      agent: Some(recollection.agent.clone()),
      folder: recollection.folder.clone(),
      places: places.clone(),
      reach,
      beside: None,
      carried: vec![],
    };
    let ran = match self
      .digest_chat(recollection, chat_id, &prompt, notes.len(), scope, backup, &table.names)
      .await
    {
      Ok(ran) => ran,
      Err(stopped) => {
        restore();
        return Err(stopped);
      }
    };
    let (writes, wrote) = match ran {
      Ran::Stood { writes, wrote } => (writes, wrote),
      Ran::Failed(reason) => {
        restore();
        return Ok(Filed::Failed(reason));
      }
    };
    let claim = RunClaim {
      places: places.clone(),
      named: wrote,
    };
    let known = match known_identifiers(ground.agency, ground.receiver).await {
      Ok(known) => known,
      Err(why) => {
        restore_documents(brain, backup, &claim);
        return Err(why.into());
      }
    };
    let problems = write_problems(brain, backup, &claim, &known, None);
    let touched = run_touched(brain, backup, &claim);
    let allowed = |path: &str| places.iter().any(|place| path.starts_with(&format!("{place}/")));
    let body = |path: &str| std::fs::read_to_string(brain.join(path)).ok();
    let seen = |path: &str| path.to_owned();
    let row_problems = table.write_problems(&Writing {
      writes: &writes,
      changed: &touched.changed,
      allowed: &allowed,
      body: &body,
      seen: &seen,
    });
    let outward: Vec<FragmentWrite> = writes
      .iter()
      .map(|write| FragmentWrite {
        path: recollection.outward(&write.path),
        ..write.clone()
      })
      .collect();
    let after = std::fs::read_to_string(&candidates).unwrap_or_default();
    let shared = match table.shared_write_paths(&outward, Some(chat_id)).await {
      Ok(shared) => shared,
      Err(why) => {
        restore_documents(brain, backup, &claim);
        return Err(why.into());
      }
    };
    let owner_only = owner_only_problems(&before, &after, &shared, batch.owner_batch);

    if !problems.is_empty() || !row_problems.is_empty() || !owner_only.is_empty() {
      restore_documents(brain, backup, &claim);
      eprintln!(
        "digest: chat {chat_id} writes put back: {}",
        problems
          .iter()
          .chain(&row_problems)
          .chain(&owner_only)
          .cloned()
          .collect::<Vec<_>>()
          .join("; ")
      );

      return Ok(Filed::Failed(put_back_reason(&problems, &row_problems, &owner_only)));
    }

    // Only the notes handed over are deleted, and only on a confirmed success.
    let settled: Result<(), Failure> = async {
      delete_notes(ground.db, &notes.iter().map(|note| note.id).collect::<Vec<_>>()).await?;
      table.follow_write(&outward, Some(chat_id), &recollection.agent).await?;

      for author in authors {
        let chunks = format!("{}/{CHUNKS_DIR}/", common_dir_of(*author));

        if touched.changed.iter().any(|path| path.starts_with(&chunks)) {
          let rows = table
            .rows(&FragmentFilter {
              kind: Some("person-book".to_owned()),
              chat_id: Some(None),
              owners: Some(vec![*author]),
              agent: Some(recollection.agent.clone()),
              ..FragmentFilter::default()
            })
            .await?
            .into_iter()
            .map(|row| PieceRow {
              path: recollection.inward(&row.path),
              status: Some(row.status),
            })
            .collect::<Vec<_>>();
          let ask = SheetFor {
            gate: ground.gate,
            layer: Layer::Common,
            chat_id,
            user_id: *author,
            agent: &recollection.label,
            registration: recollection.registration.as_deref(),
          };

          write_common_sheet(&ask, brain, &known, &rows).await;
        }
      }

      Ok(())
    }
    .await;

    if let Err(why) = settled {
      restore_documents(brain, backup, &claim);
      return Err(why.into());
    }

    // What the run wrote is cleaned before anybody is told to read it, then announced and committed.
    let written: Vec<PathBuf> =
      run_touched(brain, backup, &claim).changed.iter().map(|path| brain.join(path)).collect();

    clean_documents(ground.gate, ground.brain_root, &written).await;
    reload_written(ground.db, ground.agency, ground.receiver, recollection, &written).await;

    let committed: Vec<String> = places.iter().filter(|path| brain.join(path).exists()).cloned().collect();

    commit_paths(
      &committed,
      &format!(
        "Room {chat_id}'s notes are filed into {}'s rulebook, knowledge library and their authors' person books ({} notes)",
        recollection.label,
        notes.len()
      ),
      brain,
    )
    .await;

    self.tell_candidates(recollection, &before, &after).await;

    Ok(Filed::Stood)
  }

  // A rule for every room asked for by anyone but the owner waits for the owner as a candidate, and the owner is
  // told of each one the batch added.
  async fn tell_candidates(&self, recollection: &Recollection, before: &str, after: &str) {
    for candidate in added_candidates(before, after) {
      let notice = candidate_notice(&recollection.label, &candidate);

      match self.ground.receiver.notify_owner(&notice).await {
        Ok(true) => {}
        Ok(false) => eprintln!("digest: the receiver did not take the notice of a candidate"),
        Err(why) => eprintln!("digest: telling the owner about a candidate failed: {why}"),
      }
    }
  }

  // One run of the digest key for one batch.
  #[allow(clippy::too_many_arguments)]
  async fn digest_chat(
    &self,
    recollection: &Recollection,
    chat_id: i64,
    prompt: &str,
    count: usize,
    scope: DeskScope,
    backup: &DocumentBackup,
    names: &Names,
  ) -> Result<Ran, Stopped> {
    let ground = &self.ground;
    let cap_seconds = editor_cap_seconds(ground.agency, KEY, &recollection.agent).await?;
    let desk = ScopedDesk {
      scope,
      dir: ground.desks(),
      db: ground.database,
      brain: ground.brain_root,
      before: backup,
      names,
    };
    let answered = answer_at_desk(
      ground.agency,
      EditorAsk {
        key: KEY,
        agent: &recollection.agent,
        chat_id,
        prompt,
        unit: ground.unit,
        cap_seconds,
        step: None,
        desk: &desk,
        registration: recollection.registration.as_deref(),
      },
    )
    .await;

    match answered {
      Err(limit) => Err(Stopped::Limit(limit)),
      Ok(Answer::Failed(reason)) => Ok(Ran::Failed(reason)),
      Ok(Answer::Answered {
        answer,
        run,
        writes,
        wrote,
        ..
      }) => {
        if js::trim_end(&answer).ends_with(SUCCESS_MARKER) {
          println!("digest: chat {chat_id} distilled {} notes", count_text(count));
          return Ok(Ran::Stood { writes, wrote });
        }

        let chars: Vec<char> = answer.chars().collect();
        let tail: String = chars[chars.len().saturating_sub(300)..].iter().collect();

        eprintln!("digest: chat {chat_id} ended without its success marker: {tail}");

        Ok(Ran::Failed(format!(
          "The model run ended without saying it had finished its work, so what it wrote was not trusted. {}",
          unknown_cause(&format!("the agency's record of run {run}"))
        )))
      }
    }
  }
}
