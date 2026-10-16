//! The person book and the libraries beside it as files (docs/people-book.md, 「Code decides where the writer
//! may write」): what a run that writes them reads off the disk before and after, how its own writing is told
//! from another writer's beside it, how what it wrote is taken back when the run does not stand, and the
//! documents a batch's writer is handed.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use brain_support::js;

use super::DocumentBackup;
use super::book::{
  CHUNKS_DIR, KNOWLEDGE_ROOT, KnownIdentifiers, PEOPLE_ROOT, PersonPiece, ROOMS_ROOT, book_being_cut,
  common_boundary_violations, common_dir_of, outside_surface, people_dir_of,
};
use crate::Failure;
use crate::ports::agency::Agency;
use crate::ports::receiver::Receiver;

/// The libraries an editor run writes, each relative to the folder it stands in: only their documents are
/// compared before and after a run and put back when it does not stand.
pub const LIBRARY_DIRS: [&str; 3] = [ROOMS_ROOT, PEOPLE_ROOT, KNOWLEDGE_ROOT];

// Which room pieces have been sorted into the two parts, one slug a line, kept by code alone.
const SORTED_FILE: &str = "sorted.txt";

const OUTSIDE_PLACE: &str = "wrote outside its place: ";
const OTHERS_PIECE: &str = "wrote the room piece of somebody this run does not write for: ";

fn markdown_files(root: &Path, dir: &Path, into: &mut Vec<String>) {
  let Ok(entries) = fs::read_dir(dir) else { return };

  for entry in entries.flatten() {
    let path = entry.path();
    let Ok(kind) = entry.file_type() else { continue };

    if kind.is_dir() {
      markdown_files(root, &path, into);
    } else if kind.is_file()
      && path.extension().is_some_and(|extension| extension == "md")
      && let Ok(relative) = path.strip_prefix(root)
    {
      into.push(relative.to_string_lossy().into_owned());
    }
  }
}

/// Every document file of the libraries, relative to the root, in order.
pub fn library_documents(root: &Path) -> Vec<String> {
  let mut files = vec![];

  for dir in LIBRARY_DIRS {
    markdown_files(root, &root.join(dir), &mut files);
  }

  files.sort();
  files
}

/// Every library document's bytes as they stand now, by path relative to the root.
pub fn backup_documents(root: &Path) -> DocumentBackup {
  library_documents(root)
    .into_iter()
    .filter_map(|path| Some((path.clone(), fs::read(root.join(&path)).ok()?)))
    .collect()
}

/// The library documents added or changed since the backup, and the ones removed, whoever did it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touched {
  pub changed: Vec<String>,
  pub removed: Vec<String>,
}

pub fn documents_touched(root: &Path, backup: &DocumentBackup) -> Touched {
  let now = library_documents(root);
  let present: HashSet<&String> = now.iter().collect();
  let changed = now
    .iter()
    .filter(|path| {
      backup
        .get(*path)
        .is_none_or(|before| fs::read(root.join(path)).ok().as_ref() != Some(before))
    })
    .cloned()
    .collect();
  let removed = backup.keys().filter(|path| !present.contains(path)).cloned().collect();

  Touched { changed, removed }
}

/// What of the libraries one run answers for: the directories it may write are its own while it runs, so
/// whatever changed there is the run's; outside them a document is the run's only where its own file tools
/// named it, by absolute path.
#[derive(Clone, Debug, Default)]
pub struct RunClaim {
  pub places: Vec<String>,
  pub named: BTreeSet<PathBuf>,
}

/// The library documents changed since the backup, parted into the run's own and the ones another writer
/// changed beside it, which the run neither answers for nor puts back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunTouched {
  pub changed: Vec<String>,
  pub removed: Vec<String>,
  pub others: Vec<String>,
}

pub fn run_touched(root: &Path, backup: &DocumentBackup, claim: &RunClaim) -> RunTouched {
  let Touched { changed, removed } = documents_touched(root, backup);
  let every: Vec<String> = changed.iter().chain(&removed).cloned().collect();
  let outside: HashSet<String> = outside_surface(&every, &claim.places).into_iter().collect();
  let own = |path: &String| !outside.contains(path) || claim.named.contains(&root.join(path));

  RunTouched {
    changed: changed.iter().filter(|path| own(path)).cloned().collect(),
    removed: removed.iter().filter(|path| own(path)).cloned().collect(),
    others: every.iter().filter(|path| !own(path)).cloned().collect(),
  }
}

/// The run's own documents put back as the backup holds them: what it changed or removed is written again,
/// and what it added is deleted. Another writer's change beside it is left as it stands. Answers the paths
/// put back.
pub fn restore_documents(root: &Path, backup: &DocumentBackup, claim: &RunClaim) -> Vec<String> {
  let RunTouched {
    changed, removed, ..
  } = run_touched(root, backup, claim);
  let paths: Vec<String> = changed.into_iter().chain(removed).collect();

  for path in &paths {
    let at = root.join(path);

    match backup.get(path) {
      None => {
        let _ = fs::remove_file(&at);
      }
      Some(before) => {
        if let Some(dir) = at.parent() {
          let _ = fs::create_dir_all(dir);
        }

        if let Err(why) = fs::write(&at, before) {
          eprintln!("brain run: {path} could not be put back: {why}");
        }
      }
    }
  }

  paths
}

/// What the rooms know that a person's common book may not name: every person that stood as a sender in a room,
/// every room, and every room title. A room title that is an awake agent's own label is that agent's name rather
/// than a room's, so it is left out.
pub async fn known_identifiers(
  agency: &dyn Agency,
  receiver: &dyn Receiver,
) -> Result<KnownIdentifiers, Failure> {
  let names: HashSet<String> = agency
    .registrations(None)
    .await?
    .into_iter()
    .filter(|registration| registration.awake)
    .map(|registration| js::trim(&registration.label.unwrap_or(registration.name)).to_lowercase())
    .collect();
  let rooms = receiver.room_titles().await?;
  let titles = rooms
    .iter()
    .filter_map(|room| room.title.clone())
    .filter(|title| !names.contains(&js::trim(title).to_lowercase()))
    .collect();
  let user_ids: BTreeSet<i64> = rooms.iter().flat_map(|room| room.senders.iter().copied()).collect();
  let chat_ids: BTreeSet<i64> = rooms.iter().map(|room| room.chat_id).collect();

  Ok(KnownIdentifiers {
    user_ids: user_ids.into_iter().collect(),
    chat_ids: chat_ids.into_iter().collect(),
    titles,
  })
}

// The user id whose common book a path lies in.
fn common_owner_of(path: &str) -> Option<i64> {
  let rest = path.strip_prefix(&format!("{PEOPLE_ROOT}/"))?;
  let (id, _) = rest.split_once('/')?;

  id.bytes().all(|byte| byte.is_ascii_digit()).then(|| id.parse().ok())?
}

// The person a room piece belongs to, read off its file name.
fn room_piece_owner_of(path: &str) -> Option<i64> {
  let at = path.rfind(&format!("/{CHUNKS_DIR}/p"))?;
  let name = &path[at + CHUNKS_DIR.len() + 3..];

  if name.contains('/') || !name.ends_with(".md") {
    return None;
  }

  let (id, _) = name.split_once('-')?;

  (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then(|| id.parse().ok())?
}

/// What the problems `write_problems` found mean, as a cause a person reads: which kind stopped the run, how
/// many documents, and one of them as an example.
pub fn write_problems_cause(problems: &[String]) -> String {
  let outside: Vec<&str> = problems.iter().filter_map(|one| one.strip_prefix(OUTSIDE_PLACE)).collect();
  let others: Vec<&str> = problems.iter().filter_map(|one| one.strip_prefix(OTHERS_PIECE)).collect();
  let naming = problems.len() - outside.len() - others.len();
  let mut parts = vec![];

  if let Some(first) = outside.first() {
    parts.push(format!(
      "the run's own file tools wrote {} document(s) outside the part of the brain it may write ({first}{})",
      outside.len(),
      if outside.len() > 1 { " and others" } else { "" }
    ));
  }

  if let Some(first) = others.first() {
    parts.push(format!(
      "the run changed {} room piece(s) of people it was not writing for ({first})",
      others.len()
    ));
  }

  if naming > 0 {
    parts.push(format!(
      "{naming} common document(s) it wrote named another person's id or a room, which a common document may not"
    ));
  }

  format!(
    "The check on what the run wrote stopped it: {}. So everything the run wrote was put back.",
    parts.join("; ")
  )
}

/// Why what a run wrote cannot stand, or nothing when it can: a document its own file tools wrote outside the
/// directories it was allowed, a room piece of somebody the run does not write for where the caller names who
/// those are, or a common document it left naming an id or a room title that is not its own person's.
pub fn write_problems(
  root: &Path,
  backup: &DocumentBackup,
  claim: &RunClaim,
  known: &KnownIdentifiers,
  writers: Option<&[i64]>,
) -> Vec<String> {
  let RunTouched {
    changed,
    removed,
    others,
  } = run_touched(root, backup, claim);
  let touched: Vec<String> = changed.iter().chain(&removed).cloned().collect();
  let mut problems: Vec<String> = outside_surface(&touched, &claim.places)
    .into_iter()
    .map(|path| format!("{OUTSIDE_PLACE}{path}"))
    .collect();

  if !others.is_empty() {
    println!(
      "brain run: another writer changed {} while the run worked; left as they stand",
      others.join(", ")
    );
  }

  if let Some(writers) = writers {
    for path in &touched {
      if room_piece_owner_of(path).is_some_and(|owner| !writers.contains(&owner)) {
        problems.push(format!("{OTHERS_PIECE}{path}"));
      }
    }
  }

  for path in &changed {
    let Some(owner) = common_owner_of(path) else { continue };
    let text = fs::read_to_string(root.join(path)).unwrap_or_default();
    let named = common_boundary_violations(&text, owner, known);

    if !named.is_empty() {
      problems.push(format!("{path} names {}", named.join(", ")));
    }
  }

  problems
}

/// The slugs of one person's room pieces in a room part's directory, in order.
pub fn room_piece_slugs(root: &Path, people_dir: &str, user_id: i64) -> Vec<String> {
  let prefix = format!("p{user_id}-");
  let mut slugs: Vec<String> = file_names(&root.join(people_dir).join(CHUNKS_DIR))
    .into_iter()
    .filter(|name| name.starts_with(&prefix) && name.ends_with(".md"))
    .map(|name| name.trim_end_matches(".md").to_owned())
    .collect();

  slugs.sort();
  slugs
}

// The names in a directory, none where it cannot be read.
fn file_names(dir: &Path) -> Vec<String> {
  fs::read_dir(dir)
    .map(|entries| {
      entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
    })
    .unwrap_or_default()
}

/// Whether this person's room book in this room part's directory is still being cut.
pub fn room_book_being_cut(root: &Path, people_dir: &str, user_id: i64) -> bool {
  let Ok(text) = fs::read_to_string(root.join(people_dir).join(format!("{user_id}.md"))) else {
    return false;
  };

  book_being_cut(&text, &room_piece_slugs(root, people_dir, user_id), user_id)
}

// Whether a name is a person's sheet: digits and `.md`.
fn sheet_person(name: &str) -> Option<i64> {
  let id = name.strip_suffix(".md")?;

  (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then(|| id.parse().ok())?
}

/// Everyone a room part's book has a sheet or a room piece for, in id order, or none where the room has no
/// book at all.
pub fn room_book_people(root: &Path, people_dir: &str) -> Option<Vec<i64>> {
  let dir = root.join(people_dir);
  let names = fs::read_dir(&dir).ok()?;
  let mut ids = BTreeSet::new();

  for entry in names.flatten() {
    if let Some(id) = sheet_person(&entry.file_name().to_string_lossy()) {
      ids.insert(id);
    }
  }

  for name in file_names(&dir.join(CHUNKS_DIR)) {
    let Some(rest) = name.strip_prefix('p') else { continue };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();

    if !digits.is_empty()
      && rest[digits.len()..].starts_with('-')
      && let Ok(id) = digits.parse()
    {
      ids.insert(id);
    }
  }

  Some(ids.into_iter().collect())
}

/// A person-book run over a whole room, asked for by hand: a cut of the room's documents into pieces, the
/// standing sheets written again, or the book written again from the messages already folded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeRoomRun {
  Cut { person: Option<i64> },
  Sheets,
  Rewrite { folded: i64 },
}

/// What a whole-room run commits into the brain when it ends, and with what message: the room's book always,
/// and with the sheets' rewrite each of the room's people's common book too.
pub fn whole_room_commit(root: &Path, chat_id: i64, run: WholeRoomRun) -> (Vec<String>, String) {
  let people_dir = people_dir_of(chat_id);

  match run {
    WholeRoomRun::Cut { person: None } => {
      (vec![people_dir], format!("Room {chat_id}'s person book is cut into pieces"))
    }
    WholeRoomRun::Cut {
      person: Some(person),
    } => (vec![people_dir], format!("Room {chat_id}'s person book of {person} is cut into pieces")),
    WholeRoomRun::Sheets => {
      let people = room_book_people(root, &people_dir).unwrap_or_default();
      let paths = std::iter::once(people_dir).chain(people.into_iter().map(common_dir_of)).collect();

      (paths, format!("Room {chat_id}'s standing sheets are written again from the pieces"))
    }
    WholeRoomRun::Rewrite { folded } => (
      vec![people_dir],
      format!("Room {chat_id}'s person book is written again from {folded} folded messages"),
    ),
  }
}

/// One person's common pieces, in slug order.
pub fn common_pieces(root: &Path, user_id: i64) -> Vec<PersonPiece> {
  let dir = format!("{}/{CHUNKS_DIR}", common_dir_of(user_id));
  let prefix = format!("c{user_id}-");
  let mut names: Vec<String> = file_names(&root.join(&dir))
    .into_iter()
    .filter(|name| name.starts_with(&prefix) && name.ends_with(".md"))
    .collect();

  names.sort();
  names
    .into_iter()
    .filter_map(|name| {
      let path = format!("{dir}/{name}");

      Some(PersonPiece {
        text: fs::read_to_string(root.join(&path)).ok()?,
        path,
      })
    })
    .collect()
}

/// What one run of the writer is handed: the documents every run reads, and each person's own, in the order
/// asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchDocuments {
  pub shared: Vec<PersonPiece>,
  pub by_sender: Vec<(i64, Vec<PersonPiece>)>,
}

fn document_at(root: &Path, path: String) -> Option<PersonPiece> {
  Some(PersonPiece {
    text: fs::read_to_string(root.join(&path)).ok()?,
    path,
  })
}

/// Everything the writer of one batch works on, read off the disk: the room part's index, and for each person
/// their room pieces, their common index and their common pieces. A document that does not exist yet is left
/// out.
pub fn batch_documents(root: &Path, people_dir: &str, senders: &[i64]) -> BatchDocuments {
  let index = document_at(root, format!("{people_dir}/index.md"));
  let by_sender = senders
    .iter()
    .map(|&user_id| {
      let mut pieces: Vec<PersonPiece> = room_piece_slugs(root, people_dir, user_id)
        .into_iter()
        .filter_map(|slug| document_at(root, format!("{people_dir}/{CHUNKS_DIR}/{slug}.md")))
        .collect();

      pieces.extend(document_at(root, format!("{}/index.md", common_dir_of(user_id))));
      pieces.extend(common_pieces(root, user_id));

      (user_id, pieces)
    })
    .collect();

  BatchDocuments {
    shared: index.into_iter().collect(),
    by_sender,
  }
}

/// The documents of one group of people, in the order the run is given them.
pub fn document_list(documents: &BatchDocuments, senders: &[i64]) -> Vec<PersonPiece> {
  let mut list = documents.shared.clone();

  for user_id in senders {
    if let Some((_, pieces)) = documents.by_sender.iter().find(|(id, _)| id == user_id) {
      list.extend(pieces.iter().cloned());
    }
  }

  list
}

fn document_chars(pieces: &[PersonPiece]) -> usize {
  pieces.iter().map(|piece| js::units(&piece.text)).sum()
}

/// One person's pieces as far as a run may carry them, in the order they came, and the ones left out: a piece
/// that does not fit what the ceiling has left is not handed whole, and those after it still may be.
pub fn within_ceiling(
  pieces: Vec<PersonPiece>,
  ceiling_chars: usize,
) -> (Vec<PersonPiece>, Vec<PersonPiece>) {
  let mut left = ceiling_chars;
  let mut handed = vec![];
  let mut held = vec![];

  for piece in pieces {
    let chars = js::units(&piece.text);

    if chars <= left {
      left -= chars;
      handed.push(piece);
    } else {
      held.push(piece);
    }
  }

  (handed, held)
}

/// How a batch's people are shared out between runs, so that no run carries more of the book than the ceiling
/// allows. The room index goes with every group; somebody whose own documents pass the ceiling alone stands in
/// a group of their own, since a person is never split across runs.
pub fn sender_groups(documents: &BatchDocuments, ceiling_chars: usize) -> Vec<Vec<i64>> {
  let shared = document_chars(&documents.shared);
  let mut groups = vec![];
  let mut group = vec![];
  let mut chars = shared;

  for (user_id, pieces) in &documents.by_sender {
    let own = document_chars(pieces);

    if !group.is_empty() && chars + own > ceiling_chars {
      groups.push(std::mem::take(&mut group));
      chars = shared;
    }

    group.push(*user_id);
    chars += own;
  }

  if !group.is_empty() {
    groups.push(group);
  }

  groups
}

/// The directories a person's common book is written into, made ahead of a run whose writer has file tools and
/// no shell. What a run leaves empty is taken away again by `prune_common_dirs`.
pub fn make_common_dirs(root: &Path, user_ids: &[i64]) {
  for user_id in user_ids {
    let _ = fs::create_dir_all(root.join(common_dir_of(*user_id)).join(CHUNKS_DIR));
  }
}

pub fn prune_common_dirs(root: &Path, user_ids: &[i64]) {
  for user_id in user_ids {
    let dir = root.join(common_dir_of(*user_id));

    // A directory not empty, or not there, stands as it is.
    let _ = fs::remove_dir(dir.join(CHUNKS_DIR));
    let _ = fs::remove_dir(dir);
  }
}

/// The room pieces sorted into the two parts.
pub fn sorted_slugs(root: &Path, people_dir: &str) -> BTreeSet<String> {
  fs::read_to_string(root.join(people_dir).join(SORTED_FILE))
    .map(|text| {
      text
        .split('\n')
        .map(js::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
    })
    .unwrap_or_default()
}

/// These room pieces added to the sorted list, a slug whose piece is gone dropped from it at the same time.
pub fn mark_sorted(root: &Path, people_dir: &str, slugs: &[String]) {
  let standing: HashSet<String> = file_names(&root.join(people_dir).join(CHUNKS_DIR))
    .into_iter()
    .filter_map(|name| name.strip_suffix(".md").map(str::to_owned))
    .collect();
  let mut kept = sorted_slugs(root, people_dir);

  kept.extend(slugs.iter().cloned());
  kept.retain(|slug| standing.contains(slug));

  let file = root.join(people_dir).join(SORTED_FILE);

  if kept.is_empty() {
    let _ = fs::remove_file(file);
    return;
  }

  let text = kept.into_iter().collect::<Vec<_>>().join("\n");

  if let Err(why) = fs::write(&file, format!("{text}\n")) {
    eprintln!("brain run: {} could not be written: {why}", file.display());
  }
}

pub fn clear_sorted(root: &Path, people_dir: &str) {
  let _ = fs::remove_file(root.join(people_dir).join(SORTED_FILE));
}
