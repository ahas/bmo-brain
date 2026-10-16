//! The desk an editor run writes its rows on (docs/fragments.md, 「An editor writes the row through the
//! fragment tool」), as the batch opens and closes it: what the run may reach and where it may write, the
//! database the rows are read from, the folder it stands in, each piece under its places as the run finds it,
//! and the names the tool's checks read. The fragment tool reads and writes the same file for every call the run
//! makes; the batch reads back what the run wrote once it ends, and takes the desk away however it ended.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fragment_desk::desk::{Beside, Desk, FragmentFilter, FragmentWrite, Names};
use fragment_desk::meta::body_hash;
use serde_json::Map;

use super::DocumentBackup;
use super::editor::RunDesk;

/// What one editor run may reach through the fragment tool, as the batch that runs it says.
#[derive(Clone, Debug, Default)]
pub struct DeskScope {
  /// The room of the batch, which a library whose scope the writer chooses reads for a piece whose write
  /// does not choose.
  pub room: Option<i64>,
  /// The agent whose memory the run writes: the run stands in that agent's folder and reaches its rows
  /// alone. Left out, the run stands at the brain root and the places alone bound it.
  pub agent: Option<String>,
  /// The agent's folder relative to the brain root, empty for the root itself.
  pub folder: String,
  /// The directories the run may write, relative to the folder it stands in.
  pub places: Vec<String>,
  /// The rows the run may read, as the table is asked for them.
  pub reach: Vec<FragmentFilter>,
  /// For a run that writes beside a book rather than into it: the directory it writes and the one its
  /// pieces will stand in once the batch takes the book's place.
  pub beside: Option<Beside>,
  /// What earlier runs of the same batch wrote, which the table does not hold yet.
  pub carried: Vec<FragmentWrite>,
}

/// Where the desks of the runs in flight stand.
pub fn desk_dir(data_root: &Path) -> PathBuf {
  data_root.join("fragment-desks")
}

/// Whether a path stands under one of the places.
pub fn in_places(places: &[String], path: &str) -> bool {
  places
    .iter()
    .any(|place| path.starts_with(&format!("{}/", place.trim_end_matches('/'))))
}

/// A desk for one run, written where the tool will read it. `brain` is the brain root, `db` the database's
/// place, `before` the documents of the folder the run stands in as the run found them, by path relative to that
/// folder, of which only the pieces under the run's places are kept, as their bodies' digests, and `names` the
/// names the tool's checks read.
pub fn open_desk(
  scope: &DeskScope,
  dir: &Path,
  db: &str,
  brain: &Path,
  before: &DocumentBackup,
  names: &Names,
) -> std::io::Result<PathBuf> {
  let path = dir.join(format!("{:032x}.json", rand::random::<u128>()));
  let digests = before
    .iter()
    .filter(|(piece, _)| in_places(&scope.places, piece))
    .map(|(piece, bytes)| (piece.clone(), body_hash(&String::from_utf8_lossy(bytes))))
    .collect::<BTreeMap<_, _>>();
  let stands = if scope.folder.is_empty() { brain.to_path_buf() } else { brain.join(&scope.folder) };
  let desk = Desk {
    room: scope.room,
    agent: scope.agent.clone(),
    folder: scope.folder.clone(),
    places: scope.places.clone(),
    reach: scope.reach.clone(),
    beside: scope.beside.clone(),
    carried: scope.carried.clone(),
    db: db.to_owned(),
    brain: stands.to_string_lossy().into_owned(),
    before: digests,
    writes: vec![],
    names: Some(names.clone()),
    rest: Map::new(),
  };

  desk.save(&path).map_err(|why| std::io::Error::other(format!("{why:?}")))?;

  Ok(path)
}

/// The folder of the brain a desk has its run stand in.
pub fn desk_brain(path: &Path) -> Option<PathBuf> {
  Desk::read(path).ok().map(|desk| PathBuf::from(desk.brain))
}

/// What the run wrote on its desk, and the desk taken away. A desk that is not there wrote nothing.
pub fn close_desk(path: &Path) -> Vec<FragmentWrite> {
  let writes = Desk::read(path).map(|desk| desk.writes).unwrap_or_default();

  let _ = std::fs::remove_file(path);

  writes
}

/// A desk a session runs at, opened afresh for each run: the scope it is opened with and everything the tool's
/// checks read beside it.
pub struct ScopedDesk<'a> {
  pub scope: DeskScope,
  pub dir: PathBuf,
  pub db: &'a str,
  pub brain: &'a Path,
  pub before: &'a DocumentBackup,
  pub names: &'a Names,
}

impl RunDesk for ScopedDesk<'_> {
  fn open(&self) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(&self.dir)?;
    open_desk(&self.scope, &self.dir, self.db, self.brain, self.before, self.names)
  }

  fn brain(&self, desk: &Path) -> Option<PathBuf> {
    desk_brain(desk)
  }

  fn close(&self, desk: &Path) -> Vec<FragmentWrite> {
    close_desk(desk)
  }
}
