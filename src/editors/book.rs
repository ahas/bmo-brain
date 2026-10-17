//! The person book's shapes as its editors read them (docs/people-book.md, 「Where it lives」 and 「Slugs」):
//! where each part's directories and files stand relative to an agent's folder, how a piece's slug names its
//! person and its stretch, how an older single document is walked in stretches and whether it is still being
//! cut, how much a text says, what a common document may not name, and a removed room's documents taken out of
//! every agent's folder.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use brain_support::js;
use regex::Regex;

use crate::Failure;
use crate::editors::commits::{commit_removal, remove};
use crate::editors::lock::{NoKeep, QUEUE_POLL, brain_lock_path, take_lock_first};
use crate::policy::BRAIN;
use crate::ports::agency::Agency;

/// The rooms' books and rulebooks, relative to an agent's folder.
pub const ROOMS_ROOT: &str = "rooms";
/// Every person's common book, one directory per user id, relative to an agent's folder.
pub const PEOPLE_ROOT: &str = "people";
/// The knowledge library, relative to an agent's folder.
pub const KNOWLEDGE_ROOT: &str = "knowledge";
/// Where the pieces of a book live under its directory, in both parts.
pub const CHUNKS_DIR: &str = "chunks";
/// A person's common sheet, in their common directory.
pub const COMMON_SHEET_NAME: &str = "sheet.md";

/// A room's documents under every agent's folder in the brain — the chat agent's being the brain's root — where
/// they stand.
pub async fn room_books(agency: &dyn Agency, brain: &Path, room: i64) -> Result<Vec<PathBuf>, Failure> {
  let mut folders = vec![String::new()];

  for registration in agency.registrations(None).await? {
    if let Some(folder) = registration.brain_folder {
      folders.push(folder.trim_end_matches('/').to_owned());
    }
  }

  folders.sort();
  folders.dedup();

  Ok(
    folders
      .iter()
      .map(|folder| brain.join(folder).join(ROOMS_ROOT).join(room.to_string()))
      .filter(|path| path.exists())
      .collect(),
  )
}

// How long a room's removal waits for the editor writing an agent's folder to let it go.
const ROOM_BOOKS_WAIT_SECONDS: u64 = 600;

/// A room's documents removed and the removal committed in the brain, under the editing lock of the agent's folder
/// they stand in, so no editor writes them again between the two (docs/people-book.md, 「One lock per agent's
/// folder」). `path` is one of what [`room_books`] answers.
pub async fn remove_room_books(data_root: &Path, brain: &Path, path: &Path) -> Result<(), String> {
  let folder = path
    .strip_prefix(brain)
    .map_err(|_| format!("{} is not in the brain", path.display()))?
    .to_path_buf();
  let agent_folder = folder
    .parent()
    .and_then(Path::parent)
    .map(|agent| agent.to_string_lossy().into_owned())
    .unwrap_or_default();
  let room = path.file_name().map(|room| room.to_string_lossy().into_owned()).unwrap_or_default();
  let lock = brain_lock_path(data_root, &agent_folder);
  let held = take_lock_first(&lock, ROOM_BOOKS_WAIT_SECONDS, "room removal", &NoKeep, QUEUE_POLL)
    .await
    .map_err(|why| why.to_string())?
    .ok_or_else(|| format!("the editing lock of the brain folder '{agent_folder}' was not had in time"))?;
  let brain = brain.to_path_buf();
  let path = path.to_path_buf();
  let removed = tokio::task::spawn_blocking(move || {
    remove(&path)
      .or_else(|why| if why.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(why) })
      .map_err(|why| why.to_string())?;
    commit_removal(
      &brain,
      &folder.to_string_lossy(),
      &format!("room {room}'s registration is removed: its documents go with it"),
    )
    .map(drop)
  })
  .await
  .map_err(|why| why.to_string())
  .and_then(|removed| removed);

  held.release();
  removed
}

/// How many characters of the book one run of the writer is handed: a batch whose people together hold more
/// than this is written by several runs, each with the whole batch and the documents of its own people.
pub const DOCUMENT_CEILING_CHARS: usize = 200_000;

/// How much of an older single document one cut run is handed: a run asked for a whole long document writes a
/// summary of it instead, so the document is walked a stretch at a time.
pub const SPAN_CHARS: usize = 8_000;

/// The fixed headings, common and room-only, each by its topic and the name the book writes. A writer is handed
/// each of these pieces of the people it writes whole, since every batch may touch them.
pub const FIXED_HEADINGS: [(&str, &str); 12] = [
  ("personality", "Personality"),
  ("values", "Values"),
  ("speech", "Speech"),
  ("habits", "Habits"),
  ("requests", "Working requests"),
  ("told", "Told to remember"),
  ("interests", "Interests"),
  ("dislikes", "Dislikes and cautions"),
  ("humor", "Humor"),
  ("address", "Address"),
  ("relations", "Relations"),
  ("ongoing", "Ongoing and promises"),
];

/// How long a fixed heading may grow: every batch carries it whole, so what stands past this is trimmed into the
/// pieces of its own matters.
pub const FIXED_CEILING_CHARS: usize = 6_000;

/// Where a fixed heading set aside to be trimmed waits, beside its part's pieces.
pub const TRIMMING_DIR: &str = "trimming";

/// One piece of a person's fixed headings, as its slug names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedHeading {
  /// Whether the piece stands in the common part rather than a room's.
  pub common: bool,
  pub user_id: i64,
  pub topic: &'static str,
  pub name: &'static str,
}

/// The fixed heading a piece's slug names, none where it names a topic of the writer's own.
pub fn fixed_heading(slug: &str) -> Option<FixedHeading> {
  let common = slug.starts_with('c');
  let rest = slug.strip_prefix('c').or_else(|| slug.strip_prefix('p'))?;
  let (id, topic) = rest.split_once('-')?;

  if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
    return None;
  }

  let (topic, name) = FIXED_HEADINGS.iter().find(|(fixed, _)| *fixed == topic)?;

  Some(FixedHeading {
    common,
    user_id: id.parse().ok()?,
    topic,
    name,
  })
}

/// A piece as a writer or the sheet judgement is handed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonPiece {
  pub path: String,
  pub text: String,
}

/// A room's person book.
pub fn people_dir_of(chat_id: i64) -> String {
  format!("{ROOMS_ROOT}/{chat_id}/people")
}

/// A room's rulebook.
pub fn room_rules_dir_of(chat_id: i64) -> String {
  format!("{ROOMS_ROOT}/{chat_id}/rules")
}

/// One person's common book.
pub fn common_dir_of(user_id: i64) -> String {
  format!("{PEOPLE_ROOT}/{user_id}")
}

/// One person's common sheet.
pub fn common_sheet_path_of(user_id: i64) -> String {
  format!("{}/{COMMON_SHEET_NAME}", common_dir_of(user_id))
}

/// The slug a piece's path ends on, without its directory and its `.md`.
pub fn piece_slug(path: &str) -> &str {
  let name = path.rsplit('/').next().unwrap_or(path);

  name.strip_suffix(".md").unwrap_or(name)
}

/// Whether a piece's slug is one of a person's fixed headings.
pub fn fixed_heading_slug(slug: &str) -> bool {
  fixed_heading(slug).is_some()
}

/// The slug every piece of one stretch begins with: the number keeps two stretches of the same person from
/// writing the same file, and tells a later run which stretches already stand.
pub fn span_prefix(user_id: i64, span: usize) -> String {
  format!("p{user_id}-{span:02}")
}

/// Whether a room piece's slug was written as part of a numbered stretch of this person.
pub fn numbered_piece_slug(slug: &str, user_id: i64) -> bool {
  let Some(rest) = slug.strip_prefix(&format!("p{user_id}-")) else { return false };
  let digits = rest.bytes().take_while(u8::is_ascii_digit).count();

  digits >= 2 && rest.as_bytes().get(digits) == Some(&b'-')
}

/// How much a text says, in characters of English: its length with every Hangul syllable counted twice, so a
/// cut of a Korean document into English pieces is measured fairly.
pub fn said_length(text: &str) -> usize {
  js::units(text) + text.chars().filter(|character| ('\u{AC00}'..='\u{D7A3}').contains(character)).count()
}

// Where a stretch may end: after a paragraph break, or after a sentence.
static PARAGRAPH_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t]*\n").expect("a pattern"));
static SENTENCE_END: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"[.!?…]["'”’」』]*(?:[ \t]+|\n)"#).expect("a pattern"));

// Every place a stretch may end, in UTF-16 units as the stretches are measured, with the text's end last.
fn cut_points(characters: &[char]) -> Vec<usize> {
  let text = js::text(characters);
  let length = js::units_of(characters);
  let mut points = BTreeSet::new();

  for pattern in [&*PARAGRAPH_BREAK, &*SENTENCE_END] {
    // Each search goes on from where the last match ended, as a global pattern's does.
    for found in pattern.find_iter(&text) {
      let end = js::units_before(&text, found.end());

      if end < length {
        points.insert(end);
      }
    }
  }

  points.into_iter().chain(std::iter::once(length)).collect()
}

// The text between two UTF-16 offsets.
fn units_slice(text: &str, from: usize, to: usize) -> String {
  let units: Vec<u16> = text.encode_utf16().collect();

  String::from_utf16_lossy(&units[from.min(units.len())..to.min(units.len())])
}

/// The stretches a document is cut in, in order: a stretch ends where a sentence or a paragraph does, and the
/// stretches put back together are the document exactly. A last stretch of a few lines rides with the one
/// before it.
pub fn person_spans(text: &str, target: usize) -> Vec<String> {
  if text.is_empty() {
    return vec![];
  }

  let characters = js::chars(text);
  let length = js::units_of(&characters);
  let mut spans = vec![];
  let mut start = 0;

  for end in cut_points(&characters) {
    if end - start >= target || end == length {
      spans.push(units_slice(text, start, end));
      start = end;
    }
  }

  if spans.len() > 1 && spans.last().is_some_and(|last| js::units(last) < target / 4) {
    let last = spans.pop().unwrap_or_default();

    if let Some(before) = spans.last_mut() {
      before.push_str(&last);
    }
  }

  spans
}

/// Whether this person's room book is still being cut: the file at their room sheet's place is over the room
/// ceiling, pieces of numbered stretches stand, and a stretch of that file has no piece yet. A person with
/// pieces and none numbered was cut in one pass.
pub fn book_being_cut(file_text: &str, piece_slugs: &[String], user_id: i64) -> bool {
  if js::units(file_text) <= BRAIN.sheets.room_ceiling {
    return false;
  }

  let own: Vec<&String> =
    piece_slugs.iter().filter(|slug| slug.starts_with(&format!("p{user_id}-"))).collect();

  if !own.is_empty() && !own.iter().any(|slug| numbered_piece_slug(slug, user_id)) {
    return false;
  }

  (1..=person_spans(file_text, SPAN_CHARS).len())
    .any(|span| !own.iter().any(|slug| slug.starts_with(&format!("{}-", span_prefix(user_id, span)))))
}

/// What the rooms know that a person's common book may not name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownIdentifiers {
  pub user_ids: Vec<i64>,
  pub chat_ids: Vec<i64>,
  pub titles: Vec<String>,
}

// The numbers standing on their own in a text: a run of digits, with the minus a group's chat id wears, and
// no digit on either side of it.
fn standalone_numbers(text: &str) -> Vec<String> {
  let characters: Vec<char> = text.chars().collect();
  let mut found = vec![];
  let mut at = 0;

  while at < characters.len() {
    let signed = characters[at] == '-' && characters.get(at + 1).is_some_and(char::is_ascii_digit);

    if !(characters[at].is_ascii_digit() || signed) || (at > 0 && characters[at - 1].is_ascii_digit()) {
      at += 1;
      continue;
    }

    let start = at;

    at += usize::from(signed);

    while characters.get(at).is_some_and(char::is_ascii_digit) {
      at += 1;
    }

    found.push(characters[start..at].iter().collect());
  }

  found
}

/// What a common document of this person names that it may not: any other user id seen speaking in a room, any
/// chat id, and any room title. The person's own id is the one number it may carry, and a title is matched
/// without regard to case.
pub fn common_boundary_violations(text: &str, own_id: i64, known: &KnownIdentifiers) -> Vec<String> {
  let users: HashSet<i64> = known.user_ids.iter().copied().collect();
  let chats: HashSet<i64> = known.chat_ids.iter().copied().collect();
  let mut found: Vec<String> = vec![];

  for token in standalone_numbers(text) {
    // A number too large to be read exactly names nothing the rooms hold.
    let Ok(value) = token.parse::<i64>() else { continue };

    if value.unsigned_abs() > (1_u64 << 53) - 1 {
      continue;
    }

    let bare = value.abs();
    let verdict = if bare == own_id {
      None
    } else if chats.contains(&value) && value < 0 {
      Some(format!("chat id {value}"))
    } else if users.contains(&bare) {
      Some(format!("user id {bare}"))
    } else if chats.contains(&bare) {
      Some(format!("chat id {bare}"))
    } else {
      None
    };

    if let Some(verdict) = verdict.filter(|verdict| !found.contains(verdict)) {
      found.push(verdict);
    }
  }

  let lower = text.to_lowercase();

  for title in &known.titles {
    let needle = js::trim(title).to_lowercase();
    let named = format!("room title {title}");

    if js::units(&needle) >= 2 && lower.contains(&needle) && !found.contains(&named) {
      found.push(named);
    }
  }

  found
}

/// The changed paths that lie under none of the directories a run was allowed to write.
pub fn outside_surface(paths: &[String], allowed: &[String]) -> Vec<String> {
  paths.iter().filter(|path| !super::desk::in_places(allowed, path)).cloned().collect()
}
