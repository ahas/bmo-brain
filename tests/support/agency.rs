//! The agency in memory: the registrations, work keys, item states and session ends a test sets, the runs
//! it starts numbered in order, and every call recorded.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use bmo_brain::ports::agency::{
  Agency, ItemState, Launch, LaunchOutcome, Outcome, Overrun, QuietInstruction, Registration, Reload,
  Rulebook, SessionEnd, SessionStart, UnitLaunch, WorkKey,
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
  /// How long each session runs before its wait answers; none answers at once.
  pub runs_for: Mutex<Option<Duration>>,
  /// The overrun ask's answers, one per ask; an ask past them answers with `overrun`.
  pub overruns: Mutex<VecDeque<Overrun>>,
  pub overrun: Mutex<Option<Overrun>>,
  /// The runs that still stand; every run the fake started stands until it is stopped.
  pub open: Mutex<BTreeSet<String>>,
  /// The moment each run or unit is kept until; one not set is kept by no ask.
  pub kept: Mutex<BTreeMap<String, i64>>,
  /// The moment the open session under each unit runs out of its cap; a unit not set has none open.
  pub cap_ends: Mutex<BTreeMap<String, i64>>,
  pub work_keys: Mutex<BTreeMap<String, WorkKey>>,
  pub finished: Mutex<BTreeSet<i64>>,
  /// What every unit launch answers, where a test set it; otherwise a unit standing answers `running`, and any
  /// other is launched and stands from then on.
  pub launch: Mutex<Option<Launch>>,
  /// The units a run stands under.
  pub running: Mutex<BTreeSet<String>>,
  /// What each session does as it starts, standing in for the model's file and fragment tools.
  pub session: Mutex<Option<Session>>,
  pub calls: Calls,
}

/// What a session does with the start it was given, in the folder and at the desk the start names.
pub struct Session(pub Box<dyn Fn(&SessionStart) + Send + Sync>);

impl std::fmt::Debug for Session {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str("Session")
  }
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
    if let Some(session) = self.session.lock().unwrap().as_ref() {
      (session.0)(start);
    }
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
    let runs_for = *self.runs_for.lock().unwrap();
    Box::pin(async move {
      if let Some(runs_for) = runs_for {
        tokio::time::sleep(runs_for).await;
      }
      Ok(end)
    })
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
    let next = self.overruns.lock().unwrap().pop_front();
    let overrun = next.or(*self.overrun.lock().unwrap()).unwrap_or(Overrun {
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

  fn kept_until<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<Option<i64>, PortFailure>> {
    self.calls.take("kept_until", json!({ "run": run }));
    let until = self.kept.lock().unwrap().get(run).copied();
    Box::pin(async move { Ok(until) })
  }

  fn cap_ends<'a>(&'a self, unit: &'a str) -> BoxFuture<'a, Result<Option<i64>, PortFailure>> {
    self.calls.take("cap_ends", json!({ "unit": unit }));
    let until = self.cap_ends.lock().unwrap().get(unit).copied();
    Box::pin(async move { Ok(until) })
  }

  fn rulebooks<'a>(&'a self, names: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Rulebook>, PortFailure>> {
    self.calls.take("rulebooks", json!({ "names": names }));
    let rulebooks = names
      .iter()
      .map(|name| Rulebook {
        name: (*name).to_owned(),
        text: format!("The {name} rulebook."),
      })
      .collect();
    Box::pin(async move { Ok(rulebooks) })
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

  fn launch_unit<'a>(&'a self, launch: &'a UnitLaunch) -> BoxFuture<'a, Result<Launch, PortFailure>> {
    self.calls.take("launch_unit", launch);
    let set = self.launch.lock().unwrap().clone();
    let mut running = self.running.lock().unwrap();
    let answer = set.unwrap_or_else(|| {
      let outcome = if running.insert(launch.unit.clone()) {
        LaunchOutcome::Launched
      } else {
        LaunchOutcome::Running
      };
      Launch { outcome, why: None }
    });
    Box::pin(async move { Ok(answer) })
  }

  fn units_running<'a>(&'a self, units: &'a [String]) -> BoxFuture<'a, Result<Vec<String>, PortFailure>> {
    self.calls.take("units_running", json!({ "units": units }));
    let running = self.running.lock().unwrap();
    let standing = units.iter().filter(|unit| running.contains(*unit)).cloned().collect();
    Box::pin(async move { Ok(standing) })
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
