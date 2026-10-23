//! A subject's sheet, rendered from its facts (docs/design.md, 「A person is a subject」), and the split of the
//! `sheet` rows a store still holds live into facts (docs/boundary-api.md, 「Splitting sheets」).

use bmo_brain_client::{Labels, MemoryMove, MemoryStatus, NewMemory};
use sqlx::PgConnection;

use super::brains::{brain_of, given};
use super::recall::day;
use super::versions::{MEMORY_COLUMNS, MemoryRow};
use super::{Fault, Held, Store, now};

/// The word no memory is written under: a person's sheet is a rendering of their facts.
pub const SHEET: &str = "sheet";

/// Who the split writes as.
pub const SPLITTER: &str = "migration";

/// The reason a split sheet carries.
pub const SPLIT_REASON: &str = "split into facts";

/// The type a writer names, held to the rule of a key and refused where it is `sheet`.
pub(crate) fn written_type(kind: &str) -> Held<()> {
  given("type", kind)?;
  if kind == SHEET {
    return Err(Fault::BadRequest(
      "type is sheet; a person's sheet is rendered from their facts, so write each fact as a fact."
        .to_owned(),
    ));
  }
  Ok(())
}

/// The facts a subject's sheet renders: its active facts about no room, and about the room named where one
/// is, by importance, highest first, then by the moment they hold from, then by id.
pub(crate) async fn sheet_rows(
  conn: &mut PgConnection,
  brain_id: i64,
  subject_id: i64,
  room: Option<i64>,
) -> Held<Vec<MemoryRow>> {
  Ok(
    sqlx::query_as(&format!(
      "SELECT {MEMORY_COLUMNS} FROM memories m WHERE brain_id = $1 AND status = 'active' AND type = 'fact' \
       AND id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = $2) \
       AND (NOT EXISTS (SELECT 1 FROM memory_subjects ms JOIN subjects s ON s.id = ms.subject_id \
             WHERE ms.memory_id = m.id AND s.type = 'room') \
         OR ($3::bigint IS NOT NULL AND id IN (SELECT memory_id FROM memory_subjects WHERE subject_id = $3))) \
       ORDER BY importance DESC, valid_from, id"
    ))
    .bind(brain_id)
    .bind(subject_id)
    .bind(room)
    .fetch_all(&mut *conn)
    .await?,
  )
}

/// One fact as a line of a sheet: its id, the day it holds from, and its words.
fn sheet_line(row: &MemoryRow) -> String {
  format!("- [#{} since {}] {}", row.id, day(row.valid_from), row.text.trim())
}

/// What one sheet came to, or would come to on a dry run: ids and counts alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetSplit {
  pub sheet_id: i64,
  pub brain_id: i64,
  pub subject_ids: Vec<i64>,
  pub lines: i64,
  pub facts: i64,
  pub headings: i64,
  pub empty: i64,
}

/// A line of a sheet as the split reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetLine {
  /// A heading, with its words.
  Heading(String),
  /// A line with no words.
  Empty,
  /// A fact.
  Fact(SplitFact),
}

/// One fact a sheet's line comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitFact {
  pub text: String,
  /// The moment it holds from, where the line's bracket names one.
  pub valid_from: Option<i64>,
  /// The moment it was last confirmed, where the line's bracket names one.
  pub confirmed: Option<i64>,
  /// The line's leading bracket as it stood, where it carried one.
  pub prefix: Option<String>,
}

/// A day, or a month as its first day, as epoch milliseconds at its start in UTC.
fn date_ms(text: &str) -> Option<i64> {
  let text = text.trim();
  let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
    .ok()
    .or_else(|| chrono::NaiveDate::parse_from_str(&format!("{text}-01"), "%Y-%m-%d").ok())?;
  Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
}

/// A date or a span of two (`<date>~<date>`): the moment it starts, and whether it is a span.
fn dated(text: &str) -> Option<(i64, bool)> {
  match text.split_once('~') {
    Some((start, end)) => {
      let from = date_ms(start)?;
      date_ms(end)?;
      Some((from, true))
    }
    None => date_ms(text).map(|from| (from, false)),
  }
}

/// The words after a bullet's marker, where the line is a bullet.
fn bullet(line: &str) -> Option<&str> {
  let line = line.trim_start();
  for marker in ['-', '*', '•'] {
    if let Some(rest) = line.strip_prefix(marker)
      && (rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with('['))
    {
      return Some(rest.trim_start());
    }
  }
  let digits = line.chars().take_while(char::is_ascii_digit).count();
  if digits > 0 {
    let rest = &line[digits..];
    if let Some(rest) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))
      && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
      return Some(rest.trim_start());
    }
  }
  None
}

/// A bullet's words with its leading bracket read: the bracket taken off where it is a note of dates, the
/// span it names kept ahead of the words.
fn noted(words: &str) -> SplitFact {
  let whole = SplitFact {
    text: words.trim().to_owned(),
    valid_from: None,
    confirmed: None,
    prefix: None,
  };
  let Some(inner_end) = words.strip_prefix('[').and_then(|rest| rest.find(']')) else {
    return whole;
  };
  let inner = &words[1..=inner_end];
  let after = words[inner_end + 2..].trim();
  let prefix = Some(words[..inner_end + 2].to_owned());
  let mut named: Option<(i64, bool, &str)> = None;
  let mut bare: Option<(i64, bool, &str)> = None;
  let mut confirmed = None;
  let mut is_note = false;
  for part in inner.split('·').map(str::trim) {
    let (word, rest) = part.split_once(char::is_whitespace).unwrap_or((part, ""));
    let rest = rest.trim();
    match word {
      "date" | "basis" => {
        is_note = true;
        if named.is_none() {
          named = dated(rest).map(|(from, span)| (from, span, rest));
        }
      }
      "confirmed" => {
        is_note = true;
        confirmed = confirmed.or_else(|| date_ms(rest));
      }
      _ => {
        if let Some((from, span)) = dated(part) {
          is_note = true;
          if bare.is_none() {
            bare = Some((from, span, part));
          }
        }
      }
    }
  }
  if !is_note {
    return SplitFact { prefix, ..whole };
  }
  let start = named.or(bare);
  let text = match start {
    Some((_, true, span)) if !after.is_empty() => format!("[{span}] {after}"),
    _ => after.to_owned(),
  };
  SplitFact {
    text,
    valid_from: start.map(|(from, _, _)| from),
    confirmed,
    prefix,
  }
}

/// One line of a sheet as the split reads it.
pub fn sheet_line_read(line: &str) -> SheetLine {
  let trimmed = line.trim();
  if trimmed.is_empty() {
    return SheetLine::Empty;
  }
  if let Some(heading) = trimmed.strip_prefix('#') {
    return SheetLine::Heading(heading.trim_start_matches('#').trim().to_owned());
  }
  let fact = match bullet(trimmed) {
    Some(words) => noted(words),
    None => SplitFact {
      text: trimmed.to_owned(),
      valid_from: None,
      confirmed: None,
      prefix: None,
    },
  };
  if fact.text.is_empty() { SheetLine::Empty } else { SheetLine::Fact(fact) }
}

impl Store {
  /// A subject's sheet rendered: the facts `sheet_rows` reads, one line each, joined by a newline, the same
  /// bytes for the same rows.
  pub async fn sheet(&self, brain_id: i64, subject_id: i64, room_id: Option<i64>) -> Held<String> {
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let standing: Option<String> =
      sqlx::query_scalar("SELECT type FROM subjects WHERE brain_id = $1 AND id = $2")
        .bind(brain_id)
        .bind(subject_id)
        .fetch_optional(&mut *conn)
        .await?;
    if standing.is_none() {
      return Err(Fault::NotFound(format!("Brain {brain_id} has no subject {subject_id}.")));
    }
    if let Some(room) = room_id {
      let kind: Option<String> =
        sqlx::query_scalar("SELECT type FROM subjects WHERE brain_id = $1 AND id = $2")
          .bind(brain_id)
          .bind(room)
          .fetch_optional(&mut *conn)
          .await?;
      match kind.as_deref() {
        None => return Err(Fault::NotFound(format!("Brain {brain_id} has no subject {room}."))),
        Some("room") => {}
        Some(other) => {
          return Err(Fault::BadRequest(format!("roomId names subject {room}, a {other}, not a room.")));
        }
      }
    }
    let rows = sheet_rows(&mut conn, brain_id, subject_id, room_id).await?;
    Ok(rows.iter().map(sheet_line).collect::<Vec<_>>().join("\n"))
  }

  /// Every live `sheet` row of every brain split into facts, each sheet with its facts in one transaction
  /// and then moved to `done`; on a dry run, what each would come to, with nothing written.
  pub async fn sheets_split(&self, dry_run: bool) -> Held<Vec<SheetSplit>> {
    let sheets: Vec<(i64, i64)> = sqlx::query_as(
      "SELECT brain_id, id FROM memories WHERE type = $1 AND status IN ('active', 'pending') ORDER BY brain_id, id",
    )
    .bind(SHEET)
    .fetch_all(&self.pool)
    .await?;
    let mut splits = Vec::new();
    for (brain_id, id) in sheets {
      let mut tx = self.begin().await?;
      let sheet = self.row_of(&mut tx, brain_id, id, true).await?;
      let status = sheet.status()?;
      if sheet.kind != SHEET || !status.is_live() {
        continue;
      }
      let subject_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT subject_id FROM memory_subjects WHERE memory_id = $1 ORDER BY subject_id",
      )
      .bind(id)
      .fetch_all(&mut *tx)
      .await?;
      let mut split = SheetSplit {
        sheet_id: id,
        brain_id,
        subject_ids: subject_ids.clone(),
        lines: 0,
        facts: 0,
        headings: 0,
        empty: 0,
      };
      let mut section: Option<String> = None;
      let mut facts: Vec<(SplitFact, Option<String>)> = Vec::new();
      for line in sheet.text.lines() {
        split.lines += 1;
        match sheet_line_read(line) {
          SheetLine::Empty => split.empty += 1,
          SheetLine::Heading(words) => {
            split.headings += 1;
            section = Some(words).filter(|words| !words.is_empty());
          }
          SheetLine::Fact(fact) => facts.push((fact, section.clone())),
        }
      }
      split.facts = facts.len() as i64;
      if !dry_run {
        let at = now();
        for (fact, section) in facts {
          let mut labels: Labels = sheet.labels.0.clone();
          labels.insert("splitFrom".to_owned(), id.to_string());
          if let Some(section) = section {
            labels.insert("section".to_owned(), section);
          }
          if let Some(prefix) = fact.prefix {
            labels.insert("prefix".to_owned(), prefix);
          }
          let written = self
            .add_in(
              &mut tx,
              brain_id,
              &NewMemory {
                key: None,
                kind: "fact".to_owned(),
                text: fact.text,
                layer_id: sheet.layer_id,
                position: None,
                subject_ids: subject_ids.clone(),
                subjects: vec![],
                importance: Some(sheet.importance.into()),
                valid_from: Some(fact.valid_from.unwrap_or(sheet.valid_from)),
                status: Some(status),
                actor_name: SPLITTER.to_owned(),
                labels,
              },
              at,
            )
            .await?;
          if let Some(confirmed) = fact.confirmed {
            sqlx::query("UPDATE memories SET last_confirmed_at = $2 WHERE id = $1")
              .bind(written.id)
              .bind(confirmed)
              .execute(&mut *tx)
              .await?;
          }
        }
        self
          .move_in(
            &mut tx,
            brain_id,
            id,
            &MemoryMove {
              status: MemoryStatus::Done,
              at: Some(at),
              actor_name: SPLITTER.to_owned(),
              reason: Some(SPLIT_REASON.to_owned()),
              text: None,
            },
          )
          .await?;
        self.commit(tx).await?;
      }
      splits.push(split);
    }
    Ok(splits)
  }
}
