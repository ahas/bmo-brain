//! A room's messages folded into the person book (docs/people-book.md, 「The launcher and the worker」, 「A batch
//! writes both parts together」 and 「A failed batch is tried again before the room stops」): one room's run plans
//! its batches as it starts and works them oldest first, each batch under the agent's editing lock, written in as
//! many runs as its people's documents need, its sheets written after, and only a batch that stands marked,
//! followed into the table, cleaned, announced and committed. A batch that fails is put back and tried again, up to
//! three runs, before the room stops and the owner is told.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use brain_support::ceiling::count_text;
use brain_support::js;
use fragment_desk::desk::{Beside, FragmentWrite};
use serde_json::{Map, Value, json};

use super::super::DocumentBackup;
use super::super::archive::{
  Message, chat_title, chat_titles, display_name, mark_read, unread_counts, unread_messages,
};
use super::super::book::{
  CHUNKS_DIR, DOCUMENT_CEILING_CHARS, PersonPiece, common_dir_of, fixed_heading_slug, people_dir_of,
  piece_slug,
};
use super::super::documents::{
  BatchDocuments, RunClaim, backup_documents, batch_documents, document_list, make_common_dirs,
  mark_sorted, prune_common_dirs, restore_documents, room_book_being_cut, run_touched, sender_groups,
  within_ceiling,
};
use super::super::editor::Limit;
use super::super::fragments::{BatchText, rows_in_order};
use super::super::lock::HeldLock;
use super::super::runs::Step;
use super::super::settle::{digest_paused, resting, tell_failure};
use super::catchup::{BatchOutcome, RoomWork, Thrown, work_room};
use super::prompts::{BatchOptions, people_prompt};
use super::{BATCH_SIZE, BATCHES_PER_RUN, FAILURE_LOG, PeopleDigest, Stopped, WriterDesk, Wrote};
use crate::Failure;
use crate::ports::receiver::OwnerNotice;

/// How long a failed batch waits before it is tried again.
pub const RETRY_DELAY: Duration = Duration::from_secs(60);

/// The code a room's failed batch is told to the owner by.
pub const PEOPLE_DIGEST_FAILED: &str = "people-digest-failed";

/// Where a book's batches read it from: the table, for a batch into the room's own book, or the disk, for a
/// rewrite beside the book whose pieces have no rows yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Book {
  Table,
  Disk,
}

/// What one batch's runs write: the part of the book they write, where each may write, and what the line naming
/// a run in the log says.
pub struct Writing<'w> {
  pub people_dir: String,
  pub book: Book,
  /// For a rewrite beside the book: the directory it writes and the one its pieces will stand in.
  pub beside: Option<Beside>,
  /// What earlier batches of the same rewrite wrote, which the table does not hold yet.
  pub carried: Vec<FragmentWrite>,
  pub common_read_only: bool,
  pub unsplit: Vec<i64>,
  pub allowed_dirs: &'w dyn Fn(&[i64]) -> Vec<String>,
  /// Where the files the batch's runs write with their own file tools are gathered.
  pub named: &'w mut BTreeSet<PathBuf>,
  pub describe: &'w dyn Fn(&str) -> String,
  /// Which of the batches a room's run planned this one is, for a batch that is one.
  pub step: Option<Step>,
}

/// Where a path a rewrite beside the book writes will stand once the rewrite takes the book's place.
pub fn beside_path(beside: &Beside, path: &str) -> String {
  match path.strip_prefix(&format!("{}/", beside.from)) {
    Some(rest) => format!("{}/{rest}", beside.to),
    None => path.to_owned(),
  }
}

/// The senders of a batch the book is written for: everyone who spoke in it but the agent itself.
pub fn senders_of(batch: &[Message], bot_id: i64) -> Vec<i64> {
  let mut senders = vec![];

  for sender in batch.iter().filter_map(|message| message.sender_id).filter(|id| *id != bot_id) {
    if !senders.contains(&sender) {
      senders.push(sender);
    }
  }

  senders
}

/// What the owner is told of a room's batch that failed its last try: the room, whose book it was, how many
/// messages were left unmarked, and why.
pub fn failure_notice(
  chat_id: i64,
  title: Option<&str>,
  agent: &str,
  messages: usize,
  cause: &str,
) -> OwnerNotice {
  let mut params = Map::new();

  params.insert("chatId".to_owned(), json!(chat_id));
  params.insert("title".to_owned(), title.map_or(Value::Null, |title| json!(title)));
  params.insert("agent".to_owned(), json!(agent));
  params.insert("messages".to_owned(), json!(messages));
  params.insert("cause".to_owned(), json!(cause));

  OwnerNotice {
    code: PEOPLE_DIGEST_FAILED.to_owned(),
    params,
  }
}

impl PeopleDigest<'_> {
  // What one group's run is handed out of the fragment table: for each person, whole, their fixed-heading pieces
  // and the pieces the batch names by an item or a word; the rest the run finds through the fragment tool.
  async fn table_documents(
    &self,
    chat_id: i64,
    batch: &[Message],
    people: &[i64],
  ) -> Result<BatchDocuments, Failure> {
    let table = self.ground.table().await?;
    let text = batch.iter().map(|message| message.text.as_str()).collect::<Vec<_>>().join("\n");
    let mut speakers = vec![];

    for sender in batch.iter().filter_map(|message| message.sender_id) {
      if !speakers.contains(&sender) {
        speakers.push(sender);
      }
    }

    let mut by_sender = vec![];

    for user_id in people {
      let rows = rows_in_order(&table, &self.book_reach(chat_id, &[*user_id], true)).await?;
      let handed = table
        .pieces_for_writer(
          &rows,
          &BatchText {
            text: &text,
            speakers: &speakers,
          },
          |row| fixed_heading_slug(&row.slug),
        )
        .await?;
      let (fixed, matched): (Vec<PersonPiece>, Vec<PersonPiece>) = handed
        .into_iter()
        .map(|piece| PersonPiece {
          path: self.recollection.inward(&piece.path),
          text: piece.text,
        })
        .partition(|piece| fixed_heading_slug(piece_slug(&piece.path)));
      // The writer's prompt goes to the agency as one request, so no one person's pieces pass a run's ceiling, and
      // the pieces every batch is handed go in before the ones its words picked.
      let (pieces, held) =
        within_ceiling(fixed.into_iter().chain(matched).collect(), DOCUMENT_CEILING_CHARS);

      for piece in &held {
        println!(
          "people digest: chat {chat_id} hands {} ({} characters) by its row alone, past what a run carries",
          piece.path,
          count_text(js::units(&piece.text))
        );
      }

      by_sender.push((*user_id, pieces));
    }

    Ok(BatchDocuments {
      shared: vec![],
      by_sender,
    })
  }

  async fn documents(
    &self,
    chat_id: i64,
    batch: &[Message],
    book: Book,
    people_dir: &str,
    people: &[i64],
  ) -> Result<BatchDocuments, Failure> {
    match book {
      Book::Table => self.table_documents(chat_id, batch, people).await,
      Book::Disk => Ok(batch_documents(self.brain(), people_dir, people)),
    }
  }

  /// One batch written, in as many runs as the documents of its people need: a batch whose people hold more of the
  /// book than one run may carry is shared out into groups, each run given the whole batch and its own group's
  /// documents, and may write nobody else's. A run that does not stand has its own writes put back and ends the
  /// batch there; putting back what the groups before it wrote is the caller's.
  pub async fn write_batch(
    &self,
    chat_id: i64,
    batch: &[Message],
    senders: &[i64],
    writing: &mut Writing<'_>,
  ) -> Result<Wrote, Stopped> {
    let receiver = self.ground.receiver;
    let bot_id = self.own_id_in(chat_id).await?;
    let title = chat_title(receiver, chat_id).await?;
    let groups = sender_groups(
      &self.documents(chat_id, batch, writing.book, &writing.people_dir, senders).await?,
      DOCUMENT_CEILING_CHARS,
    );
    let mut writes: Vec<FragmentWrite> = vec![];
    let beside = writing.beside.clone();
    let seen =
      |path: &str| beside.as_ref().map_or_else(|| path.to_owned(), |beside| beside_path(beside, path));

    for (index, group) in groups.iter().enumerate() {
      // Read after the group before this one has written, so what this run is handed is the book as it left it.
      let documents = self.documents(chat_id, batch, writing.book, &writing.people_dir, group).await?;
      let unsplit: Vec<i64> =
        writing.unsplit.iter().copied().filter(|user_id| group.contains(user_id)).collect();
      let titles = chat_titles(receiver).await?;
      let listed = document_list(&documents, group);
      let prompt = people_prompt(
        self.ground.prompts(),
        &self.recollection.label,
        chat_id,
        title.as_deref(),
        bot_id,
        batch,
        &BatchOptions {
          forbidden_titles: &titles,
          people_dir: Some(&writing.people_dir),
          common_read_only: writing.common_read_only,
          unsplit_senders: &unsplit,
          documents: &listed,
          writers: Some(group),
        },
      )
      .await?;
      let before = backup_documents(self.brain());
      let places = (writing.allowed_dirs)(group);
      let at = if groups.len() == 1 {
        String::new()
      } else {
        format!(" (people {}/{})", index + 1, groups.len())
      };
      let carried: Vec<FragmentWrite> = writing.carried.iter().chain(&writes).cloned().collect();
      let written = self
        .run_writer(
          chat_id,
          &prompt,
          &(writing.describe)(&at),
          WriterDesk {
            room: chat_id,
            places: places.clone(),
            reach: self.book_reach(chat_id, group, writing.book == Book::Table),
            beside: beside.clone(),
            carried,
            before: &before,
            named: writing.named,
          },
          &writing.people_dir,
          writing.step,
        )
        .await?;
      let claim = RunClaim {
        places,
        named: writing.named.clone(),
      };
      let held = match written {
        Wrote::Stood(run_writes) => {
          self.hold_writes(&before, &claim, Some(group), run_writes, &seen).await?
        }
        failed => failed,
      };

      match held {
        Wrote::Stood(run_writes) => writes.extend(run_writes),
        failed => return Ok(failed),
      }
    }

    Ok(Wrote::Stood(writes))
  }

  /// One batch of a room's messages folded into both parts of the book of everyone it was about, then the sheets
  /// of those people. Whatever the batch wrote is put back on any failure, so its messages are read again into the
  /// book as it was. Answers what it wrote, and the senders it was about.
  pub async fn digest_batch(
    &self,
    chat_id: i64,
    batch: &[Message],
    step: Step,
  ) -> Result<(Wrote, Vec<i64>), Stopped> {
    let brain = self.brain().to_path_buf();
    let bot_id = self.own_id_in(chat_id).await?;
    let senders = senders_of(batch, bot_id);
    let people_dir = people_dir_of(chat_id);
    let unsplit: Vec<i64> = senders
      .iter()
      .copied()
      .filter(|user_id| room_book_being_cut(&brain, &people_dir, *user_id))
      .collect();
    let backup = backup_documents(&brain);
    let places: Vec<String> = std::iter::once(people_dir.clone())
      .chain(senders.iter().map(|id| common_dir_of(*id)))
      .collect();
    let mut named = BTreeSet::new();

    make_common_dirs(&brain, &senders);

    let outcome = async {
      let allowed = |group: &[i64]| -> Vec<String> {
        std::iter::once(people_dir.clone())
          .chain(group.iter().map(|id| common_dir_of(*id)))
          .collect()
      };
      let describe = |at: &str| {
        format!(
          "people digest: chat {chat_id} folded in {} messages{at} (batch {}/{})",
          count_text(batch.len()),
          step.step,
          step.steps
        )
      };
      let mut writing = Writing {
        people_dir: people_dir.clone(),
        book: Book::Table,
        beside: None,
        carried: vec![],
        common_read_only: false,
        unsplit: unsplit.clone(),
        allowed_dirs: &allowed,
        named: &mut named,
        describe: &describe,
        step: Some(step),
      };
      let written = self.write_batch(chat_id, batch, &senders, &mut writing).await?;
      let claim = RunClaim {
        places: places.clone(),
        named: named.clone(),
      };
      let Wrote::Stood(writes) = written else {
        restore_documents(&brain, &backup, &claim);
        return Ok(written);
      };
      // Every room piece of a sender whose book is not being cut was sorted whole by this run if it touched it.
      let touched: Vec<String> = run_touched(&brain, &backup, &claim)
        .changed
        .into_iter()
        .filter(|path| path.starts_with(&format!("{people_dir}/{CHUNKS_DIR}/")))
        .map(|path| piece_slug(&path).to_owned())
        .filter(|slug| {
          senders
            .iter()
            .any(|user_id| !unsplit.contains(user_id) && slug.starts_with(&format!("p{user_id}-")))
        })
        .collect();

      mark_sorted(&brain, &people_dir, &touched);
      self.write_standing_sheets(chat_id, &senders, None, false, true).await?;

      Ok::<_, Stopped>(Wrote::Stood(writes))
    }
    .await;

    if outcome.is_err() {
      restore_documents(&brain, &backup, &RunClaim { places, named });
    }

    prune_common_dirs(&brain, &senders);

    outcome.map(|wrote| (wrote, senders))
  }

  // The order is the digest's: the messages stay unmarked first, then the owner is told, and only when telling
  // fails does the failure log take over.
  pub(super) async fn report_failure(&self, chat_id: i64, batch: &[Message], reason: &str) {
    let ground = &self.ground;
    let title = chat_title(ground.receiver, chat_id).await.ok().flatten();
    let notice = failure_notice(chat_id, title.as_deref(), &self.recollection.label, batch.len(), reason);
    let first = batch.first().map_or_else(|| "?".to_owned(), |message| message.message_id.to_string());
    let last = batch.last().map_or_else(|| "?".to_owned(), |message| message.message_id.to_string());

    tell_failure(ground.receiver, &notice, &ground.failure_log(FAILURE_LOG), |why| {
      [
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        format!("chat {chat_id}"),
        format!("messages left unmarked: {} from {first} to {last}", batch.len()),
        format!("people digest failure: {reason}"),
        format!("owner notification failed: {why}"),
      ]
      .join(" | ")
    })
    .await;
  }

  // A batch that stood: its messages marked, its rows applied to the table and the index files drawn, what it
  // wrote cleaned, announced and committed.
  async fn settle_batch(
    &self,
    chat_id: i64,
    batch: &[Message],
    backup: &DocumentBackup,
    senders: &[i64],
    writes: &[FragmentWrite],
  ) -> Result<(), Failure> {
    let ids: Vec<i64> = batch.iter().map(|message| message.message_id).collect();

    mark_read(self.ground.receiver, &self.recollection.agent, chat_id, &ids).await?;
    self.follow_writes(writes, chat_id).await?;

    let places: Vec<String> = std::iter::once(people_dir_of(chat_id))
      .chain(senders.iter().map(|id| common_dir_of(*id)))
      .collect();
    let written = self.changed_since(backup, &places);

    self
      .settle_written(
        &written,
        &places,
        &format!("Room {chat_id}'s person book covers {} more messages", batch.len()),
      )
      .await;

    Ok(())
  }

  /// One room, oldest messages first, in batches until the room has nothing left or the run has taken on as much as
  /// it may (`catchup::work_room`). Answers whether the room stopped on a failure.
  pub async fn digest_chat(&self, chat_id: i64, retry_delay: Duration) -> Result<bool, Stopped> {
    let mut room = RoomRun {
      digest: self,
      chat_id,
      backup: DocumentBackup::default(),
      senders: vec![],
      writes: vec![],
      limit: None,
      retry_delay,
    };
    let failed = work_room(&mut room).await?;

    if let Some(limit) = room.limit.take() {
      return Err(Stopped::Limit(limit));
    }

    let left = unread_counts(self.ground.db, self.ground.receiver, &self.recollection.agent)
      .await?
      .get(&chat_id)
      .copied()
      .unwrap_or(0);

    if !failed && left > 0 {
      println!(
        "people digest: chat {chat_id} has {} messages left for the next run",
        count_text(usize::try_from(left).unwrap_or(usize::MAX))
      );
    }

    Ok(failed)
  }

  /// The display name a person's common book is headed with, where the receiver knows one.
  pub async fn display_name_of(&self, user_id: i64) -> Result<Option<String>, Failure> {
    display_name(self.ground.receiver, user_id).await
  }
}

// One room's run as the batch loop asks it: what each batch stood on and wrote, carried from its run to its settling,
// and the usage limit that ended the run where one did.
struct RoomRun<'p, 'a> {
  digest: &'p PeopleDigest<'a>,
  chat_id: i64,
  backup: DocumentBackup,
  senders: Vec<i64>,
  writes: Vec<FragmentWrite>,
  limit: Option<Limit>,
  retry_delay: Duration,
}

impl RoomWork for RoomRun<'_, '_> {
  type Batch = Message;
  type Held = HeldLock;
  type Fatal = Limit;

  fn chat_id(&self) -> i64 {
    self.chat_id
  }

  fn catch_up(&self) -> bool {
    self.digest.catch_up
  }

  fn batches_per_run(&self) -> i64 {
    BATCHES_PER_RUN
  }

  fn batch_size(&self) -> i64 {
    BATCH_SIZE
  }

  fn record(&self) -> String {
    self.digest.ground.record()
  }

  fn retry_delay(&self) -> Duration {
    self.retry_delay
  }

  async fn backlog(&mut self) -> Result<i64, Failure> {
    let ground = &self.digest.ground;

    Ok(
      unread_counts(ground.db, ground.receiver, &self.digest.recollection.agent)
        .await?
        .get(&self.chat_id)
        .copied()
        .unwrap_or(0),
    )
  }

  async fn resting(&mut self) -> Result<bool, Failure> {
    resting(self.digest.ground.agency).await
  }

  async fn paused(&mut self) -> Result<bool, Failure> {
    digest_paused(self.digest.ground.db).await
  }

  async fn next_batch(&mut self) -> Result<Vec<Message>, Failure> {
    let ground = &self.digest.ground;

    unread_messages(ground.db, ground.receiver, &self.digest.recollection.agent, self.chat_id, BATCH_SIZE)
      .await
  }

  async fn lock(&mut self) -> Result<Option<HeldLock>, Failure> {
    self.digest.lock(&format!("people digest: chat {}", self.chat_id)).await
  }

  async fn run(&mut self, batch: &[Message], step: Step) -> Result<BatchOutcome, Thrown<Limit>> {
    self.backup = backup_documents(self.digest.brain());

    match self.digest.digest_batch(self.chat_id, batch, step).await {
      Ok((Wrote::Stood(writes), senders)) => {
        self.senders = senders;
        self.writes = writes;
        Ok(BatchOutcome::Stood)
      }
      Ok((Wrote::Failed(reason), senders)) => {
        self.senders = senders;
        self.writes = vec![];
        Ok(BatchOutcome::Failed(reason))
      }
      Err(Stopped::Limit(limit)) => Err(Thrown::Fatal(limit)),
      Err(Stopped::Failed(why)) => Err(Thrown::Unexpected(why)),
    }
  }

  async fn settle(&mut self, batch: &[Message]) -> Result<(), String> {
    self
      .digest
      .settle_batch(self.chat_id, batch, &self.backup, &self.senders, &self.writes)
      .await
      .map_err(|why| why.to_string())
  }

  async fn report(&mut self, batch: &[Message], reason: &str) {
    self.digest.report_failure(self.chat_id, batch, reason).await;
  }

  // The agency has rested the login and told the owner; the batch is put back already and its messages wait.
  async fn on_fatal(&mut self, _batch: &[Message], limit: Limit) {
    eprintln!(
      "people digest: run {} reached a usage limit, stopping; the messages stay unmarked",
      limit.run
    );
    self.limit = Some(limit);
  }

  fn log(&mut self, line: &str) {
    println!("{line}");
  }
}
