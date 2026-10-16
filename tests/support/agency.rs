//! The agency in memory: the registrations, work keys, item states and session ends a test sets, the runs
//! it starts numbered in order, and every call recorded.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

use bmo_brain::ports::agency::{
  Agency, ItemState, Launch, LaunchOutcome, Outcome, Overrun, QuietInstruction, Registration, Reload,
  SessionEnd, SessionStart, WorkKey,
};
use bmo_brain::ports::http::PortFailure;
use futures_util::future::BoxFuture;
use serde_json::json;

use super::calls::Calls;

/// The cap a work key a test did not set answers with.
pub const CAP_SECONDS: i64 = 600;

#[derive(Debug, Default)]
pub struct FakeAgency {
  pub registrations: Mutex<Vec<Registration>>,
  /// The ends the sessions come to, one per wait; a wait past them answers a session that answered nothing.
  pub ends: Mutex<VecDeque<SessionEnd>>,
  pub overrun: Mutex<Option<Overrun>>,
  /// The runs that still stand; every run the fake started stands until it is stopped.
  pub open: Mutex<BTreeSet<String>>,
  pub work_keys: Mutex<BTreeMap<String, WorkKey>>,
  pub finished: Mutex<BTreeSet<i64>>,
  pub launch: Mutex<Option<Launch>>,
  pub calls: Calls,
}

/// A registration of `name` whose brain folder is its own name, awake and with nothing else set.
pub fn registration(name: &str) -> Registration {
  Registration {
    name: name.to_owned(),
    label: None,
    key: name.to_owned(),
    brain_folder: name.to_owned(),
    awake: true,
    recollector_registration: None,
    reasoning: None,
  }
}

impl FakeAgency {
  pub fn with_agents(names: &[&str]) -> Self {
    let fake = Self::default();
    *fake.registrations.lock().unwrap() = names.iter().map(|name| registration(name)).collect();
    fake
  }
}

impl Agency for FakeAgency {
  fn registrations<'a>(
    &'a self,
    agent: Option<&'a str>,
  ) -> BoxFuture<'a, Result<Vec<Registration>, PortFailure>> {
    self.calls.take("registrations", json!({ "agent": agent }));
    let all = self.registrations.lock().unwrap().clone();
    let named = all.into_iter().filter(|known| agent.is_none_or(|agent| known.name == agent)).collect();
    Box::pin(async move { Ok(named) })
  }

  fn start_session<'a>(&'a self, start: &'a SessionStart) -> BoxFuture<'a, Result<String, PortFailure>> {
    self.calls.take("start_session", start);
    let run = format!("run-{}", self.calls.of("start_session").len());
    self.open.lock().unwrap().insert(run.clone());
    Box::pin(async move { Ok(run) })
  }

  fn wait_session<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<SessionEnd, PortFailure>> {
    self.calls.take("wait_session", json!({ "run": run }));
    let end = self.ends.lock().unwrap().pop_front().unwrap_or(SessionEnd {
      outcome: Outcome::Answered,
      answer: None,
      files_written: Vec::new(),
      cost: None,
      size: None,
    });
    Box::pin(async move { Ok(end) })
  }

  fn stop_session<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("stop_session", json!({ "run": run }));
    self.open.lock().unwrap().remove(run);
    Box::pin(async move { Ok(true) })
  }

  fn ask_overrun<'a>(
    &'a self,
    run: &'a str,
    unit: &'a str,
    cap_seconds: i64,
  ) -> BoxFuture<'a, Result<Overrun, PortFailure>> {
    self
      .calls
      .take("ask_overrun", json!({ "run": run, "unit": unit, "capSeconds": cap_seconds }));
    let overrun = self.overrun.lock().unwrap().unwrap_or(Overrun {
      keep: false,
      until: None,
    });
    Box::pin(async move { Ok(overrun) })
  }

  fn run_open<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("run_open", json!({ "run": run }));
    let open = self.open.lock().unwrap().contains(run);
    Box::pin(async move { Ok(open) })
  }

  fn work_key<'a>(
    &'a self,
    work_key: &'a str,
    agent: &'a str,
  ) -> BoxFuture<'a, Result<WorkKey, PortFailure>> {
    self.calls.take("work_key", json!({ "workKey": work_key, "agent": agent }));
    let known = self.work_keys.lock().unwrap().get(work_key).cloned();
    let key = known.unwrap_or(WorkKey {
      cap_seconds: CAP_SECONDS,
      reasoning: None,
    });
    Box::pin(async move { Ok(key) })
  }

  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>> {
    self.calls.take("item_states", json!({ "items": items }));
    let finished = self.finished.lock().unwrap();
    let states = items
      .iter()
      .map(|&item| ItemState {
        item,
        finished: finished.contains(&item),
      })
      .collect();
    Box::pin(async move { Ok(states) })
  }

  fn launch_unit<'a>(
    &'a self,
    unit: &'a str,
    command: &'a [String],
  ) -> BoxFuture<'a, Result<Launch, PortFailure>> {
    self.calls.take("launch_unit", json!({ "unit": unit, "command": command }));
    let launch = self.launch.lock().unwrap().clone().unwrap_or(Launch {
      outcome: LaunchOutcome::Launched,
      why: None,
    });
    Box::pin(async move { Ok(launch) })
  }

  fn quiet_instruction<'a>(
    &'a self,
    instruction: &'a QuietInstruction,
  ) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("quiet_instruction", instruction);
    Box::pin(async move { Ok(true) })
  }

  fn reload_documents<'a>(&'a self, reload: &'a Reload) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("reload_documents", reload);
    Box::pin(async move { Ok(true) })
  }
}
