//! What the fragment tool does on a desk: the rows a run may reach with this batch's writes laid over
//! them, and the writes, checked as the table will hold them, kept on the desk until the batch stands.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::desk::{Desk, DeskError, Desked, FragmentFilter, FragmentWrite, Names};
use crate::libraries::{Folders, Library, in_folder, out_of_folder};
use crate::meta::{
  LineRow, body_facts, body_hash, fragment_write, item_bracket_problem, keyword_problem, removal_write,
  title_shape_problem, words_carried, write_path,
};
use crate::rows::{AgentFolder, Rows};

/// A desk opened for one call: the desk, the rows read from the database it names, and the agents'
/// folders.
pub struct Open {
  pub desk: Desk,
  names: Names,
  rows: Rows,
  folders: Folders,
}

/// One row as the run sees it: the table's row with this batch's writes laid over it.
#[derive(Clone, Debug)]
pub struct DeskRow {
  pub path: String,
  pub kind: String,
  pub owner: Option<i64>,
  pub slug: String,
  pub scope: Option<String>,
  pub title: String,
  pub written_on: Option<String>,
  pub confirmed_on: Option<String>,
  pub keywords: Vec<String>,
  pub items: Vec<i64>,
  pub status: String,
  pub active: bool,
  pub written: bool,
  pub removed: bool,
}

/// What a search of the run's rows asks for; every part left out matches everything.
#[derive(Default)]
pub struct Query {
  pub text: Option<String>,
  pub item: Option<i64>,
  pub person: Option<i64>,
  pub status: Option<String>,
  pub down: bool,
}

/// What a row write asks for.
pub struct WriteRequest {
  pub path: String,
  pub title: String,
  pub keywords: Option<Vec<String>>,
  pub items: Option<Vec<i64>>,
  pub shared: Option<bool>,
  pub all_agents: Option<bool>,
  pub distinct_from: Vec<String>,
}

fn refused<T>(code: &'static str, why: impl Into<String>) -> Desked<T> {
  Err(DeskError::refused(code, why))
}

/// A path with its empty and `.` steps dropped and each `..` folding the step before it, for the one
/// question asked of it: whether it climbs out.
pub fn normalized(path: &str) -> String {
  let mut parts: Vec<&str> = Vec::new();

  for part in path.split('/') {
    match part {
      "" | "." => {}
      ".." if parts.last().is_some_and(|last| *last != "..") => {
        parts.pop();
      }
      other => parts.push(other),
    }
  }

  if parts.is_empty() { ".".to_owned() } else { parts.join("/") }
}

fn in_places(places: &[String], path: &str) -> bool {
  places
    .iter()
    .any(|place| path.starts_with(&format!("{}/", place.trim_end_matches('/'))))
}

fn line_of(one: &DeskRow) -> String {
  let mark = if one.removed {
    " [removed in this run]"
  } else if one.written {
    " [written in this run]"
  } else if one.active {
    ""
  } else {
    " [down]"
  };
  let line = crate::meta::writer_line(&LineRow {
    slug: &one.slug,
    scope: one.scope.as_deref(),
    title: &one.title,
    written_on: one.written_on.as_deref(),
    confirmed_on: one.confirmed_on.as_deref(),
    path: &one.path,
    keywords: &one.keywords,
    items: &one.items,
    status: &one.status,
  });

  format!("{line}{mark}")
}

fn unique<T: Clone + Eq + std::hash::Hash>(values: impl IntoIterator<Item = T>) -> Vec<T> {
  let mut seen = HashSet::new();

  values.into_iter().filter(|value| seen.insert(value.clone())).collect()
}

impl Open {
  /// The desk at this path, read afresh, with the rows of the database it names, among the agents'
  /// folders the caller read from the agency's registration (every awake agent that has one) and the libraries
  /// the caller hands in, asked in that order; with none, the desk takes no piece.
  pub async fn at(
    path: &Path,
    folders: Vec<AgentFolder>,
    libraries: Vec<&'static dyn Library>,
  ) -> Desked<Self> {
    let desk = Desk::read(path)?;
    let names = desk.names()?.clone();
    let rows = Rows::open(Path::new(&desk.db)).await?;
    let folders = Folders::new(folders, &names.agent_key).holding(libraries);

    Ok(Self {
      desk,
      names,
      rows,
      folders,
    })
  }

  // Where a path the run names will stand once its batch is kept.
  fn seen_path(&self, path: &str) -> String {
    match &self.desk.beside {
      Some(beside) => match path.strip_prefix(&format!("{}/", beside.from)) {
        Some(rest) => format!("{}/{rest}", beside.to),
        None => path.to_owned(),
      },
      None => path.to_owned(),
    }
  }

  // A path the run names, as the brain root and the table name it.
  fn brain_side(&self, path: &str) -> String {
    in_folder(&self.desk.folder, &self.seen_path(path))
  }

  // A row's path as the run is shown it: relative to the folder it stands in.
  fn run_side(&self, path: &str) -> String {
    out_of_folder(&self.desk.folder, path)
  }

  // The rows the run may reach: only its own agent's, where it names one.
  fn reach(&self) -> Vec<FragmentFilter> {
    match &self.desk.agent {
      None => self.desk.reach.clone(),
      Some(agent) => self
        .desk
        .reach
        .iter()
        .map(|filter| FragmentFilter {
          agent: Some(agent.clone()),
          ..filter.clone()
        })
        .collect(),
    }
  }

  // The last write of this batch naming each path, the run's own over the ones carried in, each
  // standing where its path was first written.
  fn latest_writes(&self) -> Vec<FragmentWrite> {
    let mut order: Vec<FragmentWrite> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();

    for write in self.desk.carried.iter().chain(&self.desk.writes) {
      match at.get(&write.path) {
        Some(&index) => order[index] = write.clone(),
        None => {
          at.insert(write.path.clone(), order.len());
          order.push(write.clone());
        }
      }
    }

    order
  }

  fn body_path(&self, path: &str) -> PathBuf {
    Path::new(&self.desk.brain).join(path)
  }

  fn body_at(&self, path: &str) -> Option<String> {
    fs::read_to_string(self.body_path(path)).ok()
  }

  /// The rows the run may reach, with what this batch wrote laid over them.
  pub fn rows(&self) -> Desked<Vec<DeskRow>> {
    let mut table: Vec<DeskRow> = Vec::new();
    let mut by_id: HashMap<i64, usize> = HashMap::new();

    for filter in self.reach() {
      for row in self.rows.fragments(&filter)? {
        let one = DeskRow {
          path: self.run_side(&row.path),
          kind: row.kind,
          owner: row.owner,
          slug: row.slug,
          scope: row.scope,
          title: row.title,
          written_on: row.written_on,
          confirmed_on: row.confirmed_on,
          keywords: row.keywords,
          items: row.items,
          status: row.status,
          active: row.active,
          written: false,
          removed: false,
        };

        match by_id.get(&row.id) {
          Some(&index) => table[index] = one,
          None => {
            by_id.insert(row.id, table.len());
            table.push(one);
          }
        }
      }
    }

    let writes = self.latest_writes();
    let named: HashSet<String> = table.iter().map(|one| one.path.clone()).collect();

    for one in &mut table {
      if let Some(write) = writes.iter().find(|write| write.path == one.path) {
        if !write.removed {
          one.title = write.title.clone();
          one.keywords = write.keywords.clone();
          one.items = write.items.clone();
        }

        one.active = !write.removed;
        one.written = true;
        one.removed = write.removed;
      }
    }

    for write in &writes {
      if named.contains(&self.seen_path(&write.path)) || named.contains(&write.path) {
        continue;
      }

      let brain = self.brain_side(&write.path);
      let Some(library) = self.folders.library_owning(&brain) else {
        continue;
      };
      let Some(place) = self.folders.place(library, &brain, write.shared, self.desk.room) else {
        continue;
      };
      let facts = self.body_at(&write.path).map(|body| body_facts(&body));

      table.push(DeskRow {
        path: write.path.clone(),
        kind: library.kind().to_owned(),
        owner: place.owner,
        slug: place.slug,
        scope: place.scope,
        title: write.title.clone(),
        written_on: facts.as_ref().and_then(|facts| facts.written_on.clone()),
        confirmed_on: facts.as_ref().and_then(|facts| facts.confirmed_on.clone()),
        keywords: write.keywords.clone(),
        items: unique(
          write.items.iter().copied().chain(facts.map(|facts| facts.items).unwrap_or_default()),
        ),
        status: "open".to_owned(),
        active: !write.removed,
        written: true,
        removed: write.removed,
      });
    }

    Ok(table)
  }

  /// The rows the run may reach that the query matches, each as the line a writer is shown.
  pub fn search(&self, query: &Query) -> Desked<Vec<String>> {
    let lower = query.text.clone().unwrap_or_default().to_lowercase();
    let asked: Vec<&str> = lower
      .split(|character: char| character.is_whitespace() || character == ',')
      .map(str::trim)
      .filter(|word| word.encode_utf16().count() >= 2)
      .collect();

    Ok(
      self
        .rows()?
        .iter()
        .filter(|one| query.down || one.active)
        .filter(|one| query.item.is_none_or(|item| one.items.contains(&item)))
        .filter(|one| query.person.is_none_or(|person| one.owner == Some(person)))
        .filter(|one| query.status.as_ref().is_none_or(|status| &one.status == status))
        .filter(|one| {
          if asked.is_empty() {
            return true;
          }

          let said = [&one.slug, &one.path, &one.title]
            .into_iter()
            .chain(&one.keywords)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

          asked.iter().any(|word| said.contains(word))
        })
        .map(line_of)
        .collect(),
    )
  }

  // The written library a path is a piece of, where the run may write it, or a refusal saying which.
  fn writable_library(&self, path: &str) -> Desked<&'static dyn Library> {
    if path.is_empty() || path.starts_with('/') || normalized(path).starts_with("..") {
      return refused(
        "out-of-place",
        format!("{path} is not a path relative to the folder this run works in."),
      );
    }

    let brain = self.brain_side(path);
    let Some(library) = self.folders.library_owning(&brain) else {
      return refused("out-of-place", self.folders.no_piece(path));
    };

    if !in_places(&self.desk.places, path) {
      return refused(
        "out-of-place",
        format!("{path} is outside where this run may write: {}.", self.desk.places.join(", ")),
      );
    }

    let (owner, _) = self.folders.agent_of(&brain);

    if let Some(agent) = &self.desk.agent
      && owner != agent
    {
      return refused(
        "out-of-place",
        format!("{path} is in {owner}'s memory, and this run writes {agent}'s alone."),
      );
    }

    Ok(library)
  }

  // Whether the table or this batch already holds a row standing at a path.
  fn stands_at(&self, library: &dyn Library, path: &str) -> Desked<bool> {
    if let Some(write) = self.latest_writes().into_iter().find(|write| write.path == path) {
      return Ok(!write.removed);
    }

    let brain = self.brain_side(path);
    let filter = FragmentFilter {
      kind: Some(library.kind().to_owned()),
      active: Some(true),
      ..FragmentFilter::default()
    };

    Ok(self.rows.fragments(&filter)?.iter().any(|row| row.path == brain))
  }

  // The rows of the same library that may already hold the matter of a new piece.
  fn overlapping(
    &self,
    library: &dyn Library,
    write: &FragmentWrite,
    spared: &HashSet<String>,
  ) -> Desked<Vec<DeskRow>> {
    Ok(
      self
        .rows()?
        .into_iter()
        .filter(|one| {
          if !one.active
            || one.kind != library.kind()
            || one.path == write.path
            || spared.contains(&one.path)
          {
            return false;
          }

          let said = [one.slug.replace('-', " "), one.title.clone()]
            .into_iter()
            .chain(one.keywords.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");

          words_carried(&write.keywords, &said) >= 2
            || one.items.iter().any(|item| write.items.contains(item))
        })
        .collect(),
    )
  }

  // A write checked as the table will hold it, or a refusal; `held` holds a new piece against the
  // rows that may already hold its matter.
  fn checked_write(&self, request: &WriteRequest, held: bool) -> Desked<FragmentWrite> {
    let write = fragment_write(
      &request.path,
      &request.title,
      request.keywords.as_deref().unwrap_or_default(),
      request.items.as_deref().unwrap_or_default(),
      request.shared,
      request.all_agents,
    );
    let library = self.writable_library(&write.path)?;

    if !self.body_path(&write.path).exists() {
      return refused(
        "no-body",
        format!(
          "{} is not on disk. Write the piece's body with your file tools first, then its row.",
          write.path
        ),
      );
    }

    if let Some(brackets) = item_bracket_problem(&self.body_at(&write.path).unwrap_or_default()) {
      return refused(
        "unnamed-slots",
        format!(
          "{} {brackets}. Put the slot names into its body with your file tools, then write its row again.",
          write.path
        ),
      );
    }

    if let Some(shape) = title_shape_problem(&write.title, &self.names) {
      return refused(
        "title-shape",
        format!(
          "The title of {} {shape}. Say what kind of document the piece is and the situations a turn needs it for, in one or two sentences.",
          write.path,
        ),
      );
    }

    if let Some(keywords) = keyword_problem(&write, &self.names) {
      return refused("keywords", format!("{} {keywords}.", write.path));
    }

    if held && !self.stands_at(library, &write.path)? {
      let spared: HashSet<String> = request.distinct_from.iter().map(|path| write_path(path)).collect();
      let already = self.overlapping(library, &write, &spared)?;

      if !already.is_empty() {
        let mut said = vec![format!(
          "{} would be a new piece, and these pieces share its items or its keywords, so one of them may already hold its matter:",
          write.path,
        )];

        said.extend(already.iter().map(line_of));
        said.push("Put the matter into the piece that holds it and write that piece's row, fold the two with fragment_merge, or — where this really is another matter — write again naming each of them in distinct_from.".to_owned());

        return refused("repeats", said.join("\n"));
      }
    }

    Ok(write)
  }

  // The row a write will make, as the run is shown it back.
  fn written_line(&self, path: &str) -> Desked<String> {
    let seen = self.seen_path(path);

    Ok(
      self
        .rows()?
        .iter()
        .find(|row| row.path == path || row.path == seen)
        .map_or_else(|| path.to_owned(), line_of),
    )
  }

  /// A piece's row written on the desk; the row as it will stand.
  pub fn write(&mut self, request: &WriteRequest) -> Desked<String> {
    let write = self.checked_write(request, true)?;
    let path = write.path.clone();

    self.desk.writes.push(write);
    self.written_line(&path)
  }

  // A piece's file deleted and its removal written. The caller has checked it may.
  fn take_away(&mut self, path: &str) {
    let _ = fs::remove_file(self.body_path(path));

    self.desk.writes.push(removal_write(path));
  }

  // A piece the run may take away: one of its own places, with a file, a row or a write behind it.
  fn removable(&self, path: &str) -> Desked<String> {
    let named = write_path(path);
    let library = self.writable_library(&named)?;

    if !self.body_path(&named).exists() && !self.stands_at(library, &named)? {
      return refused("nothing-stands", format!("Nothing stands at {named}: no file and no row."));
    }

    Ok(named)
  }

  /// A piece taken away: its file deleted, its row marked to go down.
  pub fn remove(&mut self, path: &str) -> Desked<String> {
    let named = self.removable(path)?;

    self.take_away(&named);

    Ok(format!("{named} removed; its row goes down when the batch stands."))
  }

  /// Pieces folded into one: the kept piece's row written, every other piece removed.
  pub fn merge(
    &mut self,
    into: &str,
    from: &[String],
    title: &str,
    keywords: Option<Vec<String>>,
    items: Option<Vec<i64>>,
    shared: Option<bool>,
  ) -> Desked<String> {
    let into = write_path(into);
    let from: Vec<String> = unique(from.iter().map(|path| write_path(path)))
      .into_iter()
      .filter(|path| *path != into)
      .collect();

    if from.is_empty() {
      return refused("bad-merge", "A merge names at least one piece to fold into the kept one.");
    }

    let folded: Vec<DeskRow> = self
      .rows()?
      .into_iter()
      .filter(|one| one.path == into || from.contains(&one.path))
      .collect();
    let write = self.checked_write(
      &WriteRequest {
        path: into.clone(),
        title: title.to_owned(),
        keywords: Some(
          keywords.unwrap_or_else(|| folded.iter().flat_map(|one| one.keywords.clone()).collect()),
        ),
        items: Some(items.unwrap_or_else(|| folded.iter().flat_map(|one| one.items.clone()).collect())),
        shared,
        all_agents: None,
        distinct_from: Vec::new(),
      },
      false,
    )?;

    for path in &from {
      self.removable(path)?;
    }

    for path in &from {
      self.take_away(path);
    }

    self.desk.writes.push(write);

    Ok([format!("{} folded into {into}:", from.join(", ")), self.written_line(&into)?].join("\n"))
  }

  /// One piece cut into several: a row for each, and the original removed unless it is one of them.
  pub fn split(&mut self, from: &str, pieces: &[WriteRequest]) -> Desked<String> {
    let from = write_path(from);
    let paths: Vec<String> = pieces.iter().map(|piece| write_path(&piece.path)).collect();

    if paths.len() < 2 && !(paths.len() == 1 && paths[0] != from) {
      return refused(
        "bad-split",
        "A split names the pieces that come out of it: two or more, or one that is not the piece cut.",
      );
    }

    let writes = pieces
      .iter()
      .map(|piece| self.checked_write(piece, false))
      .collect::<Desked<Vec<_>>>()?;
    let kept = paths.contains(&from);

    if !kept {
      self.removable(&from)?;
      self.take_away(&from);
    }

    self.desk.writes.extend(writes);

    let mut said = vec![format!(
      "{from} cut into {} pieces{}:",
      paths.len(),
      if kept { "" } else { ", and removed" }
    )];

    for path in &paths {
      said.push(self.written_line(path)?);
    }

    Ok(said.join("\n"))
  }

  /// The pieces under the run's places it has created or changed on disk and written no row for yet.
  pub fn owed(&self) -> Vec<String> {
    let written: HashSet<String> = self
      .latest_writes()
      .into_iter()
      .filter(|write| !write.removed)
      .map(|write| write.path)
      .collect();
    let mut owed = Vec::new();

    for place in &self.desk.places {
      let place = place.trim_end_matches('/');
      let Ok(names) = walk(&self.body_path(place)) else {
        continue;
      };

      for name in names.into_iter().filter(|name| name.ends_with(".md")) {
        let path = format!("{place}/{name}");

        if written.contains(&path) || self.folders.library_owning(&self.brain_side(&path)).is_none() {
          continue;
        }

        if let Some(body) = self.body_at(&path)
          && self.desk.before.get(&path) != Some(&body_hash(&body))
        {
          owed.push(path);
        }
      }
    }

    owed.sort();
    owed
  }
}

// Every entry under a directory, files and directories alike, by its path relative to that directory.
fn walk(dir: &Path) -> std::io::Result<Vec<String>> {
  let mut found = Vec::new();
  let mut waiting = vec![(dir.to_owned(), String::new())];

  while let Some((at, prefix)) = waiting.pop() {
    for entry in fs::read_dir(&at)? {
      let entry = entry?;
      let name = format!("{prefix}{}", entry.file_name().to_string_lossy());

      if entry.file_type()?.is_dir() {
        waiting.push((entry.path(), format!("{name}/")));
      }

      found.push(name);
    }
  }

  Ok(found)
}
