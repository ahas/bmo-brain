//! The service's routes served on a local port of the test's own and called as another unit calls them: a scratch
//! database, the other units in memory, the chat agent's brain as a git repository, a data root beside it, the
//! prompts the tests' writers are handed, and the words a launched run would start the binary with.

use std::path::Path;
use std::sync::Arc;

use bmo_brain::api::memories::Withering;
use bmo_brain::api::{Api, router};
use bmo_brain::ports::agency::Registration;
use bmo_brain::ports::http::{Caller, Peer};
use serde_json::Value;

use super::agency::{FakeAgency, registration};
use super::editing::repository;
use super::gate::FakeGate;
use super::job::FakeJob;
use super::receiver::FakeReceiver;
use super::scratch::Scratch;
use super::sorting::prompts_root;
use super::starter::FakeStarter;
use super::{AGENT, OWNER};

/// The token the served routes are called with.
pub const TOKEN: &str = "brain-token";

/// The words a launched run starts the binary with, before its own.
pub const PROGRAM: [&str; 3] = ["bmo-brain", "--config", "brain.toml"];

pub struct Served {
  pub api: Arc<Api>,
  pub scratch: Scratch,
  pub agency: Arc<FakeAgency>,
  pub receiver: Arc<FakeReceiver>,
  pub gate: Arc<FakeGate>,
  pub job: Arc<FakeJob>,
  pub starter: Arc<FakeStarter>,
  pub caller: Caller,
  pub base: String,
  pub brain: tempfile::TempDir,
  pub data: tempfile::TempDir,
}

/// A caller of the served routes with a token of its own.
pub fn caller(base: &str, token: &str) -> Caller {
  Caller::new(&Peer {
    base: base.to_owned(),
    token: token.to_owned(),
  })
}

/// The routes served over a fresh stage whose chat agent, `AGENT`, keeps its memory at the brain root and whose
/// owner is `OWNER`.
pub async fn served() -> Served {
  let scratch = Scratch::new().await;
  let agency = Arc::new(FakeAgency::default());
  let receiver = Arc::new(FakeReceiver::default());
  let gate = Arc::new(FakeGate::default());
  let job = Arc::new(FakeJob::default());
  let starter = Arc::new(FakeStarter::default());
  let brain = tempfile::tempdir().unwrap();
  let data = tempfile::tempdir().unwrap();

  *agency.registrations.lock().unwrap() = vec![Registration {
    brain_folder: Some(String::new()),
    label: Some("Helper".to_owned()),
    ..registration(AGENT)
  }];
  repository(brain.path());

  let api = Arc::new(Api {
    db: scratch.db().clone(),
    database: scratch.address(),
    agency: agency.clone(),
    receiver: receiver.clone(),
    gate: gate.clone(),
    job: job.clone(),
    starter: starter.clone(),
    brain_root: brain.path().to_path_buf(),
    data_root: data.path().to_path_buf(),
    prompts_root: prompts_root(),
    chat_key: AGENT.to_owned(),
    owner: Some(OWNER),
    owner_names: vec!["Owner".to_owned()],
    service_unit: "brain-service".to_owned(),
    program: PROGRAM.map(str::to_owned).to_vec(),
    token: TOKEN.to_owned(),
    withering: Withering::default(),
  });
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());

  let routed = api.clone();

  tokio::spawn(async move { axum::serve(listener, router(routed)).await.unwrap() });

  Served {
    api,
    caller: caller(&base, TOKEN),
    scratch,
    agency,
    receiver,
    gate,
    job,
    starter,
    base,
    brain,
    data,
  }
}

impl Served {
  /// A route posted to, its answer read as JSON.
  pub async fn post(&self, path: &str, body: Value) -> Value {
    self.caller.post::<Value>(path, &body).await.unwrap()
  }

  /// A route read with GET, as a screen reads it: its status and its body.
  pub async fn get(&self, path: &str) -> (u16, Value) {
    let answer = reqwest::Client::new()
      .get(format!("{}{path}", self.base))
      .bearer_auth(TOKEN)
      .send()
      .await
      .unwrap();
    let status = answer.status().as_u16();

    (status, serde_json::from_slice(&answer.bytes().await.unwrap()).unwrap())
  }

  /// The brain root.
  pub fn root(&self) -> &Path {
    self.brain.path()
  }
}
