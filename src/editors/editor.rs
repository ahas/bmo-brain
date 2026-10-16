//! One run of an editor key at a fragment desk of its own (docs/boundary-api.md, 「The agency」; docs/fragments.md,
//! 「The desk is the session's」): the desk opened with what the run may reach, a recollector session of the key
//! the agency starts standing in the folder of the agent whose memory it writes, its clock kept until the
//! session ends, and what it wrote on the desk read back however it ended.
//!
//! The model the session runs on, the logins it goes out on, its usage and a usage limit's rest are the
//! agency's. What the run's own file tools wrote comes back with the session's end, so it can be told from
//! what another writer changed beside it (docs/people-book.md, 「Code decides where the writer may write」).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fragment_desk::desk::FragmentWrite;

use super::runs::{self, Step};
use super::settle::unknown_cause;
use crate::Failure;
use crate::ports::agency::{Agency, Outcome, SessionStart};

/// The tools an editor session is shown: the file tools and the brain's fragment tool, and no shell, web or
/// subagents.
pub const TOOLS: [&str; 6] = ["read", "write", "edit", "glob", "grep", "fragment"];

/// The fragment desk a run is laid at: opened with what the run may reach, the folder of the brain it has the
/// run stand in, and what the run wrote on it read back as it is taken away.
pub trait RunDesk: Send + Sync {
  /// The desk opened, as its path.
  fn open(&self) -> std::io::Result<PathBuf>;
  /// The folder of the brain an open desk has its run stand in.
  fn brain(&self, desk: &Path) -> Option<PathBuf>;
  /// What the run wrote on its desk, and the desk taken away. A desk that is not there wrote nothing.
  fn close(&self, desk: &Path) -> Vec<FragmentWrite>;
}

/// What one editor run is asked: its key, the agent whose memory it writes, the room of its batch, its prompt,
/// the unit it runs in, its cap, which batch it is, its desk, and the registration it is named where its
/// agent's recollectors go out on one.
pub struct EditorAsk<'a> {
  pub key: &'a str,
  pub agent: &'a str,
  pub chat_id: i64,
  pub prompt: &'a str,
  pub unit: Option<&'a str>,
  pub cap_seconds: i64,
  pub step: Option<Step>,
  pub desk: &'a dyn RunDesk,
  pub registration: Option<&'a str>,
}

/// What the run came to: its answer, the run that gave it, what it cost and the size it ended carrying, the row
/// writes it made on its desk and the files its own file tools wrote; or why it failed, as a person reads the
/// cause.
#[derive(Debug, PartialEq)]
pub enum Answer {
  Answered {
    answer: String,
    run: String,
    cost: Option<f64>,
    size: Option<i64>,
    writes: Vec<FragmentWrite>,
    wrote: BTreeSet<PathBuf>,
  },
  Failed(String),
}

/// A usage limit the session reached: the agency has rested its login, and the run stops without a retry.
#[derive(Debug, PartialEq, Eq)]
pub struct Limit {
  pub run: String,
}

/// How long an editor run may take for the agent it writes for: its key's cap as the agency gives it, which is
/// never less than the wait one request of the agent's recollector registration is given.
pub async fn editor_cap_seconds(agency: &dyn Agency, key: &str, agent: &str) -> Result<i64, Failure> {
  Ok(agency.work_key(key, agent).await?.cap_seconds)
}

/// The run at its desk, as a session of its key the agency starts.
pub async fn answer_at_desk(agency: &dyn Agency, ask: EditorAsk<'_>) -> Result<Answer, Limit> {
  let desk = match ask.desk.open() {
    Ok(desk) => desk,
    Err(why) => {
      return Ok(Answer::Failed(format!("The run's fragment desk could not be opened: {why}.")));
    }
  };
  let answered = match lay(ask.desk, &desk) {
    Ok(cwd) => carry(agency, &ask, &desk, &cwd).await,
    Err(why) => Ok(Answer::Failed(why)),
  };
  let writes = ask.desk.close(&desk);

  answered.map(|answer| match answer {
    Answer::Answered {
      answer,
      run,
      cost,
      size,
      wrote,
      ..
    } => Answer::Answered {
      answer,
      run,
      cost,
      size,
      writes,
      wrote,
    },
    failed => failed,
  })
}

// The folder the run stands in, read off its desk and made where it is missing, since the run that writes it has
// file tools and no shell.
fn lay(desks: &dyn RunDesk, desk: &Path) -> Result<PathBuf, String> {
  let cwd = desks
    .brain(desk)
    .ok_or_else(|| "The run's fragment desk could not be read back.".to_owned())?;

  std::fs::create_dir_all(&cwd).map_err(|why| format!("{}: {why}", cwd.display()))?;

  Ok(cwd)
}

fn cut_at(cap_seconds: i64) -> String {
  format!(
    "The model run went past its {}-minute limit and was stopped.",
    (cap_seconds as f64 / 60.0).round()
  )
}

fn unreadable(run: &str) -> String {
  format!(
    "The model run ended without a readable answer. {}",
    unknown_cause(&format!("the agency's record of run {run}"))
  )
}

// The session started at the desk, waited out under its clock, and what it ended with read.
async fn carry(agency: &dyn Agency, ask: &EditorAsk<'_>, desk: &Path, cwd: &Path) -> Result<Answer, Limit> {
  let start = SessionStart {
    work_key: ask.key.to_owned(),
    agent: ask.agent.to_owned(),
    chat_id: ask.chat_id,
    unit: ask.unit.map(str::to_owned),
    cap_seconds: ask.cap_seconds,
    step: ask.step.map(|step| step.step),
    steps: ask.step.map(|step| step.steps),
    prompt: ask.prompt.to_owned(),
    tools: TOOLS.map(str::to_owned).to_vec(),
    workdir: cwd.to_string_lossy().into_owned(),
    desk: desk.to_string_lossy().into_owned(),
    registration: ask.registration.map(str::to_owned),
  };
  let run = match agency.start_session(&start).await {
    Ok(run) => run,
    Err(why) => return Ok(Answer::Failed(format!("The agency could not start the run: {why}."))),
  };
  let waited = match runs::wait(agency, &run, ask.unit, ask.cap_seconds).await {
    Ok(waited) => waited,
    Err(why) => {
      return Ok(Answer::Failed(format!("The agency did not follow run {run} to its end: {why}.")));
    }
  };

  if waited.cut {
    return Ok(Answer::Failed(cut_at(ask.cap_seconds)));
  }

  let end = waited.end;

  match (end.outcome, end.answer) {
    (Outcome::Answered, Some(answer)) => Ok(Answer::Answered {
      answer,
      run,
      cost: end.cost,
      size: end.size,
      writes: vec![],
      wrote: end.files_written.iter().map(|path| cwd.join(path)).collect(),
    }),
    (Outcome::Limit, _) => Err(Limit { run }),
    (Outcome::Stopped, _) => {
      Ok(Answer::Failed("the run was stopped from outside while it was working".to_owned()))
    }
    (Outcome::Overran, _) => Ok(Answer::Failed(cut_at(ask.cap_seconds))),
    (Outcome::Answered | Outcome::Failed, _) => {
      eprintln!("{}: run {run} left no readable answer", ask.key);

      Ok(Answer::Failed(unreadable(&run)))
    }
  }
}
