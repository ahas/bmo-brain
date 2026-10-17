//! An editor process on a stage of the test's own: a scratch database, the other units in memory, the chat agent's
//! brain as a git repository at a folder of its own, a data root beside it, and the prompts the tests' writers are
//! handed. A session the agency starts can stand in for the model, writing files in its folder and rows on its desk.

use std::fs;
use std::path::Path;

use bmo_brain::editors::commits::git;
use bmo_brain::editors::ground::Ground;
use bmo_brain::ports::agency::{Outcome, Registration, SessionEnd, SessionStart};
use fragment_desk::desk::{Desk, FragmentWrite};

use super::AGENT;
use super::agency::{FakeAgency, Session, registration};
use super::gate::FakeGate;
use super::receiver::FakeReceiver;
use super::scratch::Scratch;
use super::sorting::prompts_root;

/// A git repository at a folder, with an identity to commit under.
pub fn repository(root: &Path) {
  for args in [
    &["init", "-q"][..],
    &["config", "user.email", "tester@example.com"],
    &["config", "user.name", "tester"],
  ] {
    git(root, args).unwrap();
  }
}

/// The subject of the brain's last commit.
pub fn last_commit(root: &Path) -> String {
  git(root, &["log", "-1", "--format=%s"]).unwrap_or_default()
}

pub struct Stage {
  pub scratch: Scratch,
  pub agency: FakeAgency,
  pub receiver: FakeReceiver,
  pub gate: FakeGate,
  pub brain: tempfile::TempDir,
  pub data: tempfile::TempDir,
  pub address: String,
  pub prompts: std::path::PathBuf,
}

impl Stage {
  /// A stage whose chat agent, `AGENT`, keeps its memory at the brain root.
  pub async fn new() -> Self {
    let scratch = Scratch::new().await;
    let agency = FakeAgency::default();
    let brain = tempfile::tempdir().unwrap();

    *agency.registrations.lock().unwrap() = vec![Registration {
      brain_folder: String::new(),
      label: Some("Helper".to_owned()),
      ..registration(AGENT)
    }];
    repository(brain.path());

    Self {
      address: scratch.address(),
      scratch,
      agency,
      receiver: FakeReceiver::default(),
      gate: FakeGate::default(),
      brain,
      data: tempfile::tempdir().unwrap(),
      prompts: prompts_root(),
    }
  }

  pub fn ground(&self) -> Ground<'_> {
    Ground {
      db: self.scratch.db(),
      database: &self.address,
      agency: &self.agency,
      receiver: &self.receiver,
      gate: &self.gate,
      brain_root: self.brain.path(),
      data_root: self.data.path(),
      prompts_root: &self.prompts,
      chat_key: AGENT,
      owner_names: &[],
      service_unit: "brain-service",
      unit: None,
      own_run: None,
    }
  }

  /// Every session from now on writes these files in its folder and these rows on its desk.
  pub fn sessions_write(&self, files: Vec<(String, String)>, writes: Vec<FragmentWrite>) {
    let session = move |start: &SessionStart| {
      let folder = Path::new(&start.workdir);

      for (path, text) in &files {
        let at = folder.join(path);

        fs::create_dir_all(at.parent().unwrap()).unwrap();
        fs::write(at, text).unwrap();
      }

      let desk = Path::new(&start.desk);
      let mut laid = Desk::read(desk).unwrap();

      laid.writes.extend(writes.iter().cloned());
      laid.save(desk).unwrap();
    };

    *self.agency.session.lock().unwrap() = Some(Session(Box::new(session)));
  }

  /// The next sessions end with these outcomes and answers, one each.
  pub fn sessions_end(&self, ends: &[(Outcome, Option<&str>)]) {
    let mut queued = self.agency.ends.lock().unwrap();

    for (outcome, answer) in ends {
      queued.push_back(SessionEnd {
        outcome: *outcome,
        answer: answer.map(str::to_owned),
        files_written: vec![],
        cost: None,
        size: None,
      });
    }
  }
}

/// A row write for a piece, as the fragment tool lays it on the desk.
pub fn row_write(path: &str, title: &str, keywords: &[&str]) -> FragmentWrite {
  FragmentWrite {
    path: path.to_owned(),
    removed: false,
    title: title.to_owned(),
    keywords: keywords.iter().map(|word| (*word).to_owned()).collect(),
    items: vec![],
    shared: None,
    all_agents: None,
  }
}
