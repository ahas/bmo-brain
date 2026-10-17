//! The agency port: the registrations, the recollector sessions and the work keys they run on, a job's run, its
//! clock keep and items, the rulebooks every document writer shares, the units launched and the notices handed to
//! an agent (`docs/boundary-api.md`, 「The agency」).

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::ports::http::{Caller, Done, Peer, PortFailure};

/// One agent's registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
  pub name: String,
  pub label: Option<String>,
  pub key: String,
  pub brain_folder: String,
  pub awake: bool,
  pub recollector_registration: Option<String>,
  pub reasoning: Option<String>,
}

/// A recollector session to start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStart {
  pub work_key: String,
  pub agent: String,
  pub chat_id: i64,
  /// The unit the batch runs in, or none where it runs in none.
  pub unit: Option<String>,
  pub cap_seconds: i64,
  /// Which batch it is, and of how many, or none for a run that is no batch of a planned run.
  pub step: Option<i64>,
  pub steps: Option<i64>,
  pub prompt: String,
  pub tools: Vec<String>,
  pub workdir: String,
  pub desk: String,
  pub registration: Option<String>,
}

/// How a recollector session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
  Answered,
  Failed,
  Overran,
  Stopped,
  /// The session reached a usage limit; the agency has rested its login.
  Limit,
}

/// A recollector session's end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEnd {
  pub outcome: Outcome,
  pub answer: Option<String>,
  pub files_written: Vec<String>,
  pub cost: Option<f64>,
  pub size: Option<i64>,
}

/// An overrun ask's answer: whether the run is kept going, and until when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overrun {
  pub keep: bool,
  pub until: Option<i64>,
}

/// A work key's cap and reasoning, for one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkKey {
  pub cap_seconds: i64,
  pub reasoning: Option<String>,
}

/// One item's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemState {
  pub item: i64,
  pub finished: bool,
}

/// What came of a unit launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchOutcome {
  Launched,
  Running,
  Refused,
}

/// A run to start apart in a unit of its own: the unit, the work it does, what the live picture calls it, the agent
/// it works for where it works for one, and its command's words in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitLaunch {
  pub unit: String,
  pub kind: String,
  pub title: String,
  pub agent: Option<String>,
  pub command: Vec<String>,
}

/// A unit launch's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
  pub outcome: LaunchOutcome,
  pub why: Option<String>,
}

/// A quiet instruction handed to an agent in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietInstruction {
  pub agent: String,
  pub chat_id: i64,
  pub text: String,
  pub kind: Option<String>,
}

/// The documents an agent should read again, and the rooms they bear on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reload {
  pub agent: String,
  pub paths: Vec<String>,
  pub rooms: Vec<i64>,
}

/// A rulebook the agency keeps for every document writer, by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rulebook {
  pub name: String,
  pub text: String,
}

/// What the brain asks of the agency.
pub trait Agency: Send + Sync {
  /// The named agent's registration, or every agent's where none is named.
  fn registrations<'a>(
    &'a self,
    agent: Option<&'a str>,
  ) -> BoxFuture<'a, Result<Vec<Registration>, PortFailure>>;

  /// Starts a recollector session and answers its run.
  fn start_session<'a>(&'a self, start: &'a SessionStart) -> BoxFuture<'a, Result<String, PortFailure>>;

  /// Waits for a run's session to end.
  fn wait_session<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<SessionEnd, PortFailure>>;

  /// Stops a run's session.
  fn stop_session<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>>;

  /// Asks whether a run past its cap is kept going.
  fn ask_overrun<'a>(
    &'a self,
    run: &'a str,
    unit: &'a str,
    cap_seconds: i64,
  ) -> BoxFuture<'a, Result<Overrun, PortFailure>>;

  /// Whether a job's run still stands.
  fn run_open<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>>;

  /// The moment the open overrun ask on a run or unit stands until, or none where none stands open.
  fn kept_until<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<Option<i64>, PortFailure>>;

  /// The moment the open session started under a unit runs out of its cap, the latest where several stand, or
  /// none where none stands.
  fn cap_ends<'a>(&'a self, unit: &'a str) -> BoxFuture<'a, Result<Option<i64>, PortFailure>>;

  /// The named rulebooks, in the order asked.
  fn rulebooks<'a>(&'a self, names: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Rulebook>, PortFailure>>;

  /// A work key's cap and reasoning for an agent.
  fn work_key<'a>(
    &'a self,
    work_key: &'a str,
    agent: &'a str,
  ) -> BoxFuture<'a, Result<WorkKey, PortFailure>>;

  /// Whether each item is finished.
  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>>;

  /// Launches a unit running a command.
  fn launch_unit<'a>(&'a self, launch: &'a UnitLaunch) -> BoxFuture<'a, Result<Launch, PortFailure>>;

  /// Those of the units a run stands under.
  fn units_running<'a>(&'a self, units: &'a [String]) -> BoxFuture<'a, Result<Vec<String>, PortFailure>>;

  /// Hands an agent a quiet instruction for a room.
  fn quiet_instruction<'a>(
    &'a self,
    instruction: &'a QuietInstruction,
  ) -> BoxFuture<'a, Result<bool, PortFailure>>;

  /// Tells an agent that documents changed.
  fn reload_documents<'a>(&'a self, reload: &'a Reload) -> BoxFuture<'a, Result<bool, PortFailure>>;
}

/// The agency over HTTP.
#[derive(Debug, Clone)]
pub struct HttpAgency {
  caller: Caller,
}

impl HttpAgency {
  pub fn new(peer: &Peer) -> Self {
    Self::with_caller(Caller::new(peer))
  }

  pub fn with_caller(caller: Caller) -> Self {
    Self { caller }
  }
}

#[derive(Serialize)]
struct AgentAsk<'a> {
  agent: Option<&'a str>,
}

#[derive(Deserialize)]
struct Agents {
  agents: Vec<Registration>,
}

#[derive(Serialize)]
struct RunAsk<'a> {
  run: &'a str,
}

#[derive(Serialize)]
struct UnitAsk<'a> {
  unit: &'a str,
}

#[derive(Deserialize)]
struct Started {
  run: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OverrunAsk<'a> {
  run: &'a str,
  unit: &'a str,
  cap_seconds: i64,
}

#[derive(Deserialize)]
struct Open {
  open: bool,
}

#[derive(Deserialize)]
struct Kept {
  until: Option<i64>,
}

#[derive(Serialize)]
struct RulebooksAsk<'a> {
  names: &'a [&'a str],
}

#[derive(Deserialize)]
struct Rulebooks {
  rulebooks: Vec<Rulebook>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkKeyAsk<'a> {
  work_key: &'a str,
  agent: &'a str,
}

#[derive(Serialize)]
struct ItemsAsk<'a> {
  items: &'a [i64],
}

#[derive(Deserialize)]
struct Items {
  items: Vec<ItemState>,
}

#[derive(Serialize)]
struct UnitsAsk<'a> {
  units: &'a [String],
}

#[derive(Deserialize)]
struct Running {
  running: Vec<String>,
}

impl Agency for HttpAgency {
  fn registrations<'a>(
    &'a self,
    agent: Option<&'a str>,
  ) -> BoxFuture<'a, Result<Vec<Registration>, PortFailure>> {
    Box::pin(async move {
      let answer: Agents = self.caller.post("/registrations/lookup", &AgentAsk { agent }).await?;
      Ok(answer.agents)
    })
  }

  fn start_session<'a>(&'a self, start: &'a SessionStart) -> BoxFuture<'a, Result<String, PortFailure>> {
    Box::pin(async move {
      let answer: Started = self.caller.post("/runs/start", start).await?;
      Ok(answer.run)
    })
  }

  fn wait_session<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<SessionEnd, PortFailure>> {
    Box::pin(async move { self.caller.post("/runs/wait", &RunAsk { run }).await })
  }

  fn stop_session<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Done = self.caller.post("/runs/stop", &RunAsk { run }).await?;
      Ok(answer.ok)
    })
  }

  fn ask_overrun<'a>(
    &'a self,
    run: &'a str,
    unit: &'a str,
    cap_seconds: i64,
  ) -> BoxFuture<'a, Result<Overrun, PortFailure>> {
    Box::pin(async move {
      let ask = OverrunAsk {
        run,
        unit,
        cap_seconds,
      };
      self.caller.post("/runs/overrun", &ask).await
    })
  }

  fn run_open<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Open = self.caller.post("/runs/open", &RunAsk { run }).await?;
      Ok(answer.open)
    })
  }

  fn kept_until<'a>(&'a self, run: &'a str) -> BoxFuture<'a, Result<Option<i64>, PortFailure>> {
    Box::pin(async move {
      let answer: Kept = self.caller.post("/runs/kept-until", &RunAsk { run }).await?;
      Ok(answer.until)
    })
  }

  fn cap_ends<'a>(&'a self, unit: &'a str) -> BoxFuture<'a, Result<Option<i64>, PortFailure>> {
    Box::pin(async move {
      let answer: Kept = self.caller.post("/runs/cap-ends", &UnitAsk { unit }).await?;
      Ok(answer.until)
    })
  }

  fn rulebooks<'a>(&'a self, names: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Rulebook>, PortFailure>> {
    Box::pin(async move {
      let answer: Rulebooks = self.caller.post("/rulebooks/read", &RulebooksAsk { names }).await?;
      Ok(answer.rulebooks)
    })
  }

  fn work_key<'a>(
    &'a self,
    work_key: &'a str,
    agent: &'a str,
  ) -> BoxFuture<'a, Result<WorkKey, PortFailure>> {
    Box::pin(async move {
      let ask = WorkKeyAsk { work_key, agent };
      self.caller.post("/work-keys/read", &ask).await
    })
  }

  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>> {
    Box::pin(async move {
      let answer: Items = self.caller.post("/items/states", &ItemsAsk { items }).await?;
      Ok(answer.items)
    })
  }

  fn launch_unit<'a>(&'a self, launch: &'a UnitLaunch) -> BoxFuture<'a, Result<Launch, PortFailure>> {
    Box::pin(async move { self.caller.post("/units/launch", launch).await })
  }

  fn units_running<'a>(&'a self, units: &'a [String]) -> BoxFuture<'a, Result<Vec<String>, PortFailure>> {
    Box::pin(async move {
      let answer: Running = self.caller.post("/units/running", &UnitsAsk { units }).await?;
      Ok(answer.running)
    })
  }

  fn quiet_instruction<'a>(
    &'a self,
    instruction: &'a QuietInstruction,
  ) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Done = self.caller.post("/instructions/quiet", instruction).await?;
      Ok(answer.ok)
    })
  }

  fn reload_documents<'a>(&'a self, reload: &'a Reload) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Done = self.caller.post("/documents/reload", reload).await?;
      Ok(answer.ok)
    })
  }
}
