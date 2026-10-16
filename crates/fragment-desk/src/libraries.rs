//! The brain libraries an editor writes, as the desk asks them about a path: whether the path is one of
//! their pieces, and which row it answers to, each read in every agent's folder
//! (`docs/agent-ownership.md`).

use std::sync::LazyLock;

use regex::Regex;

use crate::rows::AgentFolder;

/// Which row one body path answers to, bar its kind.
#[derive(Clone, Debug, PartialEq)]
pub struct PiecePlace {
  pub owner: Option<i64>,
  pub slug: String,
  pub scope: Option<String>,
}

/// A library its editors write: the kind its rows carry, and how it reads a path inside one agent's
/// folder.
pub struct Library {
  pub kind: &'static str,
  owns: fn(&str) -> bool,
  place: fn(&str, Option<bool>, Option<i64>) -> Option<PiecePlace>,
}

static RULE_PATH: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^rooms/(-?[0-9]+)/rules/([a-z0-9-]+)\.md$").unwrap());
static KNOWLEDGE_CHUNK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^knowledge/([a-z0-9-]+)\.md$").unwrap());
static ROOM_PERSON_PIECE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^rooms/(-?[0-9]+)/people/chunks/(p([0-9]+)-[a-z0-9-]+)\.md$").unwrap());
static COMMON_PERSON_PIECE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^people/([0-9]+)/chunks/(c([0-9]+)-[a-z0-9-]+)\.md$").unwrap());

fn rule_slug(path: &str) -> Option<String> {
  RULE_PATH.captures(path).map(|rule| rule[2].to_owned()).filter(|slug| slug != "index")
}

fn chunk_slug(path: &str) -> Option<String> {
  KNOWLEDGE_CHUNK
    .captures(path)
    .map(|chunk| chunk[1].to_owned())
    .filter(|slug| slug != "index" && slug != "candidates")
}

fn person_piece(path: &str) -> Option<(i64, String)> {
  if let Some(room) = ROOM_PERSON_PIECE.captures(path) {
    return Some((room[3].parse().ok()?, room[2].to_owned()));
  }

  let common = COMMON_PERSON_PIECE.captures(path)?;

  (common[1] == common[3]).then(|| Some((common[1].parse().ok()?, common[2].to_owned())))?
}

/// The three libraries an editor writes, in the order they are asked.
pub const LIBRARIES: [Library; 3] = [
  Library {
    kind: "room-rule",
    owns: |path| rule_slug(path).is_some(),
    place: |path, _, _| {
      rule_slug(path).map(|slug| PiecePlace {
        owner: None,
        slug,
        scope: None,
      })
    },
  },
  Library {
    kind: "library-knowledge",
    owns: |path| chunk_slug(path).is_some(),
    place: |path, shared, room| {
      // The scope is the writer's to choose; one that does not say belongs to the batch's room.
      let slug = chunk_slug(path)?;
      let chat_id = if shared == Some(true) { None } else { room };

      if chat_id.is_none() && shared != Some(true) {
        return None;
      }

      let scope = chat_id.map_or_else(|| "[shared]".to_owned(), |room| format!("[chat:{room}]"));

      Some(PiecePlace {
        owner: None,
        slug,
        scope: Some(scope),
      })
    },
  },
  Library {
    kind: "person-book",
    owns: |path| person_piece(path).is_some(),
    place: |path, _, _| {
      person_piece(path).map(|(owner, slug)| PiecePlace {
        owner: Some(owner),
        slug,
        scope: None,
      })
    },
  },
];

/// Every agent's folder, the chat agent's at the brain root among them even where the folders handed in
/// name none there.
pub struct Folders(Vec<AgentFolder>);

impl Folders {
  /// The folders as the agency's registration names them, each without a closing `/`, and the chat
  /// agent's key for the brain root.
  pub fn new(read: Vec<AgentFolder>, agent_key: &str) -> Self {
    let read: Vec<AgentFolder> = read
      .into_iter()
      .map(|one| AgentFolder {
        folder: one.folder.trim_end_matches('/').to_owned(),
        ..one
      })
      .collect();

    if read.iter().any(|one| one.folder.is_empty()) {
      Self(read)
    } else {
      Self(
        [AgentFolder {
          agent: agent_key.to_owned(),
          folder: String::new(),
        }]
        .into_iter()
        .chain(read)
        .collect(),
      )
    }
  }

  /// Whose memory a path relative to the brain root is, and the path relative to that agent's
  /// folder: the agent whose folder is the longest that holds it.
  pub fn agent_of<'a>(&'a self, path: &'a str) -> (&'a str, &'a str) {
    let mut folders: Vec<&AgentFolder> = self.0.iter().collect();

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
  pub fn library_owning(&self, path: &str) -> Option<&'static Library> {
    let (_, inner) = self.agent_of(path);

    LIBRARIES.iter().find(|library| (library.owns)(inner))
  }

  /// The row a piece of a library answers to.
  pub fn place(
    &self,
    library: &Library,
    path: &str,
    shared: Option<bool>,
    room: Option<i64>,
  ) -> Option<PiecePlace> {
    (library.place)(self.agent_of(path).1, shared, room)
  }
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
