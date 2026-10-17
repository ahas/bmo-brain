//! Sorting a person's unsorted room pieces into the two parts of their book, one group at a time (docs/people-book.md,
//! 「Sorting pieces」): a book still in one document is cut first, a stretch a run; then each group of unsorted
//! pieces is handed to one run, which writes what is the same in every room into the person's common part and leaves
//! the rest in the room's. The pieces on disk and each room's sorted list are its whole state, so it stops anywhere
//! and picks up from there. A failure is remembered by what it was done to, and the second on the same unchanged
//! material leaves that work skipped and the owner told.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

use brain_support::js;
use fragment_desk::desk::{FragmentFilter, FragmentWrite, Names};
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value, json};

use super::super::DocumentBackup;
use super::super::archive::chat_title;
use super::super::book::{
  CHUNKS_DIR, KnownIdentifiers, PersonPiece, ROOMS_ROOT, common_dir_of, people_dir_of, piece_slug,
  said_length,
};
use super::super::documents::{
  RunClaim, backup_documents, common_pieces, known_identifiers, mark_sorted, restore_documents,
  room_book_being_cut, room_piece_slugs, run_touched, sorted_slugs, write_problems, write_problems_cause,
};
use super::super::fragments::libraries::Brain;
use super::super::fragments::{Writing, names as table_names, write_problems as row_problems};
use super::super::lock::HeldLock;
use super::super::prompts::Prompts;
use super::super::settle::{resting, unknown_cause};
use super::prompts::sort_prompt;
use super::trim::{trim_person, trim_status, trims_of};
use super::{PeopleDigest, Stopped, WriterDesk, Wrote};
use crate::Failure;
use crate::policy::BRAIN;
use crate::ports::receiver::OwnerNotice;
use crate::settings;

/// How much of one person's room pieces one sorting run is handed.
pub const GROUP_CHARS: usize = 8_000;

/// How much of a group its sorted result must still account for: a faithful sort turns paragraphs into items and
/// loses the words that joined the sentences, while a summary falls to about half.
pub const SORT_FIDELITY_FLOOR: f64 = 0.8;

/// How many failures of the same unchanged group or book leave that work skipped.
pub const SKIP_AFTER: i64 = 2;

/// The code a skipped work of the sort is told to the owner by.
pub const PEOPLE_SORT_FAILED: &str = "people-sort-failed";

// The sort's failures under the brain's settings, by the fingerprint of what failed.
const FAILURE_PREFIX: &str = "people_sort_failure:";

/// How a person's sort ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
  Done,
  Stopped,
  Failed,
}

impl Ended {
  pub fn word(self) -> &'static str {
    match self {
      Self::Done => "done",
      Self::Stopped => "stopped",
      Self::Failed => "failed",
    }
  }
}

/// The work of the sort a failure stopped, which the owner is told is skipped once it fails twice: cutting a book,
/// sorting a group, writing a room sheet over the ceiling again, walking a stretch of a fixed heading, and a fixed
/// heading still past its ceiling after its walk, the last two by the heading's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
  Cut,
  Group,
  Sheet,
  TrimStretch(&'static str),
  TrimHeading(&'static str),
}

impl Work {
  /// The work as the owner notice names it.
  pub fn code(self) -> &'static str {
    match self {
      Self::Cut => "cut",
      Self::Group => "group",
      Self::Sheet => "sheet",
      Self::TrimStretch(_) => "trim-stretch",
      Self::TrimHeading(_) => "trim-heading",
    }
  }

  /// The fixed heading a trim's work is about.
  pub fn heading(self) -> Option<&'static str> {
    match self {
      Self::TrimStretch(heading) | Self::TrimHeading(heading) => Some(heading),
      Self::Cut | Self::Group | Self::Sheet => None,
    }
  }
}

/// What the owner is told of a work the sort now skips: what it was, whose book and which part, and why.
pub fn sort_notice(
  work: Work,
  user_id: i64,
  chat_id: Option<i64>,
  title: Option<&str>,
  cause: &str,
) -> OwnerNotice {
  let mut params = Map::new();

  params.insert("work".to_owned(), json!(work.code()));
  params.insert("heading".to_owned(), work.heading().map_or(Value::Null, |heading| json!(heading)));
  params.insert("person".to_owned(), json!(user_id));
  params.insert("chatId".to_owned(), chat_id.map_or(Value::Null, |chat_id| json!(chat_id)));
  params.insert("title".to_owned(), title.map_or(Value::Null, |title| json!(title)));
  params.insert("cause".to_owned(), json!(cause));

  OwnerNotice {
    code: PEOPLE_SORT_FAILED.to_owned(),
    params,
  }
}

fn read_text(root: &Path, path: &str) -> String {
  fs::read_to_string(root.join(path)).unwrap_or_default()
}

/// What a failure is remembered by: the kind, the room, the person, and a hash of what it was done to.
pub fn failure_key(kind: &str, chat_id: i64, user_id: i64, material: &str) -> String {
  format!("{kind}:{chat_id}:{user_id}:{:x}", js::bun_hash(material))
}

fn group_failure_key(chat_id: i64, user_id: i64, group: &[PersonPiece]) -> String {
  let material = group
    .iter()
    .map(|piece| format!("{}\n{}", piece.path, piece.text))
    .collect::<Vec<_>>()
    .join("\n\u{0}\n");

  failure_key("group", chat_id, user_id, &material)
}

// What a cut can change of one person's room book: the file at the sheet's place and the pieces.
fn cut_state(root: &Path, chat_id: i64, user_id: i64) -> String {
  let dir = people_dir_of(chat_id);

  format!(
    "{}|{}",
    js::units(&read_text(root, &format!("{dir}/{user_id}.md"))),
    room_piece_slugs(root, &dir, user_id).join(",")
  )
}

fn cut_failure_key(root: &Path, chat_id: i64, user_id: i64) -> String {
  let file = read_text(root, &format!("{}/{user_id}.md", people_dir_of(chat_id)));

  failure_key(
    "cut",
    chat_id,
    user_id,
    &format!("{file}\n\u{0}\n{}", cut_state(root, chat_id, user_id)),
  )
}

// A room sheet's rewrite is remembered by the file at its place and the pieces it is written from.
fn sheet_failure_key(root: &Path, chat_id: i64, user_id: i64) -> String {
  let mut parts = vec![read_text(root, &format!("{}/{user_id}.md", people_dir_of(chat_id)))];

  parts.extend(
    room_pieces(root, chat_id, user_id)
      .into_iter()
      .map(|piece| format!("{}\n{}", piece.path, piece.text)),
  );

  failure_key("sheet", chat_id, user_id, &parts.join("\n\u{0}\n"))
}

/// How many times the work a key fingerprints has failed before.
pub trait FailureCounts {
  fn count(&mut self, key: &str) -> impl Future<Output = Result<i64, Failure>>;
}

/// The failures of a sort remembered between its runs, in the brain's settings.
pub struct Failures<'d> {
  pub db: &'d DatabaseConnection,
}

impl FailureCounts for Failures<'_> {
  async fn count(&mut self, key: &str) -> Result<i64, Failure> {
    Failures::count(self, key).await
  }
}

impl Failures<'_> {
  pub async fn count(&self, key: &str) -> Result<i64, Failure> {
    Ok(
      settings::get(self.db, &format!("{FAILURE_PREFIX}{key}"))
        .await?
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0),
    )
  }

  pub async fn record(&self, key: &str, count: i64) -> Result<(), Failure> {
    Ok(settings::set(self.db, &format!("{FAILURE_PREFIX}{key}"), &count.to_string()).await?)
  }

  pub async fn clear(&self, key: &str) -> Result<(), Failure> {
    Ok(settings::forget(self.db, &format!("{FAILURE_PREFIX}{key}")).await?)
  }
}

/// The rooms whose books hold anything of this person: a room piece or a file at the room sheet's place.
pub fn rooms_of(root: &Path, user_id: i64) -> Vec<i64> {
  let Ok(entries) = fs::read_dir(root.join(ROOMS_ROOT)) else { return vec![] };
  let mut rooms: Vec<i64> = entries
    .flatten()
    .filter_map(|entry| room_id(&entry.file_name().to_string_lossy()))
    .filter(|chat_id| {
      let dir = people_dir_of(*chat_id);

      !room_piece_slugs(root, &dir, user_id).is_empty()
        || root.join(&dir).join(format!("{user_id}.md")).exists()
    })
    .collect();

  rooms.sort_unstable();
  rooms
}

/// The rooms whose books hold anything of this person and keep a room part: a one-to-one with the agent keeps none,
/// and what stands there is left as it is.
pub fn kept_rooms_of(root: &Path, user_id: i64, unkept: &BTreeSet<i64>) -> Vec<i64> {
  rooms_of(root, user_id)
    .into_iter()
    .filter(|chat_id| !unkept.contains(chat_id))
    .collect()
}

/// A room directory's name read as its chat id: a run of digits with an optional minus.
pub fn room_id(name: &str) -> Option<i64> {
  let digits = name.strip_prefix('-').unwrap_or(name);

  (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())).then(|| name.parse().ok())?
}

/// Everyone any room's book holds, by the ids its pieces and sheets carry.
pub fn people_in_books(root: &Path) -> Vec<i64> {
  let mut ids = BTreeSet::new();
  let Ok(rooms) = fs::read_dir(root.join(ROOMS_ROOT)) else { return vec![] };

  for room in rooms.flatten() {
    let dir = room.path().join("people");

    for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
      let name = entry.file_name().to_string_lossy().into_owned();

      if let Some(id) = name
        .strip_suffix(".md")
        .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
      {
        ids.extend(id.parse::<i64>().ok());
      }
    }

    for entry in fs::read_dir(dir.join(CHUNKS_DIR)).into_iter().flatten().flatten() {
      let name = entry.file_name().to_string_lossy().into_owned();

      if let Some(rest) = name.strip_prefix('p').filter(|_| name.ends_with(".md")) {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();

        if !digits.is_empty() && rest[digits.len()..].starts_with('-') {
          ids.extend(digits.parse::<i64>().ok());
        }
      }
    }
  }

  ids.into_iter().collect()
}

/// Whether this person's book in this room must be cut before it can be sorted: a book still being cut, or a sheet
/// standing with no piece under it.
pub fn needs_cut(root: &Path, chat_id: i64, user_id: i64) -> bool {
  let dir = people_dir_of(chat_id);

  if room_book_being_cut(root, &dir, user_id) {
    return true;
  }

  if !room_piece_slugs(root, &dir, user_id).is_empty() {
    return false;
  }

  fs::read_to_string(root.join(&dir).join(format!("{user_id}.md")))
    .is_ok_and(|text| !js::trim(&text).is_empty())
}

/// Whether this person's room sheet here is over the room ceiling and can be written again: pieces stand to write
/// it from, and the file is not a book still being cut.
pub fn sheet_over_ceiling(root: &Path, chat_id: i64, user_id: i64) -> bool {
  let dir = people_dir_of(chat_id);

  js::units(js::trim(&read_text(root, &format!("{dir}/{user_id}.md")))) > BRAIN.sheets.room_ceiling
    && !room_piece_slugs(root, &dir, user_id).is_empty()
    && !room_book_being_cut(root, &dir, user_id)
}

fn room_piece(root: &Path, dir: &str, slug: &str) -> PersonPiece {
  let path = format!("{dir}/{CHUNKS_DIR}/{slug}.md");

  PersonPiece {
    text: read_text(root, &path),
    path,
  }
}

fn room_pieces(root: &Path, chat_id: i64, user_id: i64) -> Vec<PersonPiece> {
  let dir = people_dir_of(chat_id);

  room_piece_slugs(root, &dir, user_id)
    .iter()
    .map(|slug| room_piece(root, &dir, slug))
    .collect()
}

/// This person's room pieces in this room not on the sorted list, in the order the room index names them.
pub fn unsorted_pieces(root: &Path, chat_id: i64, user_id: i64) -> Vec<PersonPiece> {
  let dir = people_dir_of(chat_id);
  let sorted = sorted_slugs(root, &dir);
  let index = read_text(root, &format!("{dir}/index.md"));
  let at = |slug: &str| index.find(&format!("- {slug} ")).unwrap_or(usize::MAX);
  let mut slugs: Vec<String> = room_piece_slugs(root, &dir, user_id)
    .into_iter()
    .filter(|slug| !sorted.contains(slug))
    .collect();

  slugs.sort_by(|a, b| at(a).cmp(&at(b)).then_with(|| a.cmp(b)));
  slugs.iter().map(|slug| room_piece(root, &dir, slug)).collect()
}

/// The pieces in the groups one run each takes on, in order; a piece larger than a group is a group of its own.
pub fn sort_groups(pieces: &[PersonPiece], target: usize) -> Vec<Vec<PersonPiece>> {
  let mut groups = vec![];
  let mut group: Vec<PersonPiece> = vec![];
  let mut held = 0;

  for piece in pieces {
    let size = js::units(&piece.text);

    if !group.is_empty() && held + size > target {
      groups.push(std::mem::take(&mut group));
      held = 0;
    }

    group.push(piece.clone());
    held += size;
  }

  if !group.is_empty() {
    groups.push(group);
  }

  groups
}

// An item line of a piece: its bracket and the text after it.
fn item_line(line: &str) -> Option<(&str, &str)> {
  let rest = line.strip_prefix('-')?.trim_start();
  let inside = rest.strip_prefix('[')?;
  let close = inside.find(']')?;
  let text = inside[close + 1..].trim_start();

  (!text.is_empty()).then_some((&inside[..close], text))
}

/// The characters of the common items that stood before a run and still stand after it with the same text and a
/// different bracket: a statement folded into an item already there.
pub fn reconfirmed_chars(before: &[PersonPiece], after: &[PersonPiece]) -> usize {
  let brackets = |pieces: &[PersonPiece]| -> HashMap<String, BTreeSet<String>> {
    let mut found: HashMap<String, BTreeSet<String>> = HashMap::new();

    for piece in pieces {
      for line in piece.text.split('\n') {
        if let Some((bracket, text)) = item_line(js::trim(line)) {
          found.entry(text.to_owned()).or_default().insert(bracket.to_owned());
        }
      }
    }

    found
  };
  let was = brackets(before);

  brackets(after)
    .into_iter()
    .filter(|(text, now)| {
      was.get(text).is_some_and(|then| now.iter().any(|bracket| !then.contains(bracket)))
    })
    .map(|(text, _)| said_length(&text))
    .sum()
}

/// How much a piece says: the characters of its statements, headings, blank lines, list marks and date brackets
/// left out, Hangul counted as `said_length` counts it.
pub fn statement_chars(text: &str) -> usize {
  text
    .split('\n')
    .map(js::trim)
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
    .map(|line| {
      let statement = match line.strip_prefix('-') {
        None => line,
        Some(rest) => {
          let rest = rest.trim_start();

          match rest
            .strip_prefix('[')
            .and_then(|inside| inside.find(']').map(|close| &inside[close + 1..]))
          {
            Some(after) => after.trim_start(),
            None => rest,
          }
        }
      };

      said_length(statement)
    })
    .sum()
}

fn total(pieces: &[PersonPiece]) -> usize {
  pieces.iter().map(|piece| statement_chars(&piece.text)).sum()
}

/// What is left to sort, a line per fixed heading to trim and per person per room, and the people it is left for;
/// work skipped after two failures is named as skipped and leaves nothing for its person.
pub async fn sort_status(
  root: &Path,
  failures: &mut impl FailureCounts,
  unkept: &BTreeSet<i64>,
) -> Result<(Vec<String>, Vec<i64>), Failure> {
  let mut lines = vec![];
  let mut people = vec![];

  for user_id in people_in_books(root) {
    let mut left = false;

    for trim in trims_of(root, user_id, unkept) {
      let (line, work) = trim_status(root, failures, &trim).await?;

      lines.push(line);
      left |= work;
    }

    for chat_id in kept_rooms_of(root, user_id, unkept) {
      let cut = needs_cut(root, chat_id, user_id);
      let unsorted = unsorted_pieces(root, chat_id, user_id);
      let pieces = room_piece_slugs(root, &people_dir_of(chat_id), user_id).len();
      let groups = sort_groups(&unsorted, GROUP_CHARS);
      let skipped = if cut {
        failures.count(&cut_failure_key(root, chat_id, user_id)).await? >= SKIP_AFTER
      } else {
        match groups.first() {
          Some(group) => failures.count(&group_failure_key(chat_id, user_id, group)).await? >= SKIP_AFTER,
          None => false,
        }
      };
      let over = sheet_over_ceiling(root, chat_id, user_id);
      let sheet_skipped =
        over && failures.count(&sheet_failure_key(root, chat_id, user_id)).await? >= SKIP_AFTER;

      if ((cut || !unsorted.is_empty()) && !skipped) || (over && !sheet_skipped) {
        left = true;
      }

      lines.push(format!(
        "{user_id} in {chat_id}: {}{}{}{} of {pieces} room pieces unsorted; {} common pieces",
        if skipped { "skipped after two failures until its pieces change; " } else { "" },
        if over {
          format!(
            "sheet over the ceiling{}; ",
            if sheet_skipped { ", not written again after two failures" } else { "" }
          )
        } else {
          String::new()
        },
        if cut { "to be cut first; " } else { "" },
        unsorted.len(),
        common_pieces(root, user_id).len()
      ));
    }

    if left {
      people.push(user_id);
    }
  }

  Ok((lines, people))
}

/// What a run of the sort's writer is given beyond its prompt: the room of the group, where it may write, what it
/// may reach, and the brain as it found it.
pub struct SortDesk<'d> {
  pub room: i64,
  pub places: Vec<String>,
  pub reach: Vec<FragmentFilter>,
  pub before: &'d DocumentBackup,
  pub named: &'d mut BTreeSet<PathBuf>,
  /// What the run's log says once it stands.
  pub done: &'d str,
}

/// What sorting stands on besides the folder it sorts, so it is run and tested apart from the process around it:
/// the agent whose book it is, the rooms' identifiers and names, the rest, the folder's lock, one run of the writer,
/// one stretch of a book cut, both standing sheets of a person written again, what a group that stood wrote
/// followed into the table and committed, the items' states, and the failures it remembers between runs and tells
/// the owner of.
pub trait SortRoom: FailureCounts {
  /// The folder's editing lock, let go when dropped.
  type Held;

  /// The folder of the agent whose book is sorted.
  fn root(&self) -> &Path;
  /// The agent whose book it is, by its registration name.
  fn agent(&self) -> &str;
  /// The rooms of the folder that keep no room part, left as they stand.
  fn unkept(&self) -> &BTreeSet<i64>;
  /// Where the writer's task is read from.
  fn prompts(&self) -> Prompts<'_>;
  /// What the agent's task calls it.
  fn label(&self) -> &str;
  /// Where the whole account of the sort's runs is, which a failure's cause names where it cannot say more.
  fn journal(&self) -> String;
  /// The names the fragment tool's checks read.
  fn names(&mut self) -> impl Future<Output = Result<Names, Failure>>;
  fn known(&mut self) -> impl Future<Output = Result<KnownIdentifiers, Failure>>;
  fn title(&mut self, chat_id: i64) -> impl Future<Output = Result<Option<String>, Failure>>;
  /// The name the person's latest message went under, which heads their common book.
  fn display_name(&mut self, user_id: i64) -> impl Future<Output = Result<Option<String>, Failure>>;
  fn resting(&mut self) -> impl Future<Output = Result<bool, Failure>>;
  fn lock(&mut self) -> impl Future<Output = Result<Option<Self::Held>, Failure>>;
  fn run_editor(
    &mut self,
    chat_id: i64,
    prompt: &str,
    desk: SortDesk<'_>,
  ) -> impl Future<Output = Result<Wrote, Stopped>>;
  /// One stretch of this person's room book cut into pieces: whether the cut failed.
  fn cut_stretch(&mut self, chat_id: i64, user_id: i64) -> impl Future<Output = Result<bool, Stopped>>;
  /// Both standing sheets of this person, from the pieces standing now.
  fn write_sheets(&mut self, chat_id: i64, user_id: i64) -> impl Future<Output = Result<(), Failure>>;
  /// What a group or a stretch that stood, a sheet rewrite or a heading set aside wrote: its row writes applied,
  /// the documents written cleaned and announced, and this person's part in that room and their common part
  /// committed under the message.
  fn finish(
    &mut self,
    chat_id: i64,
    user_id: i64,
    written: &[String],
    message: &str,
    writes: &[FragmentWrite],
  ) -> impl Future<Output = Result<(), Failure>>;
  /// Whether each of these item numbers is finished, as far as bmo-job knows them.
  fn item_states(&mut self, ids: &[i64]) -> impl Future<Output = Result<HashMap<i64, bool>, Failure>>;
  fn record(&mut self, key: &str, count: i64) -> impl Future<Output = Result<(), Failure>>;
  fn clear(&mut self, key: &str) -> impl Future<Output = Result<(), Failure>>;
  fn notify(&mut self, notice: OwnerNotice) -> impl Future<Output = ()>;
}

/// One more failure counted; on the failure that makes the work skipped, the owner is told once. Answers whether the
/// work is now skipped. The work stands in a room's part of the book, or in the common part where no room is named.
pub async fn note_failure<R: SortRoom>(
  room: &mut R,
  key: &str,
  work: Work,
  chat_id: Option<i64>,
  user_id: i64,
  cause: &str,
) -> Result<bool, Failure> {
  let count = room.count(key).await? + 1;

  room.record(key, count).await?;

  if count != SKIP_AFTER {
    return Ok(count > SKIP_AFTER);
  }

  let title = match chat_id {
    Some(chat_id) => room.title(chat_id).await?,
    None => None,
  };

  room.notify(sort_notice(work, user_id, chat_id, title.as_deref(), cause)).await;

  Ok(true)
}

/// The lock, taken for one more run of a sort; none where the runs it was given are spent, the agents rest or the
/// lock cannot be had.
pub async fn next_run_lock<R: SortRoom>(room: &mut R, budget: usize) -> Result<Option<R::Held>, Failure> {
  if budget == 0 || room.resting().await? {
    return Ok(None);
  }

  room.lock().await
}

/// Before any group runs: both standing sheets of every one of these people whose room sheet is over the ceiling and
/// can be written again, each under the lock and committed on its own. A sheet the sheet judgement could not bring
/// under the ceiling is remembered like a group's failure. Answers whether it stopped for a rest or a lock not had.
pub async fn refresh_over_ceiling_sheets<R: SortRoom>(
  room: &mut R,
  people: &[i64],
) -> Result<bool, Stopped> {
  let root = room.root().to_path_buf();
  let unkept = room.unkept().clone();

  for user_id in people {
    for chat_id in kept_rooms_of(&root, *user_id, &unkept) {
      if !sheet_over_ceiling(&root, chat_id, *user_id) {
        continue;
      }

      let key = sheet_failure_key(&root, chat_id, *user_id);

      if room.count(&key).await? >= SKIP_AFTER {
        println!(
          "people sort: person {user_id} in chat {chat_id} keeps its sheet over the ceiling; writing it failed twice as it stands"
        );
        continue;
      }

      if room.resting().await? {
        return Ok(true);
      }

      let Some(held) = room.lock().await? else { return Ok(true) };
      let backup = backup_documents(&root);
      let written = match room.write_sheets(chat_id, *user_id).await {
        Ok(()) => {
          let touched = run_touched(
            &root,
            &backup,
            &RunClaim {
              places: vec![people_dir_of(chat_id), common_dir_of(*user_id)],
              named: BTreeSet::new(),
            },
          );
          let paths: Vec<String> = touched.changed.into_iter().chain(touched.removed).collect();

          if paths.is_empty() {
            Ok(())
          } else {
            room
              .finish(
                chat_id,
                *user_id,
                &paths,
                &format!("Person {user_id}'s standing sheets in room {chat_id} are written again under the room ceiling"),
                &[],
              )
              .await
          }
        }
        Err(why) => Err(why),
      };

      drop(held);
      written?;

      if !sheet_over_ceiling(&root, chat_id, *user_id) {
        room.clear(&key).await?;
        println!("people sort: person {user_id} in chat {chat_id} has its sheet under the ceiling again");
        continue;
      }

      note_failure(
        room,
        &key,
        Work::Sheet,
        Some(chat_id),
        *user_id,
        &format!(
          "The sheet judgement could not write that person's sheet for the room within its {}-character ceiling.",
          BRAIN.sheets.room_ceiling
        ),
      )
      .await?;
    }
  }

  Ok(false)
}

/// One person's book sorted in every room, cutting first where a book must be cut, until nothing of theirs is left
/// or the budget of runs is spent: done with nothing left, stopped for a spent budget or a rest, failed for a run that
/// did not stand, which stops this person with the book as it was before that run.
pub async fn sort_person<R: SortRoom>(
  room: &mut R,
  user_id: i64,
  budget: &mut usize,
) -> Result<Ended, Stopped> {
  let root = room.root().to_path_buf();
  let unkept = room.unkept().clone();

  'rooms: for chat_id in kept_rooms_of(&root, user_id, &unkept) {
    // Cut first, a stretch a run, while the book in this room is not yet in pieces.
    while needs_cut(&root, chat_id, user_id) {
      let key = cut_failure_key(&root, chat_id, user_id);

      if room.count(&key).await? >= SKIP_AFTER {
        println!(
          "people sort: person {user_id} in chat {chat_id} is skipped; its cut failed twice on this book"
        );
        continue 'rooms;
      }

      let Some(held) = next_run_lock(room, *budget).await? else { return Ok(Ended::Stopped) };
      let was = cut_state(&root, chat_id, user_id);

      *budget -= 1;

      let cut = room.cut_stretch(chat_id, user_id).await;

      drop(held);

      let failed = cut?;
      // A cut that says it did not fail and left the book exactly as it was would be asked again for ever.
      let unmoved = cut_state(&root, chat_id, user_id) == was;

      if failed || unmoved {
        eprintln!("people sort: person {user_id} in chat {chat_id} could not be cut");

        let cause = if failed {
          format!("The run that cuts the book into pieces failed. {}", unknown_cause(&room.journal()))
        } else {
          "The run that cuts the book into pieces ended without changing the book, so asking it again would change nothing either."
            .to_owned()
        };

        if note_failure(room, &key, Work::Cut, Some(chat_id), user_id, &cause).await? {
          continue 'rooms;
        }

        return Ok(Ended::Failed);
      }

      room.clear(&key).await?;
    }

    while let Some(group) =
      sort_groups(&unsorted_pieces(&root, chat_id, user_id), GROUP_CHARS).into_iter().next()
    {
      let key = group_failure_key(chat_id, user_id, &group);

      if room.count(&key).await? >= SKIP_AFTER {
        println!(
          "people sort: person {user_id} in chat {chat_id} is skipped; this group failed twice as it stands"
        );
        continue 'rooms;
      }

      let Some(held) = next_run_lock(room, *budget).await? else { return Ok(Ended::Stopped) };

      *budget -= 1;

      let sorted = sort_group(room, chat_id, user_id, &group).await;

      drop(held);

      match sorted? {
        None => room.clear(&key).await?,
        Some(cause) => {
          if note_failure(room, &key, Work::Group, Some(chat_id), user_id, &cause).await? {
            continue 'rooms;
          }

          return Ok(Ended::Failed);
        }
      }
    }
  }

  Ok(Ended::Done)
}

// What a group stood on before its run: the person's room pieces here and their common pieces.
struct Before<'b> {
  room: &'b [PersonPiece],
  common: &'b [PersonPiece],
}

// What a sorted group's checks read besides the folder: the group, what stood before it, the brain as the run found
// it, what a common document may name, the run's row writes and the files its own tools named, and the brain and
// the names the row checks read.
struct GroupCheck<'c> {
  chat_id: i64,
  user_id: i64,
  group: &'c [PersonPiece],
  before: Before<'c>,
  backup: &'c DocumentBackup,
  known: &'c KnownIdentifiers,
  rows: &'c [FragmentWrite],
  named: &'c BTreeSet<PathBuf>,
  brain: &'c Brain,
  names: &'c Names,
}

// Why a sorted group cannot stand, or nothing when it can, with the cause a person reads: a write outside the room's
// book and the person's common part, a common document naming anybody else, a room piece outside the group changed,
// a piece written without its row, or less of the group accounted for than the floor.
fn group_problems(root: &Path, check: &GroupCheck<'_>) -> (Vec<String>, String) {
  let places = vec![people_dir_of(check.chat_id), common_dir_of(check.user_id)];
  let claim = RunClaim {
    places: places.clone(),
    named: check.named.clone(),
  };
  let writes = write_problems(root, check.backup, &claim, check.known, None);
  let mut problems = writes.clone();
  let group_paths: BTreeSet<&str> = check.group.iter().map(|piece| piece.path.as_str()).collect();
  let room_after = room_pieces(root, check.chat_id, check.user_id);
  let common_after = common_pieces(root, check.user_id);

  for piece in check.before.room {
    let after = room_after.iter().find(|one| one.path == piece.path).map(|one| one.text.as_str());

    if !group_paths.contains(piece.path.as_str()) && after != Some(piece.text.as_str()) {
      problems.push(format!("changed {}, which is not in the group", piece.path));
    }
  }

  let touched = run_touched(root, check.backup, &claim);
  let allowed = |path: &str| places.iter().any(|place| path.starts_with(&format!("{place}/")));
  let body = |path: &str| fs::read_to_string(root.join(path)).ok();
  let seen = |path: &str| path.to_owned();
  let unlined = row_problems(
    check.brain,
    &Writing {
      writes: check.rows,
      changed: &touched.changed,
      allowed: &allowed,
      body: &body,
      seen: &seen,
    },
    check.names,
  );

  problems.extend(unlined.iter().cloned());

  let group_chars = total(check.group) as i64;
  let outside_group = total(check.before.room) as i64 - group_chars;
  let kept = total(&room_after) as i64 - outside_group
    + (total(&common_after) as i64 - total(check.before.common) as i64)
    + reconfirmed_chars(check.before.common, &common_after) as i64;
  let shrunk = (kept as f64) < group_chars as f64 * SORT_FIDELITY_FLOOR;

  if shrunk {
    problems.push(format!(
      "the group's {group_chars} characters of statements come back as {kept}, which is a summary rather than a sort"
    ));
  }

  let changed_elsewhere = problems[writes.len()..]
    .iter()
    .filter(|one| one.ends_with("which is not in the group"))
    .count();
  let mut parts = vec![];

  if !writes.is_empty() {
    parts.push(write_problems_cause(&writes));
  }

  if changed_elsewhere > 0 {
    parts.push(format!(
      "The run changed {changed_elsewhere} piece(s) that were not in the group it was given."
    ));
  }

  if !unlined.is_empty() {
    parts.push(format!(
      "{} of the rows it wrote through the fragment tool did not match what it wrote on disk.",
      unlined.len()
    ));
  }

  if shrunk {
    parts.push(format!(
      "The group's {group_chars} characters of statements came back as {kept}: the run summarised them instead of sorting them."
    ));
  }

  (
    problems,
    format!(
      "What the sorting run wrote did not pass the checks on a sorted group, so it was put back. {}",
      parts.join(" ")
    ),
  )
}

// One group, under the lock the caller holds: the run, the checks, and on a run that stands the sorted list, the
// sheets and what was written followed. Answers why the group did not stand, as a person reads it, or none when it did.
async fn sort_group<R: SortRoom>(
  room: &mut R,
  chat_id: i64,
  user_id: i64,
  group: &[PersonPiece],
) -> Result<Option<String>, Stopped> {
  let root = room.root().to_path_buf();
  let people_dir = people_dir_of(chat_id);
  let before_room = room_pieces(&root, chat_id, user_id);
  let before_common = common_pieces(&root, user_id);
  let backup = backup_documents(&root);
  let places = vec![people_dir.clone(), common_dir_of(user_id)];
  let slugs_before: BTreeSet<String> =
    before_room.iter().map(|piece| piece_slug(&piece.path).to_owned()).collect();
  let mut named = BTreeSet::new();
  let known = room.known().await?;
  let names = room.names().await?;
  let title = room.title(chat_id).await?;
  let display = room.display_name(user_id).await?;
  let prompt = sort_prompt(
    room.prompts(),
    room.label(),
    chat_id,
    title.as_deref(),
    user_id,
    display.as_deref(),
    group,
    &known.titles,
  )
  .await?;
  // The run reaches this person's room part here and their common part, and writes nowhere else.
  let reach = vec![
    FragmentFilter {
      kind: Some("person-book".to_owned()),
      chat_id: Some(Some(chat_id)),
      owners: Some(vec![user_id]),
      active: Some(true),
      ..FragmentFilter::default()
    },
    FragmentFilter {
      kind: Some("person-book".to_owned()),
      chat_id: Some(None),
      owners: Some(vec![user_id]),
      active: Some(true),
      ..FragmentFilter::default()
    },
  ];
  let ran = room
    .run_editor(
      chat_id,
      &prompt,
      SortDesk {
        room: chat_id,
        places: places.clone(),
        reach,
        before: &backup,
        named: &mut named,
        done: &format!("people sort: chat {chat_id} sorted a group"),
      },
    )
    .await;
  let claim = RunClaim {
    places: places.clone(),
    named: named.clone(),
  };
  let ran = match ran {
    Ok(ran) => ran,
    Err(stopped) => {
      restore_documents(&root, &backup, &claim);
      return Err(stopped);
    }
  };
  let (problems, cause, writes) = match ran {
    Wrote::Failed(reason) => (vec![reason.clone()], reason, vec![]),
    Wrote::Stood(writes) => {
      let brain = Brain::new(root.clone(), room.agent(), vec![]);
      let (problems, cause) = group_problems(
        &root,
        &GroupCheck {
          chat_id,
          user_id,
          group,
          before: Before {
            room: &before_room,
            common: &before_common,
          },
          backup: &backup,
          known: &known,
          rows: &writes,
          named: &named,
          brain: &brain,
          names: &names,
        },
      );

      (problems, cause, writes)
    }
  };

  if !problems.is_empty() {
    restore_documents(&root, &backup, &claim);
    eprintln!(
      "people sort: person {user_id} in chat {chat_id}, group {}: {}; put back",
      group.iter().map(|piece| piece_slug(&piece.path)).collect::<Vec<_>>().join(", "),
      problems.iter().take(5).cloned().collect::<Vec<_>>().join("; ")
    );

    return Ok(Some(cause));
  }

  let group_slugs: Vec<String> = group.iter().map(|piece| piece_slug(&piece.path).to_owned()).collect();
  let made: Vec<String> = room_piece_slugs(&root, &people_dir, user_id)
    .into_iter()
    .filter(|slug| !slugs_before.contains(slug))
    .collect();

  mark_sorted(&root, &people_dir, &[group_slugs.clone(), made].concat());

  // A person with no room piece left has no room sheet either: a sheet standing with nothing under it would read as
  // a book still to cut.
  if room_piece_slugs(&root, &people_dir, user_id).is_empty() {
    let _ = fs::remove_file(root.join(&people_dir).join(format!("{user_id}.md")));
  }

  room.write_sheets(chat_id, user_id).await?;

  let touched = run_touched(&root, &backup, &claim);
  let paths: Vec<String> = touched.changed.into_iter().chain(touched.removed).collect();

  room
    .finish(
      chat_id,
      user_id,
      &paths,
      &format!("Person {user_id}'s book in room {chat_id} is sorted into its common and room-only parts by one more group"),
      &writes,
    )
    .await?;
  println!("people sort: person {user_id} in chat {chat_id} sorted {} pieces", group_slugs.len());

  Ok(None)
}

/// Sorting the room pieces not yet sorted, a stretch or a group a run, each under the lock and let go before the
/// next; the fixed headings past their ceiling are trimmed first, then the room sheets over the ceiling are written
/// again. Answers whether any person's sort or trim failed.
pub async fn sort_people<R: SortRoom>(
  room: &mut R,
  people: &[i64],
  mut budget: usize,
) -> Result<bool, Stopped> {
  let mut failed = false;

  // Every batch carries the fixed headings whole, so the ones past their ceiling are trimmed before anything else.
  for user_id in people {
    match trim_person(room, *user_id, &mut budget).await? {
      Ended::Done => {}
      Ended::Stopped => return Ok(failed),
      Ended::Failed => failed = true,
    }
  }

  if refresh_over_ceiling_sheets(room, people).await? {
    return Ok(failed);
  }

  for user_id in people {
    let ended = sort_person(room, *user_id, &mut budget).await?;

    println!("people sort: person {user_id} {}", ended.word());

    if ended == Ended::Failed {
      failed = true;
    }

    if budget == 0 || room.resting().await? {
      break;
    }
  }

  Ok(failed)
}

impl FailureCounts for &PeopleDigest<'_> {
  async fn count(&mut self, key: &str) -> Result<i64, Failure> {
    Failures { db: self.ground.db }.count(key).await
  }
}

impl SortRoom for &PeopleDigest<'_> {
  type Held = HeldLock;

  fn root(&self) -> &Path {
    self.brain()
  }

  fn agent(&self) -> &str {
    &self.recollection.agent
  }

  fn unkept(&self) -> &BTreeSet<i64> {
    &self.unkept_rooms
  }

  fn prompts(&self) -> Prompts<'_> {
    self.ground.prompts()
  }

  fn label(&self) -> &str {
    &self.recollection.label
  }

  fn journal(&self) -> String {
    self.ground.record()
  }

  async fn names(&mut self) -> Result<Names, Failure> {
    let ground = &self.ground;

    Ok(table_names(
      &Brain::of(ground.agency, ground.brain_root, ground.chat_key).await?,
      ground.chat_key,
      ground.owner_names,
    ))
  }

  async fn known(&mut self) -> Result<KnownIdentifiers, Failure> {
    known_identifiers(self.ground.agency, self.ground.receiver).await
  }

  async fn title(&mut self, chat_id: i64) -> Result<Option<String>, Failure> {
    chat_title(self.ground.receiver, chat_id).await
  }

  async fn display_name(&mut self, user_id: i64) -> Result<Option<String>, Failure> {
    self.display_name_of(user_id).await
  }

  async fn resting(&mut self) -> Result<bool, Failure> {
    resting(self.ground.agency).await
  }

  async fn lock(&mut self) -> Result<Option<HeldLock>, Failure> {
    PeopleDigest::lock(self, "people sort").await
  }

  async fn run_editor(&mut self, chat_id: i64, prompt: &str, desk: SortDesk<'_>) -> Result<Wrote, Stopped> {
    self
      .run_writer(
        chat_id,
        prompt,
        desk.done,
        WriterDesk {
          room: desk.room,
          places: desk.places,
          reach: desk.reach,
          beside: None,
          carried: vec![],
          before: desk.before,
          named: desk.named,
        },
        &people_dir_of(chat_id),
        None,
      )
      .await
  }

  async fn cut_stretch(&mut self, chat_id: i64, user_id: i64) -> Result<bool, Stopped> {
    self.split_chat(chat_id, Some(user_id), 1).await
  }

  async fn write_sheets(&mut self, chat_id: i64, user_id: i64) -> Result<(), Failure> {
    self.write_standing_sheets(chat_id, &[user_id], None, false, true).await
  }

  async fn finish(
    &mut self,
    chat_id: i64,
    user_id: i64,
    written: &[String],
    message: &str,
    writes: &[FragmentWrite],
  ) -> Result<(), Failure> {
    self.follow_writes(writes, chat_id).await?;

    let documents: Vec<String> = written.iter().filter(|path| path.ends_with(".md")).cloned().collect();

    self
      .settle_written(&documents, &[people_dir_of(chat_id), common_dir_of(user_id)], message)
      .await;

    Ok(())
  }

  async fn item_states(&mut self, ids: &[i64]) -> Result<HashMap<i64, bool>, Failure> {
    let mut unique: Vec<i64> = ids.to_vec();

    unique.sort_unstable();
    unique.dedup();

    if unique.is_empty() {
      return Ok(HashMap::new());
    }

    let states = self.ground.job.item_states(&unique).await?;

    Ok(states.into_iter().map(|state| (state.id, state.done)).collect())
  }

  async fn record(&mut self, key: &str, count: i64) -> Result<(), Failure> {
    Failures { db: self.ground.db }.record(key, count).await
  }

  async fn clear(&mut self, key: &str) -> Result<(), Failure> {
    Failures { db: self.ground.db }.clear(key).await
  }

  async fn notify(&mut self, notice: OwnerNotice) {
    match self.ground.receiver.notify_owner(&notice).await {
      Ok(true) => {}
      Ok(false) => eprintln!("people sort: the receiver did not take the notice of a skipped work"),
      Err(why) => eprintln!("people sort: telling the owner about a skipped work failed: {why}"),
    }
  }
}
