//! What the person-book writer is told (docs/people-book.md, 「The writer's prompt carries what it reads」): a batch
//! of a room's messages to fold into both parts of the book of everyone it was about, one stretch of an older
//! single document to cut into pieces, or one group of a person's older room pieces to sort into the two parts —
//! each a task document of its own under the prompts' `people/` folder, behind the rulebooks every such editor
//! reads.

use brain_support::js;

use super::super::archive::Message;
use super::super::book::{
  CHUNKS_DIR, FIXED_CEILING_CHARS, PEOPLE_ROOT, PersonPiece, people_dir_of, piece_slug, span_prefix,
};
use super::super::prompts::{
  PromptFailure, Prompts, forbidden_titles_text, fragment_writer, seoul_timestamp,
};

/// The folder under the prompts the person-book writer's tasks are in.
const TASKS: &str = "people";

/// What a run of the person-book writer must end its answer with for what it wrote to be trusted.
pub const SUCCESS_MARKER: &str = "PEOPLE OK";

fn title_suffix(chat_title: Option<&str>) -> String {
  chat_title.map_or_else(String::new, |title| format!(" (title: {title})"))
}

/// One message as the writer reads it: its number, when it was said, who said it and what, and what it answers.
pub fn message_line(message: &Message) -> String {
  let sender = message.sender_id.map_or_else(|| "unknown".to_owned(), |id| id.to_string());
  let reply = message.reply_to_message_id.map_or_else(String::new, |id| format!(" [replies to {id}]"));

  format!(
    "{} | {} | {sender} | {} | {}{reply}",
    message.message_id,
    seoul_timestamp(message.sent_at),
    message.sender_name,
    message.text
  )
}

/// How one batch's writer is pointed at the book.
#[derive(Default)]
pub struct BatchOptions<'a> {
  /// Every room title the archive holds, which the common part may not carry.
  pub forbidden_titles: &'a [String],
  /// The room part's directory; the room's own people directory unless a rewrite writes beside it.
  pub people_dir: Option<&'a str>,
  /// A rewrite of one room's part may not change the common part every room shares.
  pub common_read_only: bool,
  /// Senders whose room book is still being cut, whose room pieces are merged into but not sorted.
  pub unsplit_senders: &'a [i64],
  /// The pieces of the people this run writes handed over whole.
  pub documents: &'a [PersonPiece],
  /// The senders this run writes for, where the batch's people are shared out between runs.
  pub writers: Option<&'a [i64]>,
}

/// A count of documents as the writer is told it, in words.
pub fn document_count_text(count: usize) -> String {
  match count {
    0 => "no documents".to_owned(),
    1 => "1 document".to_owned(),
    count => format!("{count} documents"),
  }
}

// What a rewrite's writer is told in place of sorting: the common part is read and left alone, and everything the
// batch shows goes into the room pieces, to be sorted once the book stands.
fn common_read_only_note() -> String {
  [
    String::new(),
    String::new(),
    "**In this run the common part is read-only.** This run writes this room's room-only part again".to_owned(),
    "from the start, beside the book the room reads, and the common part is shared by every room, so".to_owned(),
    format!(
      "nothing under {PEOPLE_ROOT}/ may change. It stands above as it is; write every statement of this batch"
    ),
    "into the room-only pieces — what belongs to the common part included, since it is sorted into".to_owned(),
    "the common part once this book stands — leaving out only what a common piece already says. No".to_owned(),
    "room-only piece is sorted, moved or deleted in this run.".to_owned(),
  ]
  .join("\n")
}

fn ids_text(ids: &[i64]) -> String {
  if ids.is_empty() {
    "none".to_owned()
  } else {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")
  }
}

/// What the writer is told to do with one batch of a room's messages: both parts of the book of every sender it
/// writes for, in the folder of the agent whose book it is, named by its label. `own_id` is the agent's own
/// number in the room, whose messages write nobody's book.
pub async fn people_prompt(
  prompts: Prompts<'_>,
  agent: &str,
  chat_id: i64,
  chat_title: Option<&str>,
  own_id: i64,
  batch: &[Message],
  options: &BatchOptions<'_>,
) -> Result<String, PromptFailure> {
  let mut senders: Vec<i64> = vec![];

  for sender in batch.iter().filter_map(|message| message.sender_id).filter(|id| *id != own_id) {
    if !senders.contains(&sender) {
      senders.push(sender);
    }
  }

  let writers = options.writers.map_or(senders, <[i64]>::to_vec);
  let people_dir = options.people_dir.map_or_else(|| people_dir_of(chat_id), str::to_owned);
  let documents = if options.documents.is_empty() {
    "None of them exists yet.".to_owned()
  } else {
    options
      .documents
      .iter()
      .map(|piece| format!("[{}]\n{}", piece.path, js::trim(&piece.text)))
      .collect::<Vec<_>>()
      .join("\n\n")
  };
  let messages = batch.iter().map(message_line).collect::<Vec<_>>().join("\n");

  fragment_writer(
    prompts,
    TASKS,
    "task.md",
    &[
      ("agent", agent),
      ("chatId", &chat_id.to_string()),
      ("chatTitleSuffix", &title_suffix(chat_title)),
      ("botId", &own_id.to_string()),
      ("count", &batch.len().to_string()),
      ("messages", &messages),
      ("peopleDir", &people_dir),
      ("chunksDir", CHUNKS_DIR),
      ("commonRoot", PEOPLE_ROOT),
      ("unsplitSenders", &ids_text(options.unsplit_senders)),
      ("writers", &ids_text(&writers)),
      ("documentCount", &document_count_text(options.documents.len())),
      ("documents", &documents),
      (
        "commonMode",
        &if options.common_read_only { common_read_only_note() } else { String::new() },
      ),
      ("forbiddenTitles", &forbidden_titles_text(options.forbidden_titles)),
      ("successMarker", SUCCESS_MARKER),
    ],
  )
  .await
}

/// What one stretch of a person's older single document is, as its cut is told it.
pub struct Stretch<'a> {
  pub user_id: i64,
  pub span: &'a str,
  pub number: usize,
  pub count: usize,
  pub recorded_on: &'a str,
}

/// What the writer is told to do with one stretch of one person's older single document.
pub async fn split_prompt(
  prompts: Prompts<'_>,
  agent: &str,
  chat_id: i64,
  chat_title: Option<&str>,
  stretch: &Stretch<'_>,
) -> Result<String, PromptFailure> {
  let span_chars = js::units(stretch.span);
  // The arithmetic of the cut, said out loud: a run told only the target size of a piece decides for itself how
  // many there are, and decides low.
  let pieces = ((span_chars as f64 / 1_100.0).round() as i64).max(2);

  fragment_writer(
    prompts,
    TASKS,
    "split.md",
    &[
      ("agent", agent),
      ("chatId", &chat_id.to_string()),
      ("chatTitleSuffix", &title_suffix(chat_title)),
      ("userId", &stretch.user_id.to_string()),
      ("span", stretch.span),
      ("spanNumber", &format!("{:02}", stretch.number)),
      ("spanCount", &stretch.count.to_string()),
      ("spanChars", &span_chars.to_string()),
      ("recordedOn", stretch.recorded_on),
      ("prefix", &span_prefix(stretch.user_id, stretch.number)),
      ("pieces", &pieces.to_string()),
      ("peopleDir", &people_dir_of(chat_id)),
      ("chunksDir", CHUNKS_DIR),
      ("successMarker", SUCCESS_MARKER),
    ],
  )
  .await
}

/// What the writer is told to do with one group of one person's older room pieces: sort them into the two parts.
#[allow(clippy::too_many_arguments)]
pub async fn sort_prompt(
  prompts: Prompts<'_>,
  agent: &str,
  chat_id: i64,
  chat_title: Option<&str>,
  user_id: i64,
  display_name: Option<&str>,
  group: &[PersonPiece],
  forbidden_titles: &[String],
) -> Result<String, PromptFailure> {
  let display = display_name.map_or_else(
    || "unknown (use the name the common index already carries)".to_owned(),
    |name| format!("\"{name}\""),
  );
  let chars: usize = group.iter().map(|piece| js::units(&piece.text)).sum();
  let slugs = group.iter().map(|piece| piece_slug(&piece.path)).collect::<Vec<_>>().join(", ");
  let text = group
    .iter()
    .map(|piece| format!("[{}]\n{}", piece.path, js::trim(&piece.text)))
    .collect::<Vec<_>>()
    .join("\n\n");

  fragment_writer(
    prompts,
    TASKS,
    "sort.md",
    &[
      ("agent", agent),
      ("chatId", &chat_id.to_string()),
      ("chatTitleSuffix", &title_suffix(chat_title)),
      ("userId", &user_id.to_string()),
      ("displayName", &display),
      ("groupChars", &chars.to_string()),
      ("groupSlugs", &slugs),
      ("group", &text),
      ("peopleDir", &people_dir_of(chat_id)),
      ("chunksDir", CHUNKS_DIR),
      ("commonRoot", PEOPLE_ROOT),
      ("forbiddenTitles", &forbidden_titles_text(forbidden_titles)),
      ("successMarker", SUCCESS_MARKER),
    ],
  )
  .await
}

/// What one stretch of a fixed heading set aside is walked with.
pub struct TrimPrompt<'a> {
  pub prompts: Prompts<'a>,
  pub agent: &'a str,
  /// The heading's room, none for a heading of the common part.
  pub chat_id: Option<i64>,
  pub chat_title: Option<&'a str>,
  pub user_id: i64,
  pub display_name: Option<&'a str>,
  /// The name the book writes for the heading.
  pub heading: &'a str,
  pub piece_path: &'a str,
  /// The heading's piece as it stands.
  pub piece: &'a str,
  pub holding_path: &'a str,
  pub dropped_path: &'a str,
  pub chunks_path: &'a str,
  pub stretch: &'a str,
  pub item_count: usize,
  /// How many items are still set aside, this stretch's included.
  pub total_items: usize,
  pub item_states: &'a str,
  pub forbidden_titles: &'a [String],
}

// Which part a trimmed heading stands in, and what that part allows, as the trimming run is told.
fn part_note(trim: &TrimPrompt<'_>) -> String {
  match trim.chat_id {
    Some(_) => [
      format!(
        "This heading is room-only: it and every piece you write stand in {}, this room's",
        trim.chunks_path
      ),
      "part of the person's book, which only this room reads. Write room pieces of this person alone,"
        .to_owned(),
      format!("`p{}-<topic>`; nothing moves into the common part in this run.", trim.user_id),
    ]
    .join("\n"),
    None => [
      format!(
        "This heading is in the common part: it and every piece you write stand in {},",
        trim.chunks_path
      ),
      "which every room this person speaks in reads. Write common pieces of this person alone,".to_owned(),
      format!("`c{}-<topic>`.", trim.user_id),
      String::new(),
      "**The common part names nobody else and no room.** No other member by name or user id, no chat"
        .to_owned(),
      "id, no room title, no \"in this room\". A common piece carrying another person's user id, a chat"
        .to_owned(),
      "id or a room title fails the run, and everything it wrote is thrown away. Room titles are matched"
        .to_owned(),
      "letter for letter, some are everyday words, and none may appear anywhere in a common piece, so"
        .to_owned(),
      format!(
        "where an item would use one, say it in other words: {}.",
        forbidden_titles_text(trim.forbidden_titles)
      ),
    ]
    .join("\n"),
  }
}

/// The task of one run walking one stretch of a fixed heading set aside to be trimmed.
pub async fn trim_prompt(trim: &TrimPrompt<'_>) -> Result<String, PromptFailure> {
  let display = trim.display_name.map_or_else(
    || "unknown (use the name the heading's piece already carries)".to_owned(),
    |name| format!("\"{name}\""),
  );
  let place = match trim.chat_id {
    Some(chat_id) => format!("Chat id: {chat_id}{}", title_suffix(trim.chat_title)),
    None => "The person's common book, which every room they speak in reads.".to_owned(),
  };
  let today = chrono::Utc::now()
    .checked_add_signed(chrono::Duration::hours(9))
    .map_or_else(String::new, |now| now.format("%Y-%m-%d").to_string());
  let piece = js::trim(trim.piece);

  fragment_writer(
    trim.prompts,
    TASKS,
    "trim.md",
    &[
      ("agent", trim.agent),
      ("place", &place),
      ("userId", &trim.user_id.to_string()),
      ("displayName", &display),
      ("today", &today),
      ("heading", trim.heading),
      ("piecePath", trim.piece_path),
      ("stretchChars", &js::units(trim.stretch).to_string()),
      ("itemCount", &trim.item_count.to_string()),
      ("totalItems", &trim.total_items.to_string()),
      ("ceiling", &FIXED_CEILING_CHARS.to_string()),
      ("itemStates", trim.item_states),
      ("partNote", &part_note(trim)),
      ("chunksPath", trim.chunks_path),
      ("holdingPath", trim.holding_path),
      ("droppedPath", trim.dropped_path),
      ("successMarker", SUCCESS_MARKER),
      ("piece", if piece.is_empty() { "(the piece holds no item yet)" } else { piece }),
      ("stretch", js::trim(trim.stretch)),
    ],
  )
  .await
}
