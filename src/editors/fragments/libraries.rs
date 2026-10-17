//! The brain's libraries as the fragment table follows them (docs/fragments.md, 「The kinds, and where each row
//! comes from」): the three the editors write in every agent's folder, whose rows are the index and whose index
//! files are drawn out of them. Each library answers for itself — which paths are its pieces, which row a piece
//! answers to, where its index stands and how it is drawn — so nothing here asks which kind it holds. A shipped
//! library is no member: its owner hands its entries in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub use fragment_desk::libraries::Library as DeskLibrary;
use fragment_desk::libraries::PiecePlace as DeskPlace;
use regex::Regex;

use super::narrow::index_line;
use super::store::Row;
use crate::Failure;
use crate::editors::book::{
  CHUNKS_DIR, KNOWLEDGE_ROOT, PEOPLE_ROOT, ROOMS_ROOT, common_dir_of, people_dir_of,
};
use crate::editors::recollection::recollections;
use crate::ports::agency::Agency;

/// Which row one body path answers to: the agent whose memory it is where the folder says so, its room, its
/// person, its slug and its scope tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiecePlace {
  pub agent: Option<String>,
  pub chat_id: Option<i64>,
  pub owner: Option<i64>,
  pub slug: String,
  pub scope: Option<String>,
}

/// What a piece's write said of it, which a library whose scope the writer chooses reads: whether the line said
/// it is every room's, the room of the batch that wrote it, and the row already standing at its path.
pub struct Said<'a> {
  pub shared: Option<bool>,
  pub room: Option<i64>,
  pub standing: Option<&'a Row>,
}

/// Which of a library's rows a room may be offered (docs/fragments.md, 「What a room may be offered」), each read
/// among the active rows of the turn's agent and those marked for every agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offering {
  /// The rows of the room alone.
  Room,
  /// The rows of the room and those naming no room, as one index in position order.
  RoomWithShared,
  /// The room's part where the room keeps one, then the part naming no room of each person who has spoken in
  /// the room, by the person's number.
  RoomPartWithSpeakers,
}

/// A library its editors write, read inside one agent's folder: every path it is asked about is relative to
/// that folder. A new brain library is added as one more member. What the fragment desk asks of it — its kind,
/// which paths are its pieces, which row a piece answers to and what a refusal calls its pieces — it answers as a
/// member of the desk's own library trait, which the desk is handed the members of.
pub trait Library: DeskLibrary {
  /// Which of its rows a room may be offered.
  fn offering(&self) -> Offering;
  /// The word a screen calls the kind by.
  fn label(&self) -> &'static str;
  /// Whether a piece is finished once every item it is tied to is.
  fn settles(&self) -> bool;
  /// The row one of its pieces answers to.
  fn place(&self, path: &str, said: &Said<'_>) -> Option<PiecePlace>;
  /// The index file a row of this library stands in.
  fn index_of(&self, chat_id: Option<i64>, owner: Option<i64>) -> String;
  /// The header the index a row stands in opens with, written as the agent whose book it is calls itself.
  fn header(&self, row: &Row, writer: &str) -> String;
  /// The section a row stands under in its index, where the library groups its rows.
  fn section(&self, _row: &Row, _names: &HashMap<i64, Option<String>>) -> Option<String> {
    None
  }
}

static RULE_PATH: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^rooms/(-?[0-9]+)/rules/([a-z0-9-]+)\.md$").expect("a pattern"));
static KNOWLEDGE_CHUNK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^knowledge/([a-z0-9-]+)\.md$").expect("a pattern"));
static ROOM_PIECE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"^rooms/(-?[0-9]+)/people/chunks/(p([0-9]+)-[a-z0-9-]+)\.md$").expect("a pattern")
});
static COMMON_PIECE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^people/([0-9]+)/chunks/(c([0-9]+)-[a-z0-9-]+)\.md$").expect("a pattern"));

/// Every room's rulebook as the digest writes it. A rule holds until it is replaced, so none is ever finished.
pub struct RoomRules;

impl RoomRules {
  fn rule_at(path: &str) -> Option<(i64, String)> {
    let found = RULE_PATH.captures(path)?;

    (&found[2] != "index").then(|| Some((found[1].parse().ok()?, found[2].to_owned())))?
  }
}

impl DeskLibrary for RoomRules {
  fn kind(&self) -> &'static str {
    "room-rule"
  }

  fn owns(&self, path: &str) -> bool {
    Self::rule_at(path).is_some()
  }

  fn piece_place(&self, path: &str, shared: Option<bool>, room: Option<i64>) -> Option<DeskPlace> {
    on_desk(self, path, shared, room)
  }

  fn pieces(&self) -> &'static str {
    "a room rule chunk"
  }

  fn not_pieces(&self) -> &'static [&'static str] {
    &["an index.md"]
  }
}

impl Library for RoomRules {
  fn offering(&self) -> Offering {
    Offering::Room
  }

  fn label(&self) -> &'static str {
    "Room rules"
  }

  fn settles(&self) -> bool {
    false
  }

  fn place(&self, path: &str, _said: &Said<'_>) -> Option<PiecePlace> {
    let (chat_id, slug) = Self::rule_at(path)?;

    Some(PiecePlace {
      agent: None,
      chat_id: Some(chat_id),
      owner: None,
      slug,
      scope: None,
    })
  }

  fn index_of(&self, chat_id: Option<i64>, _owner: Option<i64>) -> String {
    format!("{ROOMS_ROOT}/{}/rules/index.md", chat_id.unwrap_or(0))
  }

  fn header(&self, row: &Row, _writer: &str) -> String {
    format!(
      "# Room rules — {}",
      row.chat_id.map_or_else(|| "null".to_owned(), |chat| chat.to_string())
    )
  }
}

/// The knowledge library: one index serving every room, each row carrying the scope that decides which room may
/// be offered it. Whether a chunk is every room's is the writer's to say; a line that does not say keeps the
/// scope the row already has, and a new chunk that does not say belongs to the room whose notes made it.
pub struct Knowledge;

impl Knowledge {
  fn chunk_slug(path: &str) -> Option<String> {
    KNOWLEDGE_CHUNK
      .captures(path)
      .map(|found| found[1].to_owned())
      .filter(|slug| slug != "index" && slug != "candidates")
  }

  /// The scope tag a knowledge row carries: `[shared]` for every room's, `[chat:<id>]` for one room's.
  pub fn scope(chat_id: Option<i64>) -> String {
    chat_id.map_or_else(|| "[shared]".to_owned(), |room| format!("[chat:{room}]"))
  }
}

impl DeskLibrary for Knowledge {
  fn kind(&self) -> &'static str {
    "library-knowledge"
  }

  fn owns(&self, path: &str) -> bool {
    Self::chunk_slug(path).is_some()
  }

  fn piece_place(&self, path: &str, shared: Option<bool>, room: Option<i64>) -> Option<DeskPlace> {
    on_desk(self, path, shared, room)
  }

  fn pieces(&self) -> &'static str {
    "a knowledge chunk"
  }

  fn not_pieces(&self) -> &'static [&'static str] {
    &["knowledge/candidates.md", "an index.md"]
  }
}

impl Library for Knowledge {
  fn offering(&self) -> Offering {
    Offering::RoomWithShared
  }

  fn label(&self) -> &'static str {
    "Knowledge"
  }

  fn settles(&self) -> bool {
    true
  }

  fn place(&self, path: &str, said: &Said<'_>) -> Option<PiecePlace> {
    let slug = Self::chunk_slug(path)?;
    let chat_id = match (said.shared, said.standing) {
      (Some(true), _) => None,
      (Some(false), _) => said.room,
      (None, Some(standing)) => standing.chat_id,
      (None, None) => said.room,
    };

    if chat_id.is_none()
      && said.shared != Some(true)
      && said.standing.is_none_or(|standing| standing.chat_id.is_some())
    {
      return None;
    }

    Some(PiecePlace {
      agent: None,
      chat_id,
      owner: None,
      slug,
      scope: Some(Self::scope(chat_id)),
    })
  }

  fn index_of(&self, _chat_id: Option<i64>, _owner: Option<i64>) -> String {
    format!("{KNOWLEDGE_ROOT}/index.md")
  }

  fn header(&self, _row: &Row, _writer: &str) -> String {
    "# Knowledge index".to_owned()
  }
}

/// The person book as its writers write it: the room part, one index per room holding the pieces of everyone in
/// it, and the common part, one index per person. A piece about a matter tied to items is finished once they all
/// are.
pub struct PersonBook;

impl PersonBook {
  fn piece_at(path: &str) -> Option<(Option<i64>, i64, String)> {
    if let Some(room) = ROOM_PIECE.captures(path) {
      return Some((Some(room[1].parse().ok()?), room[3].parse().ok()?, room[2].to_owned()));
    }

    let common = COMMON_PIECE.captures(path)?;

    (common[1] == common[3]).then(|| Some((None, common[1].parse().ok()?, common[2].to_owned())))?
  }

  /// The header a room index carries, so every room's book states what it is, whose it is and what belongs in
  /// it.
  pub fn room_header(chat_id: i64, writer: &str) -> String {
    [
      format!("# Person book (room {chat_id})"),
      String::new(),
      format!("The room-only part of what {writer} writes about each person from this room's conversations. The cast"),
      format!("table in the room's persona is the owner's own; this book is what {writer} fills in. The same person's"),
      format!("common part is kept apart under `{PEOPLE_ROOT}/<userId>/`, and what goes in which part is"),
      "set out in `docs/people-book.md`.".to_owned(),
      String::new(),
      "- This part holds what is sensitive outside this room: how a person is addressed here, their".to_owned(),
      "  relations with this room's people, this room's topics and promises. When unclear, it is room-only.".to_owned(),
      format!("- A piece is one heading or one topic of one person, at {CHUNKS_DIR}/<slug>.md with the slug"),
      "  `p<userId>-<topic>`. The fixed topics are Address `address`, Relations `relations` and Ongoing and".to_owned(),
      "  promises `ongoing`; Address, a basic heading, stays short.".to_owned(),
      "- A piece is English lists, never paragraphs: `## Facts`, `## Views`, `## Judgements`. Facts are".to_owned(),
      format!("  what the room showed, Views are what the person said they think, Judgements are {writer}'s readings,"),
      "  and the three never mix.".to_owned(),
      "- Every item opens with `[date D · confirmed D]` (basis in place of date on a Judgement). A day is".to_owned(),
      "  `YYYY-MM-DD` in Korean time, or `unknown`.".to_owned(),
      "- Names, forms of address, examples of a person's way of talking, short phrases of theirs and Korean words".to_owned(),
      "  English would blur stay in Korean.".to_owned(),
      "- `<userId>.md` is that person's room sheet, rewritten by the summariser from this room's pieces.".to_owned(),
      "  The writer never touches it.".to_owned(),
      "- No message is copied out, nothing of another room is written, and no credentials, phone numbers,".to_owned(),
      "  addresses or medical details.".to_owned(),
      "- This is knowledge of how to talk with people, not material for answering about anyone's private life.".to_owned(),
      "- Below, drawn from the fragment table, one section per person (`## <userId> — <name>`), and under".to_owned(),
      "  it one line per piece (`- <slug> — <subject nouns> · <period>`). The writer never edits this index:".to_owned(),
      "  each piece's row is written through the fragment tool by the run that writes the piece, and this".to_owned(),
      "  file is drawn again.".to_owned(),
    ]
    .join("\n")
  }

  /// The header every person's common index opens with: a common document, so it names no room and carries no id.
  pub fn common_header(writer: &str) -> String {
    [
      "# Person book — common".to_owned(),
      String::new(),
      format!("The common part of what {writer} writes about one person: what is the same in every room. Every room"),
      "the person has spoken in reads it. What goes in which part is set out in `docs/people-book.md`.".to_owned(),
      String::new(),
      "- This part holds Personality, Values, Speech, Habits, Working requests, Told to remember,".to_owned(),
      "  Interests, Dislikes and cautions, and Humor. The basic headings (Personality, Values, Speech,".to_owned(),
      "  Habits, Working requests) stay short, a few items of a line or two.".to_owned(),
      "- No other person is named, by name or by user id, and no room name or chat id is written.".to_owned(),
      "  What passes between people and a room's affairs belong to the room-only part.".to_owned(),
      format!("- A piece is one heading or one topic, at {CHUNKS_DIR}/<slug>.md with the slug"),
      "  `c<userId>-<topic>`.".to_owned(),
      "- A piece is English lists: `## Facts`, `## Views`, `## Judgements`, every item opening with".to_owned(),
      "  `[date D · confirmed D]` (basis in place of date on a Judgement). A day is `YYYY-MM-DD` in Korean".to_owned(),
      "  time, or `unknown`.".to_owned(),
      "- Names, examples of the person's way of talking, short phrases of theirs and Korean words English".to_owned(),
      "  would blur stay in Korean.".to_owned(),
      "- `sheet.md` is the person's common sheet, rewritten by the summariser from the common".to_owned(),
      "  pieces alone. The writer never touches it.".to_owned(),
      "- Below, drawn from the fragment table, this person's one section (`## <userId> — <Telegram display".to_owned(),
      "  name>`), and under it one line per piece (`- <slug> — <subject nouns> · <period>`). The writer never".to_owned(),
      "  edits this index: each piece's row is written through the fragment tool by the run that writes it.".to_owned(),
    ]
    .join("\n")
  }
}

impl DeskLibrary for PersonBook {
  fn kind(&self) -> &'static str {
    "person-book"
  }

  fn owns(&self, path: &str) -> bool {
    Self::piece_at(path).is_some()
  }

  fn piece_place(&self, path: &str, shared: Option<bool>, room: Option<i64>) -> Option<DeskPlace> {
    on_desk(self, path, shared, room)
  }

  fn pieces(&self) -> &'static str {
    "a person-book piece under chunks/"
  }

  fn not_pieces(&self) -> &'static [&'static str] {
    &["a standing sheet", "an index.md"]
  }
}

impl Library for PersonBook {
  fn offering(&self) -> Offering {
    Offering::RoomPartWithSpeakers
  }

  fn label(&self) -> &'static str {
    "Person book"
  }

  fn settles(&self) -> bool {
    true
  }

  fn place(&self, path: &str, _said: &Said<'_>) -> Option<PiecePlace> {
    let (chat_id, owner, slug) = Self::piece_at(path)?;

    Some(PiecePlace {
      agent: None,
      chat_id,
      owner: Some(owner),
      slug,
      scope: None,
    })
  }

  fn index_of(&self, chat_id: Option<i64>, owner: Option<i64>) -> String {
    match chat_id {
      None => format!("{}/index.md", common_dir_of(owner.unwrap_or(0))),
      Some(chat_id) => format!("{}/index.md", people_dir_of(chat_id)),
    }
  }

  fn header(&self, row: &Row, writer: &str) -> String {
    match row.chat_id {
      None => Self::common_header(writer),
      Some(chat_id) => Self::room_header(chat_id, writer),
    }
  }

  fn section(&self, row: &Row, names: &HashMap<i64, Option<String>>) -> Option<String> {
    let name = row.owner.and_then(|owner| names.get(&owner).cloned().flatten());

    Some(format!(
      "## {} — {}",
      row.owner.map_or_else(|| "null".to_owned(), |owner| owner.to_string()),
      name.as_deref().unwrap_or("unknown")
    ))
  }
}

/// The libraries the editors write, in the order they are asked.
pub const LIBRARIES: [&dyn Library; 3] = [&RoomRules, &Knowledge, &PersonBook];

/// The same libraries as a fragment desk is handed them, in the same order.
pub fn on_the_desk() -> Vec<&'static dyn DeskLibrary> {
  LIBRARIES.into_iter().map(|library| library as &'static dyn DeskLibrary).collect()
}

// The row a piece answers to as the desk asks it: what the write said of its scope and the batch's room, with no
// standing row read, since the desk lays the batch's writes over the table itself.
fn on_desk(
  library: &dyn Library,
  path: &str,
  shared: Option<bool>,
  room: Option<i64>,
) -> Option<DeskPlace> {
  library
    .place(
      path,
      &Said {
        shared,
        room,
        standing: None,
      },
    )
    .map(|placed| DeskPlace {
      owner: placed.owner,
      slug: placed.slug,
      scope: placed.scope,
    })
}

/// Whether a kind is one of the brain's own libraries.
pub fn brain_kind(kind: &str) -> bool {
  LIBRARIES.iter().any(|library| library.kind() == kind)
}

/// One agent's folder of the brain, with the name its documents call it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFolder {
  pub agent: String,
  pub folder: String,
  pub label: Option<String>,
}

/// The brain as its libraries stand in it: its root and every agent's folder, the chat agent's at the root among
/// them even before its registration says so. Each library reads paths relative to one folder; this reads them
/// relative to the brain root, each piece the memory of the agent whose folder it lies in.
pub struct Brain {
  pub root: PathBuf,
  pub folders: Vec<AgentFolder>,
}

impl Brain {
  /// The brain at a root with these folders; `chat_agent` names the agent whose memory the root is where no
  /// folder stands there.
  pub fn new(root: PathBuf, chat_agent: &str, mut folders: Vec<AgentFolder>) -> Self {
    for one in &mut folders {
      one.folder = one.folder.trim_end_matches('/').to_owned();
    }

    if !folders.iter().any(|one| one.folder.is_empty()) {
      folders.insert(
        0,
        AgentFolder {
          agent: chat_agent.to_owned(),
          folder: String::new(),
          label: None,
        },
      );
    }

    Self { root, folders }
  }

  /// The brain at a root with every awake agent's folder the agency registers, each with its label; `chat_key`
  /// is the chat agent's registration key, whose memory is the root.
  pub async fn of(agency: &dyn Agency, root: &Path, chat_key: &str) -> Result<Self, Failure> {
    let folders: Vec<AgentFolder> = recollections(agency, root, chat_key)
      .await?
      .into_iter()
      .map(|one| AgentFolder {
        agent: one.agent,
        folder: one.folder,
        label: Some(one.label),
      })
      .collect();

    Ok(Self::new(root.to_path_buf(), chat_key, folders))
  }

  /// Why the brain cannot be read as the brain now, or nothing: a brain that is no git repository lost what it
  /// held with its history.
  pub fn fault(&self) -> Option<String> {
    (!self.root.join(".git").exists()).then(|| {
      format!(
        "the brain at {} is no git repository, so what stands there cannot be told from what it lost",
        self.root.display()
      )
    })
  }

  /// Where a path relative to the brain root is read from.
  pub fn locate(&self, path: &str) -> PathBuf {
    self.root.join(path)
  }

  /// The agent whose memory the brain root is.
  pub fn root_agent(&self) -> &str {
    self
      .folders
      .iter()
      .find(|one| one.folder.is_empty())
      .map_or("", |one| one.agent.as_str())
  }

  /// Whose memory a path relative to the brain root is, the folder that holds it, and the path relative to that
  /// folder: the agent whose folder is the longest that holds it, the root's agent for any other.
  pub fn agent_of<'a>(&'a self, path: &'a str) -> (&'a str, &'a str, &'a str) {
    let mut folders: Vec<&AgentFolder> = self.folders.iter().collect();

    folders.sort_by_key(|one| std::cmp::Reverse(one.folder.len()));

    for one in folders {
      if one.folder.is_empty() {
        return (&one.agent, "", path);
      }

      if let Some(inner) = path.strip_prefix(&format!("{}/", one.folder)) {
        return (&one.agent, &one.folder, inner);
      }
    }

    (self.root_agent(), "", path)
  }

  /// The folder of an agent, the root's where the brain names none for it.
  pub fn folder_of(&self, agent: &str) -> &str {
    self.folders.iter().find(|one| one.agent == agent).map_or("", |one| one.folder.as_str())
  }

  /// The name a document of an agent's memory calls the agent that writes it: the label its registration
  /// carries, or its name where it carries none.
  pub fn writer_name(&self, agent: &str) -> String {
    self
      .folders
      .iter()
      .find(|one| one.agent == agent)
      .and_then(|one| one.label.clone())
      .filter(|label| !label.is_empty())
      .unwrap_or_else(|| agent.to_owned())
  }

  /// The library a brain path is a piece of, read inside the folder that holds it.
  pub fn library_owning(&self, path: &str) -> Option<&'static dyn Library> {
    let (_, _, inner) = self.agent_of(path);

    LIBRARIES.into_iter().find(|library| library.owns(inner))
  }

  /// The row a brain path of a library answers to, naming the agent whose folder it lies in.
  pub fn place(&self, library: &dyn Library, path: &str, said: &Said<'_>) -> Option<PiecePlace> {
    let (agent, _, inner) = self.agent_of(path);

    library.place(inner, said).map(|placed| PiecePlace {
      agent: Some(agent.to_owned()),
      ..placed
    })
  }

  /// The index file a row of a library stands in, relative to the brain root.
  pub fn index_of(
    &self,
    library: &dyn Library,
    chat_id: Option<i64>,
    owner: Option<i64>,
    agent: &str,
  ) -> String {
    let inner = library.index_of(chat_id, owner);
    let folder = self.folder_of(agent);

    if folder.is_empty() { inner } else { format!("{folder}/{inner}") }
  }

  /// Each index file of a library drawn out of its rows, by its path relative to the brain root: every folder's
  /// own rows under that folder's indexes.
  pub fn draw(
    &self,
    library: &dyn Library,
    rows: &[Row],
    names: &HashMap<i64, Option<String>>,
  ) -> Vec<(String, String)> {
    let mut drawn = vec![];

    for one in &self.folders {
      let mut by_index: Vec<(String, Vec<&Row>)> = vec![];

      for row in rows.iter().filter(|row| row.agent == one.agent) {
        let index = library.index_of(row.chat_id, row.owner);

        match by_index.iter_mut().find(|(path, _)| *path == index) {
          Some((_, members)) => members.push(row),
          None => by_index.push((index, vec![row])),
        }
      }

      for (index, members) in by_index {
        let header = library.header(members[0], &self.writer_name(&one.agent));
        let text = drawn_index(&header, &members, |row| library.section(row, names));
        let path = if one.folder.is_empty() { index } else { format!("{}/{index}", one.folder) };

        drawn.push((path, text));
      }
    }

    drawn
  }
}

/// The rows of one index in the order they stand, written out under a header: every active row as its line,
/// grouped under a heading per section where the library has sections, a section following the first of its rows.
pub fn drawn_index(header: &str, rows: &[&Row], section: impl Fn(&Row) -> Option<String>) -> String {
  let mut standing: Vec<&&Row> = rows.iter().filter(|row| row.active).collect();

  standing.sort_by_key(|row| (row.position, row.id));

  let mut lines = vec![header.trim_end().to_owned(), String::new()];
  let mut sections: Vec<(String, Vec<String>)> = vec![];

  for row in standing {
    let line = index_line(row);

    match section(row) {
      None => lines.push(line),
      Some(name) => match sections.iter_mut().find(|(held, _)| *held == name) {
        Some((_, members)) => members.push(line),
        None => sections.push((name, vec![line])),
      },
    }
  }

  for (name, members) in sections {
    lines.push(name);
    lines.push(String::new());
    lines.extend(members);
    lines.push(String::new());
  }

  format!("{}\n", lines.join("\n").trim_end())
}
