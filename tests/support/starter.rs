//! The user's service manager in memory, as the brain starts its runs apart: every start recorded, a unit started
//! standing from then on so a second start of it answers `running`, and an answer a test sets for every start.

use std::collections::BTreeSet;
use std::sync::Mutex;

use bmo_brain::schedules::apart::{Launch, LaunchOutcome, Starter, UnitLaunch};
use futures_util::future::BoxFuture;
use serde_json::json;

use super::calls::Calls;

#[derive(Debug, Default)]
pub struct FakeStarter {
  /// What every start answers, where a test set it.
  pub answer: Mutex<Option<Launch>>,
  /// The units standing.
  pub standing: Mutex<BTreeSet<String>>,
  pub calls: Calls,
}

impl FakeStarter {
  /// The units started, in order.
  pub fn units(&self) -> Vec<String> {
    self
      .calls
      .of("start")
      .iter()
      .map(|start| start["unit"].as_str().unwrap().to_owned())
      .collect()
  }
}

impl Starter for FakeStarter {
  fn start<'a>(&'a self, launch: &'a UnitLaunch) -> BoxFuture<'a, Launch> {
    self.calls.take(
      "start",
      json!({ "unit": launch.unit, "title": launch.title, "command": launch.command }),
    );
    let set = self.answer.lock().unwrap().clone();
    let answer = set.unwrap_or_else(|| {
      let outcome = if self.standing.lock().unwrap().insert(launch.unit.clone()) {
        LaunchOutcome::Launched
      } else {
        LaunchOutcome::Running
      };
      Launch { outcome, why: None }
    });
    Box::pin(async move { answer })
  }
}
