//! The sort's surroundings as a test stands them in: a folder of its own, a writer that does what it is told, a cut
//! that does what it is told, the sheet judgement writing what it is told, and one failure store shared by every
//! run of a test, since what is tested is what one run leaves the next. Two rooms and two people stand in the
//! books the tests lay down.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bmo_brain::Failure;
use bmo_brain::editors::book::KnownIdentifiers;
use bmo_brain::editors::documents::library_documents;
use bmo_brain::editors::people::sort::{FailureCounts, SortDesk, SortRoom};
use bmo_brain::editors::people::{Stopped, Wrote};
use bmo_brain::editors::prompts::Prompts;
use bmo_brain::policy::COMMON_WORDS;
use bmo_brain::ports::receiver::OwnerNotice;
use fragment_desk::desk::{FragmentFilter, FragmentWrite, Names};

use super::AGENT;
use super::agency::FakeAgency;

/// The person whose book is sorted, and another member of the rooms.
pub const PERSON: i64 = 10_000_001;
pub const BOSS: i64 = 200_000_002;
/// The two rooms the person's book stands in.
pub const ROOM_A: i64 = -1002;
pub const ROOM_B: i64 = -1001;

/// The prompts the tests' writers are handed.
pub fn prompts_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prompts")
}

pub fn write(root: &Path, path: &str, text: &str) {
  let at = root.join(path);

  fs::create_dir_all(at.parent().unwrap()).unwrap();
  fs::write(at, text).unwrap();
}

pub fn read(root: &Path, path: &str) -> String {
  fs::read_to_string(root.join(path)).unwrap()
}

pub fn room_dir(chat_id: i64) -> String {
  format!("rooms/{chat_id}/people")
}

pub fn common_dir() -> String {
  format!("people/{PERSON}")
}

/// Every piece file under a chunks directory, with what it holds, relative to the root.
pub fn piece_files(root: &Path) -> HashMap<String, String> {
  library_documents(root)
    .into_iter()
    .filter(|path| path.contains("/chunks/"))
    .map(|path| {
      let text = read(root, &path);

      (path, text)
    })
    .collect()
}

/// The row writes a faithful run makes through the fragment tool: one for every piece it created or changed, and a
/// removal for every piece it deleted.
pub fn fragment_writes_for(
  before: &HashMap<String, String>,
  after: &HashMap<String, String>,
) -> Vec<FragmentWrite> {
  let mut writes: Vec<FragmentWrite> = after
    .iter()
    .filter(|(path, text)| before.get(*path) != Some(*text))
    .map(|(path, _)| FragmentWrite {
      path: path.clone(),
      removed: false,
      title: "A sorted piece of the person's book, needed when a turn comes to what it holds".to_owned(),
      keywords: vec![],
      items: vec![],
      shared: None,
      all_agents: None,
    })
    .collect();

  writes.extend(
    before
      .keys()
      .filter(|path| !after.contains_key(*path))
      .map(|path| fragment_desk::meta::removal_write(path)),
  );
  writes
}

/// The failures a sort remembers between its runs, as the brain's settings would keep them, and what the owner
/// was told.
#[derive(Clone, Default)]
pub struct Store {
  pub counts: Rc<RefCell<HashMap<String, i64>>>,
  pub notices: Rc<RefCell<Vec<OwnerNotice>>>,
}

impl FailureCounts for Store {
  async fn count(&mut self, key: &str) -> Result<i64, Failure> {
    Ok(self.counts.borrow().get(key).copied().unwrap_or(0))
  }
}

/// What a writer, a cut or the sheet judgement does to the folder, for the room it is asked about.
pub type Editor = fn(&Path, i64);

#[derive(Default)]
pub struct Recorded {
  pub prompts: Vec<(i64, String)>,
  pub desks: Vec<(i64, Vec<String>, Vec<FragmentFilter>)>,
  pub sheets: Vec<i64>,
  pub finished: Vec<Vec<String>>,
  pub messages: Vec<String>,
  pub removed: Vec<String>,
  pub cuts: usize,
  pub events: Vec<String>,
}

pub struct Room {
  root: PathBuf,
  editor: Editor,
  cut: Option<Editor>,
  sheet: Option<Editor>,
  store: Store,
  agency: FakeAgency,
  prompts: PathBuf,
  pub recorded: Recorded,
  pub unkept: BTreeSet<i64>,
}

pub fn room(
  root: &Path,
  editor: Editor,
  cut: Option<Editor>,
  store: &Store,
  sheet: Option<Editor>,
) -> Room {
  Room {
    root: root.to_path_buf(),
    editor,
    cut,
    sheet,
    store: store.clone(),
    agency: FakeAgency::default(),
    prompts: prompts_root(),
    recorded: Recorded::default(),
    unkept: BTreeSet::new(),
  }
}

impl FailureCounts for Room {
  async fn count(&mut self, key: &str) -> Result<i64, Failure> {
    self.store.count(key).await
  }
}

impl SortRoom for Room {
  type Held = ();

  fn root(&self) -> &Path {
    &self.root
  }

  fn agent(&self) -> &str {
    AGENT
  }

  fn unkept(&self) -> &BTreeSet<i64> {
    &self.unkept
  }

  fn prompts(&self) -> Prompts<'_> {
    Prompts {
      agency: &self.agency,
      root: &self.prompts,
    }
  }

  fn label(&self) -> &str {
    "Tester Bot"
  }

  fn journal(&self) -> String {
    "the journal of its unit".to_owned()
  }

  async fn names(&mut self) -> Result<Names, Failure> {
    Ok(Names {
      agent: "Tester Bot".to_owned(),
      agent_key: AGENT.to_owned(),
      common_words: COMMON_WORDS.words.clone(),
    })
  }

  async fn known(&mut self) -> Result<KnownIdentifiers, Failure> {
    Ok(KnownIdentifiers {
      user_ids: vec![PERSON, BOSS],
      chat_ids: vec![ROOM_A, ROOM_B],
      titles: vec!["Alpha Room".to_owned()],
    })
  }

  async fn title(&mut self, _chat_id: i64) -> Result<Option<String>, Failure> {
    Ok(None)
  }

  async fn display_name(&mut self, _user_id: i64) -> Result<Option<String>, Failure> {
    Ok(Some("Tester".to_owned()))
  }

  async fn resting(&mut self) -> Result<bool, Failure> {
    Ok(false)
  }

  async fn lock(&mut self) -> Result<Option<()>, Failure> {
    Ok(Some(()))
  }

  async fn run_editor(&mut self, chat_id: i64, prompt: &str, desk: SortDesk<'_>) -> Result<Wrote, Stopped> {
    self.recorded.prompts.push((chat_id, prompt.to_owned()));
    self.recorded.desks.push((desk.room, desk.places.clone(), desk.reach.clone()));
    self.recorded.events.push(format!("run {chat_id}"));

    let before = piece_files(&self.root);

    (self.editor)(&self.root, chat_id);

    Ok(Wrote::Stood(fragment_writes_for(&before, &piece_files(&self.root))))
  }

  async fn cut_stretch(&mut self, _chat_id: i64, _user_id: i64) -> Result<bool, Stopped> {
    self.recorded.cuts += 1;

    if let Some(cut) = self.cut {
      cut(&self.root, 0);
    }

    Ok(false)
  }

  async fn write_sheets(&mut self, chat_id: i64, _user_id: i64) -> Result<(), Failure> {
    self.recorded.sheets.push(chat_id);
    self.recorded.events.push(format!("sheets {chat_id}"));

    if let Some(sheet) = self.sheet {
      sheet(&self.root, chat_id);
    }

    Ok(())
  }

  async fn finish(
    &mut self,
    _chat_id: i64,
    _user_id: i64,
    written: &[String],
    message: &str,
    writes: &[FragmentWrite],
  ) -> Result<(), Failure> {
    self.recorded.finished.push(written.to_vec());
    self.recorded.messages.push(message.to_owned());
    self
      .recorded
      .removed
      .extend(writes.iter().filter(|write| write.removed).map(|write| write.path.clone()));
    Ok(())
  }

  async fn item_states(&mut self, ids: &[i64]) -> Result<HashMap<i64, bool>, Failure> {
    Ok(ids.iter().map(|id| (*id, *id % 2 == 0)).collect())
  }

  async fn record(&mut self, key: &str, count: i64) -> Result<(), Failure> {
    self.store.counts.borrow_mut().insert(key.to_owned(), count);
    Ok(())
  }

  async fn clear(&mut self, key: &str) -> Result<(), Failure> {
    self.store.counts.borrow_mut().remove(key);
    Ok(())
  }

  async fn notify(&mut self, notice: OwnerNotice) {
    self.store.notices.borrow_mut().push(notice);
  }
}
