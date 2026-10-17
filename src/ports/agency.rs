//! The agency port: the registrations, the editor sessions the recollector runs go out as and the event stream
//! their turns end on, a run's stop, its clock and the cap a run is given, which names are alive, the rest, the
//! rulebooks every document writer shares and the documents that moved (`docs/boundary-api.md`, 「The agency」).
//!
//! Where the agency serves no route yet for what the brain needs of it, the client answers the nearest behaviour
//! that table states without making a call, so the brain stands with what is served now.

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use sse_stream::SseStream;
use tokio::sync::oneshot;

use crate::ports::http::{Caller, Peer, PortFailure};

/// The cap an editor run is given while the agency serves no editor-cap read: the bound its `docs/agent-control.md`
/// gives an editor run, thirty minutes.
pub const EDITOR_CAP_SECONDS: i64 = 30 * 60;

/// How long after its turn was answered the brain still waits for the turn's end on the event stream.
pub const END_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// One agent as the agency lists it. Its name is its registration key; an agent listed with no brain folder keeps
/// no long-term memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
  pub name: String,
  #[serde(default)]
  pub label: Option<String>,
  #[serde(default)]
  pub brain_folder: Option<String>,
  /// An agent listed without saying is taken as awake: the agency lists the agents it stands.
  #[serde(default = "listed_awake")]
  pub awake: bool,
  #[serde(default)]
  pub recollector_registration: Option<String>,
}

fn listed_awake() -> bool {
  true
}

/// One turn of an editor work key's session: the session and the prompt, and the run's parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorTurn {
  pub session: String,
  pub prompt: String,
  /// The name the brain gives the run.
  pub run: String,
  /// The unit the batch runs in, or none.
  pub parent: Option<String>,
  /// The agent whose memory the run writes.
  pub agent: String,
  pub chat_id: i64,
  /// Which batch it is, and of how many, or none for a run that is no batch of a planned run.
  pub step: Option<i64>,
  pub steps: Option<i64>,
  /// The agent's brain folder, relative to the brain root: empty for the root.
  pub workdir: String,
  pub desk: String,
  /// The registration the agent's recollectors run on, or none for the key's own road.
  pub registration: Option<String>,
}

/// How a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnOutcome {
  Answered,
  /// The program waits on a person, so the turn gave no answer.
  Blocked,
  Stopped,
  Failed,
}

/// A turn's end, as the agency publishes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnEnd {
  /// The run the turn went out as.
  pub run: String,
  pub outcome: TurnOutcome,
  #[serde(default)]
  pub answer: Option<String>,
  #[serde(default)]
  pub reason: Option<String>,
}

/// Why a turn came to no end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnFault {
  /// The session carries a turn already, so this one was not taken.
  Busy,
  /// The call did not come to the turn's end.
  Failed(PortFailure),
}

impl From<PortFailure> for TurnFault {
  fn from(failure: PortFailure) -> Self {
    Self::Failed(failure)
  }
}

/// A run's clock, by its name or the name it runs under: when its bound runs out, and until when a keep or an open
/// ask on it stands. Each is none where none stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunClock {
  pub cap_ends: Option<i64>,
  pub kept_until: Option<i64>,
}

/// Whether the deployment rests, until when, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rest {
  pub resting: bool,
  #[serde(default)]
  pub until: Option<i64>,
  /// `limit` or `outage`.
  #[serde(default)]
  pub kind: Option<String>,
}

impl Rest {
  /// Whether the rest stands for a usage limit.
  pub fn for_limit(&self) -> bool {
    self.resting && self.kind.as_deref() == Some("limit")
  }
}

/// The documents an agent's sessions should read again, and the rooms they bear on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reload {
  pub agent: String,
  /// Relative to the brain root.
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

  /// The editor session of a work key, as its id.
  fn editor_session<'a>(&'a self, work_key: &'a str) -> BoxFuture<'a, Result<String, PortFailure>>;

  /// One turn carried to its end: the run it goes out as handed to `started` once the turn has started, and its
  /// end answered.
  fn editor_turn<'a>(
    &'a self,
    turn: &'a EditorTurn,
    started: oneshot::Sender<String>,
  ) -> BoxFuture<'a, Result<TurnEnd, TurnFault>>;

  /// A run stopped, with why.
  fn stop_run<'a>(&'a self, run: &'a str, reason: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>>;

  /// The clock of the run of that name, or of the runs under it.
  fn run_clock<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<RunClock, PortFailure>>;

  /// The cap a run of a work key carries for an agent, in seconds.
  fn editor_cap<'a>(&'a self, work_key: &'a str, agent: &'a str)
  -> BoxFuture<'a, Result<i64, PortFailure>>;

  /// Those of the names a unit, an open execution or a pane's run holds, or none where nobody could say.
  fn alive<'a>(&'a self, names: &'a [String]) -> BoxFuture<'a, Result<Option<Vec<String>>, PortFailure>>;

  /// Whether the deployment rests.
  fn rest(&self) -> BoxFuture<'_, Result<Rest, PortFailure>>;

  /// Those of the named rulebooks the agency hands over, in the order asked.
  fn rulebooks<'a>(&'a self, names: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Rulebook>, PortFailure>>;

  /// Tells the agency which documents moved.
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

  // The turn posted while its events are read off the stream opened before it: the run it went out as handed on
  // once it started, and its end answered.
  async fn carried(
    &self,
    turn: &EditorTurn,
    started: oneshot::Sender<String>,
  ) -> Result<TurnEnd, TurnFault> {
    let stream = self.caller.open("/events", &[("kinds", "turn-started,turn-ended")]).await?;
    let mut events = std::pin::pin!(SseStream::from_bytes_stream(stream.bytes_stream()));
    let posted = self.caller.post::<Delivered>("/session/turn", turn);
    let mut posted = std::pin::pin!(posted);
    let mut started = Some(started);
    let mut run: Option<String> = None;
    let mut delivered: Option<Delivery> = None;
    let mut grace = std::pin::pin!(tokio::time::sleep(std::time::Duration::MAX / 4));
    let events_url = self.caller.url("/events");

    loop {
      tokio::select! {
        answer = &mut posted, if delivered.is_none() => match answer {
          Ok(answer) => {
            delivered = Some(answer.delivery);
            grace.as_mut().reset(tokio::time::Instant::now() + END_GRACE);
          }
          Err(PortFailure::Answered { answer: Some(error), .. }) if error.code == "session-busy" => {
            return Err(TurnFault::Busy);
          }
          Err(failure) => return Err(failure.into()),
        },
        event = events.next() => {
          let event = match event {
            Some(Ok(event)) => event,
            Some(Err(why)) => {
              return Err(TurnFault::Failed(PortFailure::Transport { url: events_url, why: why.to_string() }));
            }
            None => {
              return Err(TurnFault::Failed(PortFailure::Transport {
                url: events_url,
                why: "the event stream ended before the turn did".to_owned(),
              }));
            }
          };
          let Some(data) = event.data.as_deref() else { continue };

          match event.event.as_deref() {
            Some("turn-started") => {
              let Ok(begun) = serde_json::from_str::<Begun>(data) else { continue };

              if run.is_none() && (begun.run == turn.run || begun.session == turn.session) {
                if let Some(started) = started.take() {
                  let _ = started.send(begun.run.clone());
                }
                run = Some(begun.run);
              }
            }
            Some("turn-ended") => {
              let Ok(ended) = serde_json::from_str::<Ended>(data) else { continue };
              let ours = ended.end.run == turn.run
                || run.as_deref() == Some(ended.end.run.as_str())
                || (run.is_none() && ended.session == turn.session);

              if ours {
                return Ok(ended.end);
              }
            }
            _ => {}
          }
        }
        () = &mut grace, if delivered.is_some() => {
          return match (delivered, run) {
            (Some(Delivery::Blocked), Some(run)) => Ok(TurnEnd { run, outcome: TurnOutcome::Blocked, answer: None, reason: None }),
            _ => Err(TurnFault::Failed(PortFailure::Transport {
              url: events_url,
              why: "the turn was answered but its end never came on the event stream".to_owned(),
            })),
          };
        }
      }
    }
  }
}

#[derive(Deserialize)]
struct Agents {
  agents: Vec<Registration>,
}

#[derive(Serialize)]
struct Scope {
  kind: &'static str,
}

#[derive(Serialize)]
struct SessionAsk<'a> {
  agent: &'a str,
  scope: Scope,
}

#[derive(Deserialize)]
struct Session {
  id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Delivery {
  Taken,
  Blocked,
}

#[derive(Deserialize)]
struct Delivered {
  delivery: Delivery,
}

#[derive(Deserialize)]
struct Begun {
  session: String,
  run: String,
}

#[derive(Deserialize)]
struct Ended {
  session: String,
  #[serde(flatten)]
  end: TurnEnd,
}

#[derive(Serialize)]
struct StopAsk<'a> {
  run: &'a str,
  reason: &'a str,
}

#[derive(Deserialize)]
struct Stopped {
  ok: bool,
}

#[derive(Serialize)]
struct NamesAsk<'a> {
  names: &'a [String],
}

#[derive(Deserialize)]
struct Alive {
  answered: bool,
  #[serde(default)]
  alive: Option<Vec<String>>,
}

#[derive(Serialize)]
struct ReloadAsk<'a> {
  paths: &'a [String],
  now: bool,
  agent: &'a str,
  rooms: &'a [i64],
}

#[derive(Deserialize)]
struct Reloaded {
  #[allow(dead_code)]
  paths: Vec<String>,
}

impl Agency for HttpAgency {
  fn registrations<'a>(
    &'a self,
    agent: Option<&'a str>,
  ) -> BoxFuture<'a, Result<Vec<Registration>, PortFailure>> {
    Box::pin(async move {
      let answer: Agents = self.caller.get("/agents", &[]).await?;

      Ok(
        answer
          .agents
          .into_iter()
          .filter(|known| agent.is_none_or(|agent| known.name == agent))
          .collect(),
      )
    })
  }

  fn editor_session<'a>(&'a self, work_key: &'a str) -> BoxFuture<'a, Result<String, PortFailure>> {
    Box::pin(async move {
      let ask = SessionAsk {
        agent: work_key,
        scope: Scope { kind: "editor" },
      };
      let session: Session = self.caller.post("/session/start", &ask).await?;

      Ok(session.id)
    })
  }

  fn editor_turn<'a>(
    &'a self,
    turn: &'a EditorTurn,
    started: oneshot::Sender<String>,
  ) -> BoxFuture<'a, Result<TurnEnd, TurnFault>> {
    Box::pin(self.carried(turn, started))
  }

  fn stop_run<'a>(&'a self, run: &'a str, reason: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Stopped = self.caller.post("/runs/stop", &StopAsk { run, reason }).await?;

      Ok(answer.ok)
    })
  }

  /// No route is served for a run's clock yet (the brain's `docs/boundary-api.md`, 「The agency」), so no run is kept.
  fn run_clock<'a>(&'a self, _name: &'a str) -> BoxFuture<'a, Result<RunClock, PortFailure>> {
    Box::pin(async move { Ok(RunClock::default()) })
  }

  /// No route is served for an editor's cap yet, so it is the bound the agency documents for an editor run.
  fn editor_cap<'a>(
    &'a self,
    _work_key: &'a str,
    _agent: &'a str,
  ) -> BoxFuture<'a, Result<i64, PortFailure>> {
    Box::pin(async move { Ok(EDITOR_CAP_SECONDS) })
  }

  fn alive<'a>(&'a self, names: &'a [String]) -> BoxFuture<'a, Result<Option<Vec<String>>, PortFailure>> {
    Box::pin(async move {
      let answer: Alive = self.caller.post("/runs/alive", &NamesAsk { names }).await?;

      Ok(answer.answered.then(|| answer.alive.unwrap_or_default()))
    })
  }

  fn rest(&self) -> BoxFuture<'_, Result<Rest, PortFailure>> {
    Box::pin(async move { self.caller.get("/rest", &[]).await })
  }

  /// No route is served for the rulebooks yet, so none is handed over.
  fn rulebooks<'a>(&'a self, _names: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Rulebook>, PortFailure>> {
    Box::pin(async move { Ok(vec![]) })
  }

  fn reload_documents<'a>(&'a self, reload: &'a Reload) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let ask = ReloadAsk {
        paths: &reload.paths,
        now: false,
        agent: &reload.agent,
        rooms: &reload.rooms,
      };
      let _: Reloaded = self.caller.post("/documents/reload", &ask).await?;

      Ok(true)
    })
  }
}
