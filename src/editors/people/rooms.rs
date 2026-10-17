//! The runs over a whole room asked for by hand (docs/people-book.md, 「Cutting a book into pieces」, 「Reforming a
//! book」 and the sheets' rewrite): a room's older single documents cut into pieces a stretch at a time, its standing
//! sheets written again from the pieces standing, and its book written again beside the old one from the messages
//! already folded, taking the old book's place only once it stands whole. None of them reads an unread message or
//! marks one, so none is held by the digest pause; each ends by committing what stands.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use brain_support::ceiling::count_text;
use brain_support::js;
use fragment_desk::desk::{Beside, FragmentWrite};

use super::super::archive::{chat_title, read_count, read_messages};
use super::super::book::{
  CHUNKS_DIR, PersonPiece, ROOMS_ROOT, SPAN_CHARS, numbered_piece_slug, people_dir_of, person_spans,
  piece_slug, said_length, span_prefix,
};
use super::super::commits::git_once;
use super::super::documents::{
  RunClaim, WholeRoomRun, backup_documents, clear_sorted, restore_documents, room_book_people, run_touched,
  whole_room_commit,
};
use super::super::settle::{clean_documents, commit_paths, reload_written};
use super::batches::{Book, Writing, beside_path, senders_of};
use super::prompts::{Stretch, split_prompt};
use super::{BATCH_SIZE, PeopleDigest, Stopped, WriterDesk, Wrote};
use crate::Failure;

/// How much of a document its pieces must still add up to: below this the run summarised rather than cut.
pub const SPLIT_FIDELITY_FLOOR: f64 = 0.85;

// Where a room's book is written anew while the book it replaces is still the one its turns read.
const REFORM_DIR: &str = "people-reform";

/// The names in a directory that are a person's sheet, `<id>.md`, in order; none where the directory cannot be
/// read.
pub fn sheet_names(dir: &Path) -> Option<Vec<String>> {
  let mut names: Vec<String> = fs::read_dir(dir)
    .ok()?
    .flatten()
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| {
      name
        .strip_suffix(".md")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
    })
    .collect();

  names.sort();
  Some(names)
}

/// The day a document was last committed, read off git's answer: a day in `YYYY-MM-DD` shape, `unknown` for
/// anything else.
pub fn recorded_day(said: Option<&str>) -> String {
  let day = said.map(str::trim).unwrap_or_default();
  let shaped = day.len() == 10
    && day
      .bytes()
      .enumerate()
      .all(|(at, byte)| if at == 4 || at == 7 { byte == b'-' } else { byte.is_ascii_digit() });

  if shaped { day.to_owned() } else { "unknown".to_owned() }
}

impl PeopleDigest<'_> {
  /// The day a document of the brain was last committed, which is the day it is known to have held what it says;
  /// `unknown` for a document git has no commit for.
  pub fn last_recorded(&self, path: &str) -> String {
    recorded_day(git_once(self.brain(), &["log", "-1", "--format=%cs", "--", path]).ok().as_deref())
  }

  fn pieces_of_span(&self, chat_id: i64, prefix: &str) -> Vec<PersonPiece> {
    self.pieces_named(&people_dir_of(chat_id), &format!("{prefix}-"))
  }

  fn drop_pieces(&self, pieces: &[PersonPiece]) {
    for piece in pieces {
      let _ = fs::remove_file(self.brain().join(&piece.path));
    }
  }

  fn piece_sizes(&self, chat_id: i64, user_id: i64) -> Vec<(String, usize)> {
    self
      .chunks_of(&people_dir_of(chat_id), user_id)
      .into_iter()
      .map(|piece| (piece.path, js::units(&piece.text)))
      .collect()
  }

  /// A room whose book was written as one document per person cut into the two layers the rest of the brain reads:
  /// per person, the writer cuts their document a stretch at a time into pieces, each stretch measured against what
  /// its pieces hold, then the sheet judgement writes the standing sheet over the document. Answers whether any part
  /// failed.
  #[allow(clippy::too_many_lines)]
  pub async fn split_chat(&self, chat_id: i64, only: Option<i64>, budget: usize) -> Result<bool, Stopped> {
    let people_rel = people_dir_of(chat_id);
    let people_dir = self.brain().join(&people_rel);
    let title = chat_title(self.ground.receiver, chat_id).await?;
    let Some(names) = sheet_names(&people_dir) else {
      eprintln!("people split: chat {chat_id} has no person book to split");
      return Ok(true);
    };
    let names: Vec<String> = names
      .into_iter()
      .filter(|name| only.is_none_or(|only| *name == format!("{only}.md")))
      .collect();

    if names.is_empty() {
      eprintln!("people split: chat {chat_id} has no person documents to split");
      return Ok(true);
    }

    let mut failed = false;
    let mut left = budget;

    for name in names {
      let user_id: i64 = name.trim_end_matches(".md").parse().unwrap_or_default();
      let source = fs::read_to_string(people_dir.join(&name)).unwrap_or_default();
      let standing = self.chunks_of(&people_rel, user_id);

      // A person whose pieces carry no stretch number was cut in one pass; there is nothing to continue.
      if !standing.is_empty()
        && !standing.iter().any(|chunk| numbered_piece_slug(piece_slug(&chunk.path), user_id))
      {
        println!("people split: chat {chat_id} person {user_id} is already in pieces, leaving it");
        continue;
      }

      let spans = person_spans(&source, SPAN_CHARS);
      let recorded_on = self.last_recorded(&format!("{people_rel}/{name}"));
      let mut whole = true;
      let mut cut = false;

      for (at, span) in spans.iter().enumerate() {
        let number = at + 1;
        let prefix = span_prefix(user_id, number);
        // The pieces on disk are the record of how far the cut got, measured again rather than taken as done.
        let standing_span = self.pieces_of_span(chat_id, &prefix);
        let standing_kept = Self::said_in(&standing_span);

        if !standing_span.is_empty()
          && standing_kept as f64 >= said_length(span) as f64 * SPLIT_FIDELITY_FLOOR
          && self.all_rowed(&standing_span).await?
        {
          continue;
        }

        if !standing_span.is_empty() {
          println!(
            "people split: chat {chat_id} person {user_id} stretch {number} holds {standing_kept} of {} characters and was left half written; cutting it again",
            said_length(span)
          );
          self.drop_pieces(&standing_span);
        }

        if left == 0 {
          whole = false;
          break;
        }

        let before = self.piece_sizes(chat_id, user_id);
        let backup = backup_documents(self.brain());
        let mut named = BTreeSet::new();
        let places = vec![people_rel.clone()];
        let prompt = split_prompt(
          self.ground.prompts(),
          &self.recollection.label,
          chat_id,
          title.as_deref(),
          &Stretch {
            user_id,
            span,
            number,
            count: spans.len(),
            recorded_on: &recorded_on,
          },
        )
        .await?;
        // A cut writes this room's part and nothing else; the common part is sorted later.
        let ran = self
          .run_writer(
            chat_id,
            &prompt,
            &format!(
              "people split: chat {chat_id} cut stretch {number}/{} of {name} ({} characters)",
              spans.len(),
              js::units(span)
            ),
            WriterDesk {
              room: chat_id,
              places: places.clone(),
              reach: self.book_reach(chat_id, &[user_id], true),
              beside: None,
              carried: vec![],
              before: &backup,
              named: &mut named,
            },
            &people_rel,
            None,
          )
          .await?;
        let claim = RunClaim { places, named };
        let outcome = match ran {
          Wrote::Stood(writes) => {
            self.hold_writes(&backup, &claim, None, writes, &|path| path.to_owned()).await?
          }
          failed => failed,
        };

        left -= 1;

        let writes = match outcome {
          Wrote::Stood(writes) => writes,
          Wrote::Failed(reason) => {
            eprintln!("people split: chat {chat_id} person {user_id} stretch {number} failed: {reason}");
            failed = true;
            whole = false;
            break;
          }
        };
        // A cut that lost most of its stretch is a summary; a run that reached into a stretch already standing is
        // thrown away on the same terms. Either way only this stretch's pieces go.
        let written = self.pieces_of_span(chat_id, &prefix);
        let kept = Self::said_in(&written);
        let span_said = said_length(span);
        let disturbed = self
          .piece_sizes(chat_id, user_id)
          .into_iter()
          .filter(|(path, size)| before.iter().any(|(was, was_size)| was == path && was_size != size))
          .count();

        if (kept as f64) < span_said as f64 * SPLIT_FIDELITY_FLOOR || disturbed > 0 {
          self.drop_pieces(&written);

          if disturbed > 0 {
            eprintln!(
              "people split: chat {chat_id} person {user_id} stretch {number} rewrote {disturbed} piece(s) of an earlier stretch; its own pieces are dropped and the document stands"
            );
          } else {
            eprintln!(
              "people split: chat {chat_id} person {user_id} stretch {number} came back with {kept} of {span_said} characters, which is a summary rather than a cut; its pieces are dropped and the document stands"
            );
          }

          failed = true;
          whole = false;
          break;
        }

        // The stretch stands, so the rows its run wrote go into the table now: the next stretch resumes from them.
        self.follow_writes(&writes, chat_id).await?;
        cut = true;
        println!(
          "people split: chat {chat_id} person {user_id} stretch {number}/{} kept {kept} of {span_said} characters across {} pieces",
          spans.len(),
          written.len()
        );
      }

      // The sheet is written over the document only once every stretch of it stands.
      if !whole {
        continue;
      }

      if !cut {
        println!("people split: chat {chat_id} person {user_id} is already in pieces, leaving it");
        continue;
      }

      let pieces = self.chunks_of(&people_rel, user_id);
      let kept: usize = pieces.iter().map(|piece| js::units(&piece.text)).sum();

      if (kept as f64) < js::units(&source) as f64 * SPLIT_FIDELITY_FLOOR
        || !self.all_rowed(&pieces).await?
      {
        eprintln!(
          "people split: chat {chat_id} person {user_id} is in {} pieces holding {kept} of {} characters, and either that does not add up or the table does not offer them all; the document stands and no sheet is written",
          pieces.len(),
          js::units(&source)
        );
        failed = true;
        continue;
      }

      println!(
        "people split: chat {chat_id} person {user_id} kept {kept} of {} characters across {} pieces",
        js::units(&source),
        pieces.len()
      );
      self.write_standing_sheets(chat_id, &[user_id], None, true, false).await?;
    }

    Ok(failed)
  }

  /// Every sheet of a room's people, both parts, written from the pieces standing on disk. Answers whether it failed.
  pub async fn sheets_for_chat(&self, chat_id: i64) -> Result<bool, Stopped> {
    let Some(ids) = room_book_people(self.brain(), &people_dir_of(chat_id)) else {
      eprintln!("people sheets: chat {chat_id} has no person book");
      return Ok(true);
    };

    self.write_standing_sheets(chat_id, &ids, None, false, true).await?;

    Ok(false)
  }

  /// A room's book written again from the start, out of the messages already folded into it, beside the book it
  /// replaces; only a book that stands whole takes the old one's place. Answers whether it failed.
  #[allow(clippy::too_many_lines)]
  pub async fn reform_chat(&self, chat_id: i64, allowed: i64) -> Result<bool, Stopped> {
    let ground = &self.ground;
    let agent = &self.recollection.agent;
    let people_rel = people_dir_of(chat_id);
    let staging_rel = format!("{ROOMS_ROOT}/{chat_id}/{REFORM_DIR}");
    let people_abs = self.brain().join(&people_rel);
    let staging_abs = self.brain().join(&staging_rel);
    let folded = read_count(ground.receiver, agent, chat_id).await?;
    let needed = (folded + BATCH_SIZE - 1) / BATCH_SIZE;

    if folded == 0 {
      eprintln!("people reform: chat {chat_id} has no folded messages to write a book from");
      return Ok(true);
    }

    // A book rewritten halfway cannot take the old one's place, so a room needing more batches is refused first.
    if needed > allowed {
      eprintln!(
        "people reform: chat {chat_id} has {} folded messages, which is {needed} batches and more than the {allowed} allowed; nothing was run",
        count_text(usize::try_from(folded).unwrap_or(usize::MAX))
      );
      return Ok(true);
    }

    let discard = |reason: String| {
      let _ = fs::remove_dir_all(&staging_abs);
      eprintln!("people reform: chat {chat_id} {reason}; the book stands as it was");
      true
    };

    let _ = fs::remove_dir_all(&staging_abs);
    let _ = fs::create_dir_all(staging_abs.join(CHUNKS_DIR));

    let bot_id = self.own_id_in(chat_id).await?;
    let mut senders: Vec<i64> = vec![];
    let beside = Beside {
      from: staging_rel.clone(),
      to: people_rel.clone(),
    };
    let mut staged: Vec<FragmentWrite> = vec![];
    let mut after = 0;

    for round in 1..=needed {
      let batch = read_messages(ground.receiver, agent, chat_id, after, BATCH_SIZE).await?;
      let Some(last) = batch.last() else { break };
      let backup = backup_documents(self.brain());
      let mut named = BTreeSet::new();
      let places = vec![staging_rel.clone()];
      let spoke = senders_of(&batch, bot_id);
      let allowed_dirs = |_: &[i64]| places.clone();
      let describe = |at: &str| {
        format!(
          "people reform: chat {chat_id} folded batch {round}/{needed} ({} messages){at}",
          count_text(batch.len())
        )
      };
      let mut writing = Writing {
        people_dir: staging_rel.clone(),
        book: Book::Disk,
        beside: Some(beside.clone()),
        carried: staged.clone(),
        common_read_only: true,
        unsplit: vec![],
        allowed_dirs: &allowed_dirs,
        named: &mut named,
        describe: &describe,
        step: None,
      };
      let outcome = self.write_batch(chat_id, &batch, &spoke, &mut writing).await;
      let claim = RunClaim {
        places: places.clone(),
        named: named.clone(),
      };

      match outcome {
        Err(stopped) => {
          restore_documents(self.brain(), &backup, &claim);
          let _ = fs::remove_dir_all(&staging_abs);
          return Err(stopped);
        }
        Ok(Wrote::Failed(reason)) => {
          restore_documents(self.brain(), &backup, &claim);
          return Ok(discard(format!("batch {round} failed: {reason}")));
        }
        Ok(Wrote::Stood(writes)) => staged.extend(writes),
      }

      for id in spoke {
        if !senders.contains(&id) {
          senders.push(id);
        }
      }

      after = last.message_id;
    }

    // Everyone the old book had a document for and the folded messages speak for must be in the new one.
    let _ = fs::create_dir_all(&people_abs);

    let documented: Vec<i64> = sheet_names(&people_abs)
      .unwrap_or_default()
      .iter()
      .filter_map(|name| name.trim_end_matches(".md").parse().ok())
      .collect();
    let lost: Vec<String> = documented
      .iter()
      .filter(|user_id| senders.contains(user_id) && self.chunks_of(&staging_rel, **user_id).is_empty())
      .map(i64::to_string)
      .collect();

    if !lost.is_empty() {
      return Ok(discard(format!("wrote no pieces for {}", lost.join(", "))));
    }

    let pieced: Vec<i64> = senders
      .iter()
      .copied()
      .filter(|user_id| !self.chunks_of(&staging_rel, *user_id).is_empty())
      .collect();

    self.write_standing_sheets(chat_id, &pieced, Some(&staging_rel), false, false).await?;

    let staged_names = sheet_names(&staging_abs).unwrap_or_default();
    let unsheeted: Vec<String> = pieced
      .iter()
      .filter(|user_id| !staged_names.contains(&format!("{user_id}.md")))
      .map(i64::to_string)
      .collect();

    if !unsheeted.is_empty() {
      return Ok(discard(format!("got no standing sheet for {}", unsheeted.join(", "))));
    }

    let before = backup_documents(self.brain());

    let _ = fs::remove_dir_all(people_abs.join(CHUNKS_DIR));
    let _ = fs::remove_file(people_abs.join("index.md"));
    // The new book's pieces have not been sorted into the two parts; the list said so of the old one.
    clear_sorted(self.brain(), &people_rel);

    for user_id in &documented {
      let _ = fs::remove_file(people_abs.join(format!("{user_id}.md")));
    }

    if let Ok(entries) = fs::read_dir(&staging_abs) {
      for entry in entries.flatten() {
        let _ = fs::rename(entry.path(), people_abs.join(entry.file_name()));
      }
    }

    let _ = fs::remove_dir_all(&staging_abs);

    // The old book's pieces are gone, so their rows go down; the new book's rows are its runs' writes, named where
    // their pieces now stand.
    let moved: Vec<FragmentWrite> = staged
      .iter()
      .map(|write| FragmentWrite {
        path: beside_path(&beside, &write.path),
        ..write.clone()
      })
      .collect();

    self.follow_writes(&moved, chat_id).await?;

    // What took the old book's place is cleaned and announced exactly as a batch's writing is.
    let written = run_touched(
      self.brain(),
      &before,
      &RunClaim {
        places: vec![people_rel.clone()],
        named: BTreeSet::new(),
      },
    )
    .changed;
    let paths: Vec<PathBuf> = written.iter().map(|path| self.brain().join(path)).collect();

    clean_documents(ground.gate, ground.brain_root, &paths).await;
    reload_written(ground.db, ground.agency, ground.receiver, &self.recollection, &paths).await;
    println!(
      "people reform: chat {chat_id} wrote its book again from {} folded messages: {} people, {} pieces",
      count_text(usize::try_from(folded).unwrap_or(usize::MAX)),
      pieced.len(),
      self.pieces_named(&people_rel, "p").len()
    );

    Ok(false)
  }

  /// A run over a whole room ends by committing what stands of its writing into the brain, the index files drawn
  /// from the table before the commit takes them. Answers whether the run failed.
  pub async fn whole_room(&self, chat_id: i64, run: WholeRoomRun, failed: bool) -> Result<bool, Failure> {
    let (paths, message) = whole_room_commit(self.brain(), chat_id, run);

    self.ground.table().await?.follow_write(&[], None, &self.recollection.agent).await?;

    let standing: Vec<String> = paths.into_iter().filter(|path| self.brain().join(path).exists()).collect();

    commit_paths(&standing, &message, self.brain()).await;

    Ok(failed)
  }
}
