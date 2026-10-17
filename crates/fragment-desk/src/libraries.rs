//! The libraries an editor writes, as the desk asks them about a path, read in every agent's folder
//! (docs/fragments.md, 「An editor writes the row through the fragment tool」). The desk names no library of its
//! own: its caller hands in the members, each answering for itself whether a path is one of its pieces and which
//! row the piece answers to. A desk handed none takes no piece.

use crate::rows::AgentFolder;

/// Which row one body path answers to, bar its kind.
#[derive(Clone, Debug, PartialEq)]
pub struct PiecePlace {
  pub owner: Option<i64>,
  pub slug: String,
  pub scope: Option<String>,
}

/// A library its editors write, as the desk asks it: every path it is asked about is relative to the folder of
/// the agent whose memory it is.
pub trait Library: Sync {
  /// The kind its rows carry.
  fn kind(&self) -> &'static str;
  /// Whether a path is one of its pieces.
  fn owns(&self, path: &str) -> bool;
  /// The row one of its pieces answers to, given whether the write said the piece is every room's and the room of
  /// the batch that wrote it; none where the piece cannot stand as a row.
  fn piece_place(&self, path: &str, shared: Option<bool>, room: Option<i64>) -> Option<PiecePlace>;
  /// What its pieces are called, as a refusal of a path no library owns names them.
  fn pieces(&self) -> &'static str;
  /// The files beside its pieces that are no pieces and have no row, as that refusal names them.
  fn not_pieces(&self) -> &'static [&'static str] {
    &[]
  }
}

/// Every agent's folder, the chat agent's at the brain root among them even where the folders handed in
/// name none there, and the libraries the caller handed in, in the order they are asked.
pub struct Folders {
  folders: Vec<AgentFolder>,
  libraries: Vec<&'static dyn Library>,
}

impl Folders {
  /// The folders as the agency's registration names them, each without a closing `/`, and the chat
  /// agent's key for the brain root; holding no library until some are handed in.
  pub fn new(read: Vec<AgentFolder>, agent_key: &str) -> Self {
    let read: Vec<AgentFolder> = read
      .into_iter()
      .map(|one| AgentFolder {
        folder: one.folder.trim_end_matches('/').to_owned(),
        ..one
      })
      .collect();
    let folders = if read.iter().any(|one| one.folder.is_empty()) {
      read
    } else {
      [AgentFolder {
        agent: agent_key.to_owned(),
        folder: String::new(),
      }]
      .into_iter()
      .chain(read)
      .collect()
    };

    Self {
      folders,
      libraries: vec![],
    }
  }

  /// The same folders holding these libraries, asked in this order.
  pub fn holding(self, libraries: Vec<&'static dyn Library>) -> Self {
    Self { libraries, ..self }
  }

  /// Whose memory a path relative to the brain root is, and the path relative to that agent's
  /// folder: the agent whose folder is the longest that holds it.
  pub fn agent_of<'a>(&'a self, path: &'a str) -> (&'a str, &'a str) {
    let mut folders: Vec<&AgentFolder> = self.folders.iter().collect();

    folders.sort_by_key(|one| std::cmp::Reverse(one.folder.len()));

    for one in folders {
      if one.folder.is_empty() {
        return (&one.agent, path);
      }

      if let Some(inner) = path.strip_prefix(&one.folder).and_then(|rest| rest.strip_prefix('/')) {
        return (&one.agent, inner);
      }
    }

    ("", path)
  }

  /// The written library a brain path is a piece of, read inside the folder that holds it.
  pub fn library_owning(&self, path: &str) -> Option<&'static dyn Library> {
    let (_, inner) = self.agent_of(path);

    self.libraries.iter().copied().find(|library| library.owns(inner))
  }

  /// The row a piece of a library answers to.
  pub fn place(
    &self,
    library: &dyn Library,
    path: &str,
    shared: Option<bool>,
    room: Option<i64>,
  ) -> Option<PiecePlace> {
    library.piece_place(self.agent_of(path).1, shared, room)
  }

  /// Why a path is no piece of any library held: what each library's pieces are called, and the files beside
  /// them that are not pieces.
  pub fn no_piece(&self, path: &str) -> String {
    if self.libraries.is_empty() {
      return format!("{path} is no piece of a brain library: this desk was handed none.");
    }

    let pieces: Vec<&str> = self.libraries.iter().map(|library| library.pieces()).collect();
    let mut beside: Vec<&str> = vec![];

    for name in self.libraries.iter().flat_map(|library| library.not_pieces().iter().copied()) {
      if !beside.contains(&name) {
        beside.push(name);
      }
    }

    let mut said = format!("{path} is no piece of a brain library: {}.", listed(&pieces, "or"));

    if !beside.is_empty() {
      let verb = if beside.len() == 1 { "is no piece and has" } else { "are not pieces and have" };

      said.push_str(&format!(" {} {verb} no row.", capitalized(&listed(&beside, "and"))));
    }

    said
  }
}

// Words listed as a sentence lists them: `a`, `a or b`, `a, b or c`.
fn listed(words: &[&str], last: &str) -> String {
  match words {
    [] => String::new(),
    [one] => (*one).to_owned(),
    [rest @ .., final_one] => format!("{} {last} {final_one}", rest.join(", ")),
  }
}

fn capitalized(text: &str) -> String {
  let mut characters = text.chars();

  characters
    .next()
    .map_or_else(String::new, |first| first.to_uppercase().chain(characters).collect())
}

/// A path relative to an agent's folder, as a path relative to the brain root.
pub fn in_folder(folder: &str, path: &str) -> String {
  if folder.is_empty() { path.to_owned() } else { format!("{folder}/{path}") }
}

/// A path relative to the brain root, as a path relative to the folder that holds it.
pub fn out_of_folder(folder: &str, path: &str) -> String {
  if folder.is_empty() {
    return path.to_owned();
  }

  path
    .strip_prefix(folder)
    .and_then(|rest| rest.strip_prefix('/'))
    .unwrap_or(path)
    .to_owned()
}
