//! The agency in memory: the registrations, caps, run clocks and turn ends a test sets, the turns it carries
//! numbered in order, which names are alive, the rest, and every call recorded.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use bmo_brain::ports::agency::{
  Agency, EditorTurn, Registration, Reload, Rest, Rulebook, RunClock, TurnEnd, TurnFault, TurnOutcome,
};
use bmo_brain::ports::http::PortFailure;
use futures_util::future::BoxFuture;
use serde_json::json;
use tokio::sync::oneshot;

use super::calls::Calls;

/// The cap a work key a test did not set answers with.
pub const CAP_SECONDS: i64 = 600;

#[derive(Debug, Default)]
pub struct FakeAgency {
  pub registrations: Mutex<Vec<Registration>>,
  /// The ends the turns come to, one per turn; a turn past them answers nothing.
  pub ends: Mutex<VecDeque<(TurnOutcome, Option<String>)>>,
  /// How long each turn runs before it ends; none ends at once.
  pub runs_for: Mutex<Option<Duration>>,
  /// How many turns from now are met by a session carrying another.
  pub busy: Mutex<usize>,
  /// Each run's or unit's clock; one not set has none standing.
  pub clocks: Mutex<BTreeMap<String, RunClock>>,
  /// Clocks answered first, one per read whatever it names, before `clocks` is read.
  pub clock_reads: Mutex<VecDeque<RunClock>>,
  /// Each work key's cap; one not set answers `CAP_SECONDS`.
  pub caps: Mutex<BTreeMap<String, i64>>,
  /// The names a run is alive under.
  pub alive: Mutex<BTreeSet<String>>,
  /// Whether nobody could say which names are alive.
  pub unsure: Mutex<bool>,
  pub rest: Mutex<Option<Rest>>,
  /// Whether a turn that fails leaves the deployment resting for a usage limit, as a login's limit does.
  pub limit_on_failure: Mutex<bool>,
  /// Whether the agency hands over no rulebook.
  pub no_rulebooks: Mutex<bool>,
  /// How many turns were carried, which numbers each turn's run.
  pub carried: Mutex<usize>,
  /// What each turn does as it starts, standing in for the model's file and fragment tools.
  pub session: Mutex<Option<Session>>,
  pub calls: Calls,
}

/// What a turn does with what it was given, in the folder and at the desk the turn names.
pub struct Session(pub Box<dyn Fn(&EditorTurn) + Send + Sync>);

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
    brain_folder: Some(name.to_owned()),
    awake: true,
    recollector_registration: None,
  }
}

/// A rest of the deployment for a usage limit.
pub fn limit_rest() -> Rest {
  Rest {
    resting: true,
    until: Some(4_102_444_800),
    kind: Some("limit".to_owned()),
  }
}

impl FakeAgency {
  pub fn with_agents(names: &[&str]) -> Self {
    let fake = Self::default();
    *fake.registrations.lock().unwrap() = names.iter().map(|name| registration(name)).collect();
    fake
  }

  /// The next turns end with these outcomes and answers, one each.
  pub fn turns_end(&self, ends: &[(TurnOutcome, Option<&str>)]) {
    self
      .ends
      .lock()
      .unwrap()
      .extend(ends.iter().map(|(outcome, answer)| (*outcome, answer.map(str::to_owned))));
  }

  /// The next turn fails on a usage limit: it ends failed, and the deployment rests for the limit from then on.
  pub fn limited(&self) {
    *self.limit_on_failure.lock().unwrap() = true;
    self.ends.lock().unwrap().push_back((TurnOutcome::Failed, None));
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

  fn editor_session<'a>(&'a self, work_key: &'a str) -> BoxFuture<'a, Result<String, PortFailure>> {
    self
      .calls
      .take("editor_session", json!({ "agent": work_key, "scope": { "kind": "editor" } }));
    let id = format!("{work_key}@editor");
    Box::pin(async move { Ok(id) })
  }

  fn editor_turn<'a>(
    &'a self,
    turn: &'a EditorTurn,
    started: oneshot::Sender<String>,
  ) -> BoxFuture<'a, Result<TurnEnd, TurnFault>> {
    self.calls.take("editor_turn", turn);
    {
      let mut busy = self.busy.lock().unwrap();

      if *busy > 0 {
        *busy -= 1;
        return Box::pin(async move { Err(TurnFault::Busy) });
      }
    }
    if let Some(session) = self.session.lock().unwrap().as_ref() {
      (session.0)(turn);
    }
    let run = {
      let mut carried = self.carried.lock().unwrap();

      *carried += 1;
      format!("run-{carried}")
    };
    let (outcome, answer) = self.ends.lock().unwrap().pop_front().unwrap_or((TurnOutcome::Answered, None));
    if outcome == TurnOutcome::Failed && *self.limit_on_failure.lock().unwrap() {
      *self.rest.lock().unwrap() = Some(limit_rest());
    }
    let runs_for = *self.runs_for.lock().unwrap();
    let _ = started.send(run.clone());
    Box::pin(async move {
      if let Some(runs_for) = runs_for {
        tokio::time::sleep(runs_for).await;
      }
      Ok(TurnEnd {
        run,
        outcome,
        answer,
        reason: None,
      })
    })
  }

  fn stop_run<'a>(&'a self, run: &'a str, reason: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("stop_run", json!({ "run": run, "reason": reason }));
    self.alive.lock().unwrap().remove(run);
    Box::pin(async move { Ok(true) })
  }

  fn run_clock<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<RunClock, PortFailure>> {
    self.calls.take("run_clock", json!({ "name": name }));
    let next = self.clock_reads.lock().unwrap().pop_front();
    let clock = next.unwrap_or_else(|| self.clocks.lock().unwrap().get(name).copied().unwrap_or_default());
    Box::pin(async move { Ok(clock) })
  }

  fn editor_cap<'a>(
    &'a self,
    work_key: &'a str,
    agent: &'a str,
  ) -> BoxFuture<'a, Result<i64, PortFailure>> {
    self.calls.take("editor_cap", json!({ "workKey": work_key, "agent": agent }));
    let cap = self.caps.lock().unwrap().get(work_key).copied().unwrap_or(CAP_SECONDS);
    Box::pin(async move { Ok(cap) })
  }

  fn alive<'a>(&'a self, names: &'a [String]) -> BoxFuture<'a, Result<Option<Vec<String>>, PortFailure>> {
    self.calls.take("alive", json!({ "names": names }));
    let alive = self.alive.lock().unwrap();
    let answer = (!*self.unsure.lock().unwrap())
      .then(|| names.iter().filter(|name| alive.contains(*name)).cloned().collect());
    Box::pin(async move { Ok(answer) })
  }

  fn rest(&self) -> BoxFuture<'_, Result<Rest, PortFailure>> {
    self.calls.take("rest", json!({}));
    let rest = self.rest.lock().unwrap().clone().unwrap_or(Rest {
      resting: false,
      until: None,
      kind: None,
    });
    Box::pin(async move { Ok(rest) })
  }

  fn rulebooks<'a>(&'a self, names: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Rulebook>, PortFailure>> {
    self.calls.take("rulebooks", json!({ "names": names }));
    let handed = !*self.no_rulebooks.lock().unwrap();
    let rulebooks = names
      .iter()
      .filter(|_| handed)
      .map(|name| Rulebook {
        name: (*name).to_owned(),
        text: format!("The {name} rulebook."),
      })
      .collect();
    Box::pin(async move { Ok(rulebooks) })
  }

  fn reload_documents<'a>(&'a self, reload: &'a Reload) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("reload_documents", reload);
    Box::pin(async move { Ok(true) })
  }
}
