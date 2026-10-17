//! Trimming a fixed heading past its ceiling (docs/people-book.md, 「Trimming a fixed heading」): the heading's text
//! is set aside into a holding file beside its part's pieces, so every batch is handed a short heading at once, and
//! the holding file is walked a stretch a run: what is still open goes back into the heading, everything else into
//! the piece of its own matter. The holding file on disk is the whole state, so the walk stops anywhere and picks up
//! from there. A stretch is measured as a sorted group is, and a failure is remembered as the sort's are.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use brain_support::js;
use fragment_desk::desk::{FragmentFilter, FragmentWrite, Names};

use super::super::DocumentBackup;
use super::super::book::{
  CHUNKS_DIR, FIXED_CEILING_CHARS, FixedHeading, KnownIdentifiers, PersonPiece, ROOMS_ROOT, TRIMMING_DIR,
  common_dir_of, fixed_heading, people_dir_of, piece_slug,
};
use super::super::documents::{
  RunClaim, backup_documents, mark_sorted, restore_documents, run_touched, sorted_slugs, write_problems,
  write_problems_cause,
};
use super::super::fragments::libraries::Brain;
use super::super::fragments::{Writing, write_problems as row_problems};
use super::prompts::{TrimPrompt, trim_prompt};
use super::sort::{
  Ended, FailureCounts, SKIP_AFTER, SORT_FIDELITY_FLOOR, SortDesk, SortRoom, Work, failure_key,
  kept_rooms_of, next_run_lock, note_failure, reconfirmed_chars, room_id, statement_chars,
};
use super::{Stopped, Wrote};
use crate::Failure;

/// How much of a heading set aside one run walks.
pub const STRETCH_CHARS: usize = 16_000;

// The sections of a piece in the order the book writes them; any other follows in the order it first stands.
const SECTIONS: [&str; 3] = ["Facts", "Views", "Judgements"];

/// One item of a piece as the trim walks it: the section it stands under and its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
  pub section: String,
  pub text: String,
}

/// A piece read as its heading line and its items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
  pub heading: String,
  pub items: Vec<Item>,
}

/// A piece of the book's own shape read as its heading line and its items, a section's items in the order they
/// stand however many times the section's heading is written. A line under an item that is neither a heading nor
/// an item of its own carries that item on; one before any item is an item of its own.
pub fn listed(text: &str) -> Listed {
  let mut listed = Listed::default();
  let mut section = SECTIONS[0].to_owned();
  let mut open = false;

  for line in text.split('\n') {
    let trimmed = js::trim(line);

    if trimmed.is_empty() {
      open = false;
    } else if let Some(name) = trimmed.strip_prefix("## ") {
      section = js::trim(name).to_owned();
      open = false;
    } else if trimmed.starts_with("# ") && listed.heading.is_empty() && listed.items.is_empty() {
      trimmed.clone_into(&mut listed.heading);
    } else if trimmed.starts_with("- ") || !open {
      listed.items.push(Item {
        section: section.clone(),
        text: js::trim_end(line).to_owned(),
      });
      open = true;
    } else if let Some(last) = listed.items.last_mut() {
      last.text.push('\n');
      last.text.push_str(js::trim_end(line));
    }
  }

  listed
}

/// A piece written back out of its heading line and its items, each section once, the book's own first.
pub fn render(listed: &Listed) -> String {
  let mut sections: Vec<&str> = vec![];

  for item in &listed.items {
    if !sections.contains(&item.section.as_str()) {
      sections.push(&item.section);
    }
  }

  let rank = |name: &str| SECTIONS.iter().position(|one| *one == name).unwrap_or(SECTIONS.len());

  sections.sort_by_key(|name| rank(name));

  let mut text = listed.heading.clone();

  for name in sections {
    if !text.is_empty() {
      text.push_str("\n\n");
    }

    text.push_str(&format!("## {name}"));

    for item in listed.items.iter().filter(|item| item.section == name) {
      text.push('\n');
      text.push_str(&item.text);
    }
  }

  text.push('\n');
  text
}

/// How many of the first items one run walks: as many as fit the stretch, and always one.
pub fn stretch_len(items: &[Item], limit: usize) -> usize {
  let mut held = 0;
  let mut count = 0;

  for item in items {
    let size = js::units(&item.text);

    if count > 0 && held + size > limit {
      break;
    }

    held += size;
    count += 1;
  }

  count
}

/// One fixed heading of one person to trim: its part, the heading its slug names, and the room a run about it is
/// named under — the heading's own room, or for a common heading the first room the person's book stands in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trim {
  pub chat_id: Option<i64>,
  pub run_room: i64,
  pub heading: FixedHeading,
  pub slug: String,
}

impl Trim {
  /// The directory of the heading's part.
  pub fn dir(&self) -> String {
    match self.chat_id {
      Some(chat_id) => people_dir_of(chat_id),
      None => common_dir_of(self.heading.user_id),
    }
  }

  pub fn piece_path(&self) -> String {
    format!("{}/{CHUNKS_DIR}/{}.md", self.dir(), self.slug)
  }

  /// Where the heading's text waits while it is walked.
  pub fn holding_path(&self) -> String {
    format!("{}/{TRIMMING_DIR}/{}.md", self.dir(), self.slug)
  }

  /// Where a run lists, word for word, the items of its stretch it dropped as work states.
  pub fn dropped_path(&self) -> String {
    format!("{}/{TRIMMING_DIR}/{}.dropped.md", self.dir(), self.slug)
  }

  // Where a log line says the heading stands.
  fn place(&self) -> String {
    self
      .chat_id
      .map_or_else(|| "the common part".to_owned(), |chat_id| format!("chat {chat_id}"))
  }
}

fn read_text(root: &Path, path: &str) -> String {
  fs::read_to_string(root.join(path)).unwrap_or_default()
}

/// Whether a fixed heading's piece is past its ceiling.
pub fn over_ceiling(root: &Path, path: &str) -> bool {
  js::units(js::trim(&read_text(root, path))) > FIXED_CEILING_CHARS
}

// The fixed headings of this person under one part's directory with a holding file or past the ceiling.
fn trims_in(root: &Path, dir: &str, user_id: i64, chat_id: Option<i64>, run_room: i64) -> Vec<Trim> {
  let mut slugs = BTreeSet::new();

  for sub in [CHUNKS_DIR, TRIMMING_DIR] {
    for entry in fs::read_dir(root.join(dir).join(sub)).into_iter().flatten().flatten() {
      let name = entry.file_name().to_string_lossy().into_owned();

      if let Some(slug) = name.strip_suffix(".md") {
        slugs.insert(slug.to_owned());
      }
    }
  }

  slugs
    .into_iter()
    .filter_map(|slug| {
      let heading = fixed_heading(&slug)?;

      (heading.user_id == user_id && heading.common == chat_id.is_none()).then_some(Trim {
        chat_id,
        run_room,
        heading,
        slug,
      })
    })
    .filter(|trim| root.join(trim.holding_path()).exists() || over_ceiling(root, &trim.piece_path()))
    .collect()
}

/// This person's fixed headings to trim: their common ones first, then each room's that keeps a room part. A common
/// heading is trimmed only for a person some such room's book holds, since only a batch of one carries it.
pub fn trims_of(root: &Path, user_id: i64, unkept: &BTreeSet<i64>) -> Vec<Trim> {
  let rooms = kept_rooms_of(root, user_id, unkept);
  let mut trims = rooms
    .first()
    .map(|first| trims_in(root, &common_dir_of(user_id), user_id, None, *first))
    .unwrap_or_default();
  let mut chats: Vec<i64> = fs::read_dir(root.join(ROOMS_ROOT))
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(|entry| room_id(&entry.file_name().to_string_lossy()))
    .filter(|chat_id| !unkept.contains(chat_id))
    .collect();

  chats.sort_unstable();

  for chat_id in chats {
    trims.extend(trims_in(root, &people_dir_of(chat_id), user_id, Some(chat_id), chat_id));
  }

  trims
}

// A heading past its ceiling is remembered by its text; a stretch by the heading and the stretch itself.
fn piece_failure_key(root: &Path, trim: &Trim) -> String {
  failure_key(
    "trim-heading",
    trim.chat_id.unwrap_or(0),
    trim.heading.user_id,
    &format!("{}\n{}", trim.piece_path(), read_text(root, &trim.piece_path())),
  )
}

fn stretch_failure_key(trim: &Trim, stretch: &[Item]) -> String {
  let text = stretch.iter().map(|item| item.text.as_str()).collect::<Vec<_>>().join("\n");

  failure_key(
    "trim",
    trim.chat_id.unwrap_or(0),
    trim.heading.user_id,
    &format!("{}\n{text}", trim.piece_path()),
  )
}

/// Where a heading's trim stands, as the sort's status says it, and whether work is left in it for a run.
pub async fn trim_status<R: FailureCounts>(
  root: &Path,
  failures: &mut R,
  trim: &Trim,
) -> Result<(String, bool), Failure> {
  let holding = read_text(root, &trim.holding_path());

  if root.join(trim.holding_path()).exists() {
    let items = listed(&holding).items;

    return Ok((
      format!(
        "{} in {}: {} being trimmed, {} items ({} characters) left to walk",
        trim.heading.user_id,
        trim.place(),
        trim.slug,
        items.len(),
        js::units(&holding)
      ),
      true,
    ));
  }

  let skipped = failures.count(&piece_failure_key(root, trim)).await? >= SKIP_AFTER;

  Ok((
    format!(
      "{} in {}: {} over its {FIXED_CEILING_CHARS}-character ceiling at {} characters{}",
      trim.heading.user_id,
      trim.place(),
      trim.slug,
      js::units(js::trim(&read_text(root, &trim.piece_path()))),
      if skipped { ", left as it stands after two trims" } else { "" }
    ),
    !skipped,
  ))
}

// This person's pieces in the heading's part, the ones a trim run may write.
fn part_pieces(root: &Path, trim: &Trim) -> Vec<PersonPiece> {
  let chunks = format!("{}/{CHUNKS_DIR}", trim.dir());
  let prefix = format!("{}{}-", if trim.heading.common { 'c' } else { 'p' }, trim.heading.user_id);
  let mut names: Vec<String> = fs::read_dir(root.join(&chunks))
    .into_iter()
    .flatten()
    .flatten()
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| name.starts_with(&prefix) && name.ends_with(".md"))
    .collect();

  names.sort();
  names
    .into_iter()
    .map(|name| {
      let path = format!("{chunks}/{name}");

      PersonPiece {
        text: read_text(root, &path),
        path,
      }
    })
    .collect()
}

fn total(pieces: &[PersonPiece]) -> usize {
  pieces.iter().map(|piece| statement_chars(&piece.text)).sum()
}

fn write(root: &Path, path: &str, text: &str) -> Result<(), Failure> {
  let at = root.join(path);

  if let Some(dir) = at.parent() {
    fs::create_dir_all(dir).map_err(|why| unwritten(path, &why))?;
  }

  fs::write(&at, text).map_err(|why| unwritten(path, &why))
}

fn unwritten(path: &str, why: &std::io::Error) -> Failure {
  Failure::Disk(format!("the brain document {path} could not be written: {why}"))
}

// The heading's text moved into its holding file, its sections joined, and the piece left with its heading line and
// its row.
fn set_aside(root: &Path, trim: &Trim) -> Result<(), Failure> {
  let whole = listed(&read_text(root, &trim.piece_path()));

  write(root, &trim.holding_path(), &render(&whole))?;
  write(
    root,
    &trim.piece_path(),
    &render(&Listed {
      heading: whole.heading,
      items: vec![],
    }),
  )
}

// The holding file without its first items.
fn walked(root: &Path, trim: &Trim, count: usize) -> Result<(), Failure> {
  let mut holding = listed(&read_text(root, &trim.holding_path()));

  holding.items.drain(..count.min(holding.items.len()));
  write(root, &trim.holding_path(), &render(&holding))
}

// The first items of the holding file put back into the heading's piece as they stand, unwalked.
fn put_back(root: &Path, trim: &Trim, count: usize) -> Result<(), Failure> {
  let holding = listed(&read_text(root, &trim.holding_path()));
  let mut piece = listed(&read_text(root, &trim.piece_path()));

  if piece.heading.is_empty() {
    piece.heading.clone_from(&holding.heading);
  }

  piece.items.extend(holding.items.iter().take(count).cloned());
  write(root, &trim.piece_path(), &render(&piece))?;
  walked(root, trim, count)
}

/// How each item number a stretch names stands, as the run is told: finished, not finished, or unknown to the
/// agency where its answer leaves the item out.
pub fn item_states_text(states: &HashMap<i64, bool>, named: &[i64]) -> String {
  let lines: Vec<String> = named
    .iter()
    .map(|item| match states.get(item) {
      Some(true) => format!("- #{item}: finished"),
      Some(false) => format!("- #{item}: not finished"),
      None => format!("- #{item}: not in the TODO"),
    })
    .collect();

  if lines.is_empty() {
    "(the stretch names no TODO item)".to_owned()
  } else {
    lines.join("\n")
  }
}

// What a run's list of dropped items accounts for: the statements of the stretch's items it names word for word, each
// once, and the lines that are no item of the stretch.
fn dropped_items(list: &str, stretch: &[Item]) -> (usize, Vec<String>) {
  let mut left: Vec<&str> = stretch.iter().map(|item| js::trim(&item.text)).collect();
  let mut chars = 0;
  let mut unknown = vec![];

  for item in listed(list).items {
    let line = js::trim(&item.text);

    match left.iter().position(|one| *one == line) {
      Some(at) => {
        chars += statement_chars(left.remove(at));
      }
      None => unknown.push(line.chars().take(120).collect()),
    }
  }

  (chars, unknown)
}

// What a walked stretch's checks read besides the folder: the stretch, this person's pieces of the part before it,
// the brain as the run found it, what a common document may name, the run's row writes and the files its own tools
// named, and the brain and the names the row checks read.
struct StretchCheck<'c> {
  stretch: &'c [Item],
  before: &'c [PersonPiece],
  backup: &'c DocumentBackup,
  known: &'c KnownIdentifiers,
  rows: &'c [FragmentWrite],
  named: &'c BTreeSet<PathBuf>,
  brain: &'c Brain,
  names: &'c Names,
}

// Why a walked stretch cannot stand, or nothing when it can, with the cause a person reads: a write outside the
// person's pieces of the part and its dropped list, a common piece naming anybody else, a piece written without its
// row, a dropped line that is no item of the stretch, or less of the stretch accounted for than the floor.
fn stretch_problems(root: &Path, trim: &Trim, check: &StretchCheck<'_>) -> (Vec<String>, String) {
  let places = vec![trim.dir()];
  let claim = RunClaim {
    places: places.clone(),
    named: check.named.clone(),
  };
  let writes = write_problems(root, check.backup, &claim, check.known, Some(&[trim.heading.user_id]));
  let mut problems = writes.clone();
  let touched = run_touched(root, check.backup, &claim);
  let own = format!(
    "{}/{CHUNKS_DIR}/{}{}-",
    trim.dir(),
    if trim.heading.common { 'c' } else { 'p' },
    trim.heading.user_id
  );
  let dropped_path = trim.dropped_path();
  let strays: Vec<String> = touched
    .changed
    .iter()
    .chain(&touched.removed)
    .filter(|path| !path.starts_with(&own) && **path != dropped_path)
    .cloned()
    .collect();
  let (dropped_chars, unknown) = dropped_items(&read_text(root, &dropped_path), check.stretch);

  problems.extend(
    unknown
      .iter()
      .map(|line| format!("the dropped list holds a line that is no item of the stretch: {line}")),
  );
  problems.extend(strays.iter().map(|path| format!("changed {path}, which is not a piece of this person")));

  let allowed = |path: &str| path.starts_with(&own);
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

  let after = part_pieces(root, trim);
  let kept = total(&after) as i64 - total(check.before) as i64
    + reconfirmed_chars(check.before, &after) as i64
    + dropped_chars as i64;
  let stretch_chars = statement_chars(&render(&Listed {
    heading: String::new(),
    items: check.stretch.to_vec(),
  }));
  let shrunk = (kept as f64) < stretch_chars as f64 * SORT_FIDELITY_FLOOR;

  if shrunk {
    problems.push(format!(
      "the stretch's {stretch_chars} characters of statements come back as {kept}, which is a summary rather than a move"
    ));
  }

  let mut parts = vec![];

  if !writes.is_empty() {
    parts.push(write_problems_cause(&writes));
  }

  if !strays.is_empty() {
    parts.push(format!(
      "The run changed {} document(s) besides this person's pieces, such as {}.",
      strays.len(),
      strays[0]
    ));
  }

  if !unlined.is_empty() {
    parts.push(format!(
      "{} of the rows it wrote through the fragment tool did not match what it wrote on disk.",
      unlined.len()
    ));
  }

  if !unknown.is_empty() {
    parts.push(format!(
      "Its list of dropped items held {} line(s) that were not items of the stretch word for word.",
      unknown.len()
    ));
  }

  if shrunk {
    parts.push(format!(
      "The stretch's {stretch_chars} characters of statements came back as {kept}: the run summarised them instead of moving them."
    ));
  }

  (
    problems,
    format!(
      "What the trimming run wrote did not pass the checks on a walked stretch, so it was put back. {}",
      parts.join(" ")
    ),
  )
}

// One stretch, under the lock the caller holds: the run, the checks, and on a run that stands the stretch taken off
// the holding file, the room pieces it made put on the sorted list where the heading stood there, and what was
// written followed. Answers why the stretch did not stand, as a person reads it, or none when it did.
#[allow(clippy::too_many_lines)]
async fn walk_stretch<R: SortRoom>(
  room: &mut R,
  trim: &Trim,
  stretch: &[Item],
  left: usize,
) -> Result<Option<String>, Stopped> {
  let root = room.root().to_path_buf();
  let user_id = trim.heading.user_id;

  // The list a stretch before this one dropped is in the brain's history already.
  let _ = fs::remove_file(root.join(trim.dropped_path()));

  let before = part_pieces(&root, trim);
  let slugs_before: BTreeSet<String> =
    before.iter().map(|piece| piece_slug(&piece.path).to_owned()).collect();
  let backup = backup_documents(&root);
  let places = vec![trim.dir()];
  let mut named = BTreeSet::new();
  let known = room.known().await?;
  let names = room.names().await?;
  let title = match trim.chat_id {
    Some(chat_id) => room.title(chat_id).await?,
    None => None,
  };
  let display = room.display_name(user_id).await?;
  let stretch_text = render(&Listed {
    heading: String::new(),
    items: stretch.to_vec(),
  });
  let named_items = fragment_desk::meta::items_named(&stretch_text);
  let states = room.item_states(&named_items).await?;
  let prompt = trim_prompt(&TrimPrompt {
    prompts: room.prompts(),
    agent: room.label(),
    chat_id: trim.chat_id,
    chat_title: title.as_deref(),
    user_id,
    display_name: display.as_deref(),
    heading: trim.heading.name,
    piece_path: &trim.piece_path(),
    piece: &read_text(&root, &trim.piece_path()),
    holding_path: &trim.holding_path(),
    dropped_path: &trim.dropped_path(),
    chunks_path: &format!("{}/{CHUNKS_DIR}", trim.dir()),
    stretch: &stretch_text,
    item_count: stretch.len(),
    total_items: left,
    item_states: &item_states_text(&states, &named_items),
    forbidden_titles: &known.titles,
  })
  .await?;
  // The run reaches this person's rows of the heading's part, and writes nowhere else.
  let reach = vec![FragmentFilter {
    kind: Some("person-book".to_owned()),
    chat_id: Some(trim.chat_id),
    owners: Some(vec![user_id]),
    active: Some(true),
    ..FragmentFilter::default()
  }];
  let done = format!("people trim: {} in {} walked one stretch", trim.slug, trim.place());
  let ran = room
    .run_editor(
      trim.run_room,
      &prompt,
      SortDesk {
        room: trim.run_room,
        places: places.clone(),
        reach,
        before: &backup,
        named: &mut named,
        done: &done,
      },
    )
    .await;
  let claim = RunClaim {
    places,
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
      let (problems, cause) = stretch_problems(
        &root,
        trim,
        &StretchCheck {
          stretch,
          before: &before,
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
      "people trim: {} in {}: {}; put back",
      trim.slug,
      trim.place(),
      problems.iter().take(5).cloned().collect::<Vec<_>>().join("; ")
    );

    return Ok(Some(cause));
  }

  let touched = run_touched(&root, &backup, &claim);

  walked(&root, trim, stretch.len())?;

  // What a room heading on the sorted list held was sorted already, so the pieces it gave are too.
  if let Some(chat_id) = trim.chat_id {
    let dir = people_dir_of(chat_id);

    if sorted_slugs(&root, &dir).contains(&trim.slug) {
      let made: Vec<String> = part_pieces(&root, trim)
        .iter()
        .map(|piece| piece_slug(&piece.path).to_owned())
        .filter(|slug| !slugs_before.contains(slug))
        .collect();

      mark_sorted(&root, &dir, &made);
    }
  }

  // The dropped list is committed with the part's directory and announced to nobody: it is no document anything reads.
  let paths: Vec<String> = touched
    .changed
    .into_iter()
    .chain(touched.removed)
    .filter(|path| !path.contains(&format!("/{TRIMMING_DIR}/")))
    .collect();

  room
    .finish(
      trim.run_room,
      user_id,
      &paths,
      &format!(
        "Person {user_id}'s {} heading in {} gives one more stretch to the pieces of its matters",
        trim.heading.name,
        trim.place()
      ),
      &writes,
    )
    .await?;
  let _ = fs::remove_file(root.join(trim.dropped_path()));
  println!("people trim: {} in {} walked {} items", trim.slug, trim.place(), stretch.len());

  Ok(None)
}

// The walk ended: the holding file taken away, a heading left with no item taken away with its row, and a heading
// still past its ceiling counted as a failure of the text it holds.
async fn end_walk<R: SortRoom>(room: &mut R, trim: &Trim) -> Result<(), Stopped> {
  let root = room.root().to_path_buf();
  let piece = trim.piece_path();
  let mut writes = vec![];

  let _ = fs::remove_file(root.join(trim.holding_path()));
  let _ = fs::remove_file(root.join(trim.dropped_path()));
  let _ = fs::remove_dir(root.join(trim.dir()).join(TRIMMING_DIR));

  if listed(&read_text(&root, &piece)).items.is_empty() {
    let _ = fs::remove_file(root.join(&piece));
    writes.push(fragment_desk::meta::removal_write(&piece));
  }

  room
    .finish(
      trim.run_room,
      trim.heading.user_id,
      std::slice::from_ref(&piece),
      &format!(
        "Person {}'s {} heading in {} is trimmed",
        trim.heading.user_id,
        trim.heading.name,
        trim.place()
      ),
      &writes,
    )
    .await?;

  if over_ceiling(&root, &piece) {
    let key = piece_failure_key(&root, trim);

    note_failure(
      room,
      &key,
      Work::TrimHeading(trim.heading.name),
      trim.chat_id,
      trim.heading.user_id,
      &format!(
        "After the trim the heading still holds {} characters, past its {FIXED_CEILING_CHARS}-character ceiling: what was judged still open does not fit it.",
        js::units(js::trim(&read_text(&root, &piece)))
      ),
    )
    .await?;
  } else {
    println!("people trim: {} in {} is under its ceiling", trim.slug, trim.place());
  }

  Ok(())
}

// One fixed heading set aside where it is past its ceiling, and walked until its holding file is empty or the budget
// of runs is spent.
async fn trim_heading<R: SortRoom>(
  room: &mut R,
  trim: &Trim,
  budget: &mut usize,
) -> Result<Ended, Stopped> {
  let root = room.root().to_path_buf();

  if !root.join(trim.holding_path()).exists() {
    if room.count(&piece_failure_key(&root, trim)).await? >= SKIP_AFTER {
      println!(
        "people trim: {} in {} is left past its ceiling after two trims",
        trim.slug,
        trim.place()
      );
      return Ok(Ended::Done);
    }

    if room.resting().await? {
      return Ok(Ended::Stopped);
    }

    let Some(held) = room.lock().await? else { return Ok(Ended::Stopped) };
    let finished = match set_aside(&root, trim) {
      Ok(()) => {
        room
          .finish(
            trim.run_room,
            trim.heading.user_id,
            &[trim.piece_path()],
            &format!(
              "Person {}'s {} heading in {} is set aside to be trimmed",
              trim.heading.user_id,
              trim.heading.name,
              trim.place()
            ),
            &[],
          )
          .await
      }
      Err(why) => Err(why),
    };

    drop(held);
    finished?;
    println!("people trim: {} in {} is set aside", trim.slug, trim.place());
  }

  loop {
    let items = listed(&read_text(&root, &trim.holding_path())).items;

    if items.is_empty() {
      let Some(held) = next_run_lock(room, usize::MAX).await? else {
        return Ok(Ended::Stopped);
      };
      let ended = end_walk(room, trim).await;

      drop(held);
      ended?;

      return Ok(Ended::Done);
    }

    let stretch = &items[..stretch_len(&items, STRETCH_CHARS)];
    let key = stretch_failure_key(trim, stretch);

    if room.count(&key).await? >= SKIP_AFTER {
      let Some(held) = next_run_lock(room, usize::MAX).await? else {
        return Ok(Ended::Stopped);
      };
      let back = match put_back(&root, trim, stretch.len()) {
        Ok(()) => {
          room
            .finish(
              trim.run_room,
              trim.heading.user_id,
              &[trim.piece_path()],
              &format!(
                "Person {}'s {} heading in {} takes back a stretch its trim could not walk",
                trim.heading.user_id,
                trim.heading.name,
                trim.place()
              ),
              &[],
            )
            .await
        }
        Err(why) => Err(why),
      };

      drop(held);
      back?;
      println!("people trim: {} in {} keeps a stretch that failed twice", trim.slug, trim.place());
      continue;
    }

    let Some(held) = next_run_lock(room, *budget).await? else { return Ok(Ended::Stopped) };

    *budget -= 1;

    let walked = walk_stretch(room, trim, stretch, items.len()).await;

    drop(held);

    match walked? {
      None => room.clear(&key).await?,
      Some(cause) => {
        let skipped = note_failure(
          room,
          &key,
          Work::TrimStretch(trim.heading.name),
          trim.chat_id,
          trim.heading.user_id,
          &cause,
        )
        .await?;

        if !skipped {
          return Ok(Ended::Failed);
        }
      }
    }
  }
}

/// Every fixed heading of this person past its ceiling trimmed, one at a time and a heading already set aside first:
/// done with nothing left, stopped for a spent budget or a rest, failed for a run that did not stand, which stops this
/// person with the book as it was before that run. What a heading set aside holds is read by nothing until it is
/// walked, so a failure never goes on to set aside the next one.
pub async fn trim_person<R: SortRoom>(
  room: &mut R,
  user_id: i64,
  budget: &mut usize,
) -> Result<Ended, Stopped> {
  let root = room.root().to_path_buf();
  let unkept = room.unkept().clone();
  let mut trims = trims_of(&root, user_id, &unkept);

  trims.sort_by_key(|trim| !root.join(trim.holding_path()).exists());

  for trim in trims {
    match trim_heading(room, &trim, budget).await? {
      Ended::Done => {}
      ended => return Ok(ended),
    }
  }

  Ok(Ended::Done)
}
