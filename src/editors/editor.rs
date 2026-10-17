//! One run of an editor key at a fragment desk of its own (docs/boundary-api.md, 「The agency」; docs/fragments.md,
//! 「The desk is the run's」): the desk opened with what the run may reach, a turn of the key's editor session the
//! agency carries for the agent whose memory it writes, watched past its cap until it ends, and what it wrote on
//! the desk read back however it ended.
//!
//! The model the run goes out on, the logins, its usage and a usage limit's rest are the agency's. What the run
//! wrote in the agent's folder is read off the brain repository once it ends, under the editing lock its batch
//! holds, so it can be told from what another writer changed beside it (docs/people-book.md, 「Code decides where
//! the writer may write」).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fragment_desk::desk::FragmentWrite;

use super::commits::changed_in;
use super::runs::{self, Step, cut_at};
use super::settle::unknown_cause;
use crate::Failure;
use crate::ports::agency::{Agency, EditorTurn, TurnOutcome};

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
/// the unit it runs in and the service's unit beside it, its cap, which batch it is, its desk, the registration it
/// is named where its agent's recollectors go out on one, and the brain root its folder stands under.
pub struct EditorAsk<'a> {
  pub key: &'a str,
  pub agent: &'a str,
  pub chat_id: i64,
  pub prompt: &'a str,
  pub unit: Option<&'a str>,
  /// The unit the service runs as, which names a run whose batch runs in no unit of its own.
  pub service_unit: &'a str,
  pub cap_seconds: i64,
  pub step: Option<Step>,
  pub desk: &'a dyn RunDesk,
  pub registration: Option<&'a str>,
  pub brain_root: &'a Path,
}

/// What the run came to: its answer, the run that gave it, the row writes it made on its desk and the files it
/// changed in its folder; or why it failed, as a person reads the cause.
#[derive(Debug, PartialEq)]
pub enum Answer {
  Answered {
    answer: String,
    run: String,
    writes: Vec<FragmentWrite>,
    wrote: BTreeSet<PathBuf>,
  },
  Failed(String),
}

/// A usage limit the run met: the agency rests on it, and the run stops without a retry.
#[derive(Debug, PartialEq, Eq)]
pub struct Limit {
  pub run: String,
}

/// How long an editor run may take for the agent it writes for: its key's cap as the agency gives it, which is
/// never less than the wait one request of the agent's recollector registration is given.
pub async fn editor_cap_seconds(agency: &dyn Agency, key: &str, agent: &str) -> Result<i64, Failure> {
  Ok(agency.editor_cap(key, agent).await?)
}

/// The run at its desk, as a turn of its key's editor session.
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
      answer, run, wrote, ..
    } => Answer::Answered {
      answer,
      run,
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

fn unreadable(run: &str) -> String {
  format!(
    "The model run ended without a readable answer. {}",
    unknown_cause(&format!("the agency's record of run {run}"))
  )
}

fn now_ms() -> u128 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis())
}

/// The name the brain gives a run: the unit its batch runs in, or the service's, then its work key, its agent, its
/// room and the moment it starts, so no two runs share one.
pub fn run_name(ask: &EditorAsk<'_>) -> String {
  format!(
    "{}:{}:{}:{}:{}",
    ask.unit.unwrap_or(ask.service_unit),
    ask.key,
    ask.agent,
    ask.chat_id,
    now_ms()
  )
}

// What the run changed in its folder, by absolute path: every file git reports changed there since the last commit,
// the other agents' folders under it set aside.
async fn changed(
  agency: &dyn Agency,
  ask: &EditorAsk<'_>,
  workdir: &str,
) -> Result<BTreeSet<PathBuf>, String> {
  let beside: Vec<String> = agency
    .registrations(None)
    .await
    .map_err(|why| why.to_string())?
    .into_iter()
    .filter(|registration| registration.name != ask.agent)
    .filter_map(|registration| registration.brain_folder)
    .map(|folder| folder.trim_end_matches('/').to_owned())
    .filter(|folder| !folder.is_empty() && folder.as_str() != workdir)
    .filter(|folder| workdir.is_empty() || folder.starts_with(&format!("{workdir}/")))
    .collect();

  Ok(
    changed_in(ask.brain_root, workdir, &beside)?
      .into_iter()
      .map(|path| ask.brain_root.join(path))
      .collect(),
  )
}

// The turn carried at the desk, watched past its cap, and what it ended with read.
async fn carry(agency: &dyn Agency, ask: &EditorAsk<'_>, desk: &Path, cwd: &Path) -> Result<Answer, Limit> {
  let Ok(workdir) = cwd.strip_prefix(ask.brain_root) else {
    return Ok(Answer::Failed(format!(
      "The run's folder, {}, does not stand under the brain.",
      cwd.display()
    )));
  };
  let workdir = workdir.to_string_lossy().into_owned();
  let asked = EditorTurn {
    session: String::new(),
    prompt: ask.prompt.to_owned(),
    run: run_name(ask),
    parent: ask.unit.map(str::to_owned),
    agent: ask.agent.to_owned(),
    chat_id: ask.chat_id,
    step: ask.step.map(|step| step.step),
    steps: ask.step.map(|step| step.steps),
    workdir: workdir.clone(),
    desk: desk.to_string_lossy().into_owned(),
    registration: ask.registration.map(str::to_owned),
  };
  let turned = match runs::turn(agency, ask.key, asked, ask.cap_seconds).await {
    Ok(turned) => turned,
    Err(why) => return Ok(Answer::Failed(format!("The agency did not carry the run to its end: {why}."))),
  };

  if turned.cut {
    return Ok(Answer::Failed(cut_at(ask.cap_seconds)));
  }

  let end = turned.end;
  let run = end.run;

  match (end.outcome, end.answer.filter(|answer| !answer.trim().is_empty())) {
    (TurnOutcome::Answered, Some(answer)) => match changed(agency, ask, &workdir).await {
      Ok(wrote) => Ok(Answer::Answered {
        answer,
        run,
        writes: vec![],
        wrote,
      }),
      Err(why) => {
        Ok(Answer::Failed(format!("What the run wrote could not be read from the brain: {why}.")))
      }
    },
    (TurnOutcome::Stopped, _) => {
      Ok(Answer::Failed("the run was stopped from outside while it was working".to_owned()))
    }
    (TurnOutcome::Failed, _) if agency.rest().await.is_ok_and(|rest| rest.for_limit()) => {
      Err(Limit { run })
    }
    (outcome, _) => {
      eprintln!(
        "{}: run {run} left no readable answer ({outcome:?}, {})",
        ask.key,
        end.reason.unwrap_or_default()
      );

      Ok(Answer::Failed(unreadable(&run)))
    }
  }
}
