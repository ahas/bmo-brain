//! The agency port: the registrations, the recollector sessions and the work keys they run on, a job's run
//! and items, the units launched and the notices handed to an agent (`docs/boundary-api.md`, 「The agency」).

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

  /// A work key's cap and reasoning for an agent.
  fn work_key<'a>(
    &'a self,
    work_key: &'a str,
    agent: &'a str,
  ) -> BoxFuture<'a, Result<WorkKey, PortFailure>>;

  /// Whether each item is finished.
  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>>;

  /// Launches a unit running a command.
  fn launch_unit<'a>(
    &'a self,
    unit: &'a str,
    command: &'a [String],
  ) -> BoxFuture<'a, Result<Launch, PortFailure>>;

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
struct LaunchAsk<'a> {
  unit: &'a str,
  command: &'a [String],
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

  fn launch_unit<'a>(
    &'a self,
    unit: &'a str,
    command: &'a [String],
  ) -> BoxFuture<'a, Result<Launch, PortFailure>> {
    Box::pin(async move {
      let ask = LaunchAsk { unit, command };
      self.caller.post("/units/launch", &ask).await
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
