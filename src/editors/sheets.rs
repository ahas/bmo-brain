//! The person book's standing sheets (docs/people-book.md, 「The standing layer — standing sheets」): one
//! person's sheet of one part written by the gate's sheet judgement from that part's own pieces still standing,
//! and held at this end to the part's ceiling, its own sections and the days its pieces name. A person whose
//! pieces are more than one call can read is folded: a sheet per group, then one sheet out of those.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use brain_support::js;
use futures_util::{StreamExt, stream};
use regex::Regex;

use super::book::{KnownIdentifiers, PersonPiece, common_boundary_violations, common_sheet_path_of};
use super::documents::common_pieces;
use crate::policy::BRAIN;
use crate::ports::gate::{Gate, Piece, SheetAsk, SheetPart};

/// How much a single summarising call is given to read.
pub const INPUT_CHARS: usize = 30_000;

/// How many of one sheet's group calls are asked at once.
pub const CALLS_AT_ONCE: usize = 4;

/// The two parts of a person's book: the common part every room the person speaks in reads, and one room's
/// own part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
  Common,
  Room,
}

impl Layer {
  /// How long a sheet of this part may be, in characters.
  pub fn ceiling(self) -> usize {
    match self {
      Self::Common => BRAIN.sheets.common_ceiling,
      Self::Room => BRAIN.sheets.room_ceiling,
    }
  }

  /// The fixed sections this part's sheet is written under, in order.
  pub fn sections(self) -> &'static [&'static str] {
    match self {
      Self::Common => &[
        "Personality",
        "Values",
        "Speech",
        "Habits",
        "Working requests",
        "Dislikes and cautions",
        "Humor",
      ],
      Self::Room => &["Address", "Relations", "Ongoing and promises"],
    }
  }

  fn other(self) -> Self {
    match self {
      Self::Common => Self::Room,
      Self::Room => Self::Common,
    }
  }

  fn part(self) -> SheetPart {
    match self {
      Self::Common => SheetPart::Common,
      Self::Room => SheetPart::Room,
    }
  }

  fn word(self) -> &'static str {
    match self {
      Self::Common => "common",
      Self::Room => "room",
    }
  }
}

/// One fragment row as a sheet's pieces are told apart by: its path, and whether its matter is done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceRow {
  pub path: String,
  pub status: Option<String>,
}

/// The pieces a standing sheet is written from: every piece of that part but those whose matter is done.
pub fn standing_pieces(pieces: Vec<PersonPiece>, rows: &[PieceRow]) -> Vec<PersonPiece> {
  let settled: HashSet<&str> = rows
    .iter()
    .filter(|row| row.status.as_deref() == Some("done"))
    .map(|row| row.path.as_str())
    .collect();

  pieces.into_iter().filter(|piece| !settled.contains(piece.path.as_str())).collect()
}

static DAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}").expect("a pattern"));

/// Whether every day a sheet names is a day its pieces name: the sheet attaches on every turn, and a day it made
/// up would be read as a record.
pub fn sheet_dates_known(sheet: &str, pieces: &[PersonPiece]) -> bool {
  let given: HashSet<&str> = pieces
    .iter()
    .flat_map(|piece| DAY.find_iter(&piece.text).map(|day| day.as_str()))
    .collect();

  DAY.find_iter(sheet).all(|day| given.contains(day.as_str()))
}

/// Why a sheet the summariser wrote for one part cannot stand, or none when it can: nothing written, over the
/// part's ceiling, a section of the other part, or a day none of its pieces names.
pub fn sheet_problem(layer: Layer, sheet: &str, pieces: &[PersonPiece]) -> Option<String> {
  let ceiling = layer.ceiling();

  if sheet.is_empty() {
    return Some("nothing".to_owned());
  }

  if js::units(sheet) > ceiling {
    return Some(format!("{} characters, over the {ceiling} ceiling", js::units(sheet)));
  }

  let foreign: Vec<&str> = layer
    .other()
    .sections()
    .iter()
    .filter(|section| sheet.split('\n').any(|line| js::trim(line) == format!("## {section}")))
    .copied()
    .collect();

  if !foreign.is_empty() {
    return Some(format!("a section of the other part ({})", foreign.join(", ")));
  }

  if !sheet_dates_known(sheet, pieces) {
    return Some("a day none of its pieces names".to_owned());
  }

  None
}

/// The pieces in the groups one call each takes on, in order: a piece never sits in two groups, and one larger
/// than the budget alone is a group of its own.
pub fn sheet_groups(pieces: &[PersonPiece], budget: usize) -> Vec<Vec<PersonPiece>> {
  let mut groups = vec![];
  let mut group: Vec<PersonPiece> = vec![];
  let mut held = 0;

  for piece in pieces {
    let size = js::units(&piece.text);

    if !group.is_empty() && held + size > budget {
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

/// Whose sheet of which part is asked for: the agent whose book it is, the room it is booked against, and the
/// registration its recollectors go out on where they name one.
pub struct SheetFor<'a> {
  pub gate: &'a dyn Gate,
  pub layer: Layer,
  pub chat_id: i64,
  pub user_id: i64,
  pub agent: &'a str,
  pub registration: Option<&'a str>,
}

// One summarising call, the part's terms held at this end: a sheet that cannot stand is no answer.
async fn ask_for_sheet(ask: &SheetFor<'_>, pieces: &[PersonPiece]) -> Option<String> {
  let judged = SheetAsk {
    agent: ask.agent.to_owned(),
    part: ask.layer.part(),
    chat_id: match ask.layer {
      Layer::Common => None,
      Layer::Room => Some(ask.chat_id),
    },
    person: ask.user_id,
    pieces: pieces
      .iter()
      .map(|piece| Piece {
        path: piece.path.clone(),
        text: piece.text.clone(),
      })
      .collect(),
    ceiling: i64::try_from(ask.layer.ceiling()).unwrap_or(i64::MAX),
    registration: ask.registration.map(str::to_owned),
  };
  let answer = match ask.gate.judge_sheet(&judged).await {
    Ok(answer) => answer,
    Err(why) => {
      eprintln!("people standing: the summariser of {} did not answer: {why}", ask.user_id);
      None
    }
  };
  let sheet = answer.map(|answer| js::trim(&answer).to_owned()).unwrap_or_default();

  if let Some(problem) = sheet_problem(ask.layer, &sheet, pieces) {
    eprintln!(
      "people standing: {} sheet of {} (chat {}) got {problem} from one call",
      ask.layer.word(),
      ask.user_id,
      ask.chat_id
    );

    return None;
  }

  Some(sheet)
}

/// One person's standing sheet of one part, written from that part's own pieces, or none where the summariser
/// gave nothing usable; the caller writes the file. The groups are asked side by side, each asked again once,
/// and a group that still gives nothing leaves the sheet that is there.
pub async fn write_standing_sheet(ask: &SheetFor<'_>, pieces: &[PersonPiece]) -> Option<String> {
  fold_sheet(ask.layer, ask.user_id, pieces, async |group: &[PersonPiece]| {
    ask_for_sheet(ask, group).await
  })
  .await
}

/// A sheet folded out of its pieces with the summariser's one call handed in: a call per group, side by side and
/// each asked again once, then — where there were several — one call over the groups' sheets. A group that still
/// gives nothing leaves the sheet that is there, since a sheet written from the groups that answered would drop the
/// rest without a word.
pub async fn fold_sheet<A>(layer: Layer, user_id: i64, pieces: &[PersonPiece], call: A) -> Option<String>
where
  A: AsyncFn(&[PersonPiece]) -> Option<String>,
{
  if pieces.is_empty() {
    return None;
  }

  let groups = sheet_groups(pieces, INPUT_CHARS);
  let part = layer.word();
  let call = &call;
  let answers: Vec<Option<String>> = stream::iter(groups.iter())
    .map(|group| async move {
      match call(group).await {
        Some(sheet) => Some(sheet),
        None => call(group).await,
      }
    })
    .buffered(CALLS_AT_ONCE)
    .collect()
    .await;

  if let Some(missing) = answers.iter().position(Option::is_none) {
    eprintln!(
      "people standing: {part} sheet of {user_id} got nothing usable for part {} of {}, keeping the sheet that is there",
      missing + 1,
      groups.len()
    );

    return None;
  }

  let parts: Vec<PersonPiece> = answers
    .into_iter()
    .enumerate()
    .map(|(at, text)| PersonPiece {
      path: format!("part {} of {}", at + 1, groups.len()),
      text: text.unwrap_or_default(),
    })
    .collect();
  let sheet = if parts.len() == 1 {
    parts.into_iter().next().map(|one| one.text)
  } else {
    call(&parts).await
  };

  match sheet {
    Some(sheet) => Some(format!("{sheet}\n")),
    None => {
      eprintln!(
        "people standing: {part} sheet of {user_id} could not be folded into one sheet, keeping the sheet that is there"
      );
      None
    }
  }
}

/// One person's common sheet written again from their common pieces standing now. A sheet that names anybody
/// but its person, or any room, is not written, on the same terms as a common piece; nor is one the summariser
/// could not produce. `root` is the folder of the agent whose book it is, and the rows are named as the paths
/// under it are. Answers whether a sheet was written.
pub async fn write_common_sheet(
  ask: &SheetFor<'_>,
  root: &Path,
  known: &KnownIdentifiers,
  rows: &[PieceRow],
) -> bool {
  let pieces = standing_pieces(common_pieces(root, ask.user_id), rows);

  if pieces.is_empty() {
    return false;
  }

  let Some(sheet) = write_standing_sheet(ask, &pieces).await else { return false };
  let named = common_boundary_violations(&sheet, ask.user_id, known);

  if !named.is_empty() {
    eprintln!(
      "people standing: the common sheet of {} named {}; keeping the sheet that is there",
      ask.user_id,
      named.join(", ")
    );

    return false;
  }

  if let Err(why) = fs::write(root.join(common_sheet_path_of(ask.user_id)), &sheet) {
    eprintln!("people standing: the common sheet of {} could not be written: {why}", ask.user_id);
    return false;
  }

  println!(
    "people standing: wrote the common sheet of {} ({} characters)",
    ask.user_id,
    js::units(js::trim(&sheet))
  );

  true
}
