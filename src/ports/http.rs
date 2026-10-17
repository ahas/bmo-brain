//! How a port reaches its unit: a POST of a JSON body, a GET with its query or a GET held open for the events it
//! streams, under the unit's base address and behind the unit's token, offered again only while the connection is
//! refused (`docs/boundary-api.md`, 「Wire conventions」 and 「Calls the brain makes」).

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use reqwest::Client;
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Where a unit answers and the token it is called with, as the service's configuration gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
  pub base: String,
  pub token: String,
}

/// How a wait between two offers of a call is spent. The caller counts the time it has waited by the
/// intervals it asked for, never by reading a clock, so a waiter that returns at once makes a whole window
/// pass at once.
pub trait Wait: Send + Sync {
  fn wait(&self, interval: Duration) -> BoxFuture<'_, ()>;
}

/// Waits on the real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealClock;

impl Wait for RealClock {
  fn wait(&self, interval: Duration) -> BoxFuture<'_, ()> {
    Box::pin(tokio::time::sleep(interval))
  }
}

/// How long a call whose connection was refused keeps being offered, and how often.
#[derive(Clone)]
pub struct Patience {
  /// The most time spent waiting before the call is given up.
  pub window: Duration,
  /// The wait before each new offer; zero offers a call only once.
  pub every: Duration,
  pub waiter: Arc<dyn Wait>,
}

impl Default for Patience {
  /// Two minutes, offered every two seconds, on the real clock.
  fn default() -> Self {
    Self {
      window: Duration::from_secs(120),
      every: Duration::from_secs(2),
      waiter: Arc::new(RealClock),
    }
  }
}

impl fmt::Debug for Patience {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Patience")
      .field("window", &self.window)
      .field("every", &self.every)
      .finish_non_exhaustive()
  }
}

/// The error shape a unit answers a non-2xx status with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
  pub code: String,
  pub error: String,
}

/// A typed refusal: the code, its English sentence and the code's own parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
  pub code: String,
  pub message: String,
  #[serde(flatten)]
  pub params: Map<String, Value>,
}

/// Why a call came to no answer of the operation's shape. A refusal is an answer, so it is never one of
/// these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortFailure {
  /// Nothing listened at the address for the whole patience window.
  Unreachable {
    url: String,
    waited: Duration,
    why: String,
  },
  /// The unit answered, and not with success. Such a call is never offered again.
  Answered {
    url: String,
    status: u16,
    /// The error shape, where the answer carried one.
    answer: Option<WireError>,
  },
  /// The unit answered with success, in a body that is not the operation's answer.
  Undecodable { url: String, why: String },
  /// The call failed after it may have reached a listener, so it is not offered again.
  Transport { url: String, why: String },
}

impl fmt::Display for PortFailure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Unreachable { url, waited, why } => {
        write!(f, "Nothing was listening at {url} after waiting {waited:?}: {why}")
      }
      Self::Answered {
        url,
        status,
        answer: Some(answer),
      } => write!(f, "{url} answered {status}: {}", answer.error),
      Self::Answered {
        url,
        status,
        answer: None,
      } => write!(f, "{url} answered {status}."),
      Self::Undecodable { url, why } => {
        write!(f, "{url} answered in a shape the operation does not have: {why}")
      }
      Self::Transport { url, why } => write!(f, "The call to {url} failed: {why}"),
    }
  }
}

impl std::error::Error for PortFailure {}

/// One unit as a port reaches it.
#[derive(Debug, Clone)]
pub struct Caller {
  base: String,
  token: String,
  patience: Patience,
  client: Client,
}

impl Caller {
  /// A caller of the unit at the peer's base address, with the default patience. It reads no environment
  /// of its own, so no proxy is taken from it.
  pub fn new(peer: &Peer) -> Self {
    let client = Client::builder()
      .no_proxy()
      .build()
      .expect("a client with no TLS and no proxy always builds");
    Self {
      base: peer.base.trim_end_matches('/').to_owned(),
      token: peer.token.clone(),
      patience: Patience::default(),
      client,
    }
  }

  pub fn with_patience(mut self, patience: Patience) -> Self {
    self.patience = patience;
    self
  }

  /// The address of the operation at `path`.
  pub fn url(&self, path: &str) -> String {
    format!("{}{path}", self.base)
  }

  /// Posts the body at `path` and decodes a successful answer as the operation's, offering the call again
  /// only while the connection is refused and the patience window lasts.
  pub async fn post<T: DeserializeOwned>(
    &self,
    path: &str,
    body: &impl Serialize,
  ) -> Result<T, PortFailure> {
    let url = self.url(path);
    let body = serde_json::to_vec(body).map_err(|error| PortFailure::Transport {
      url: url.clone(),
      why: error.to_string(),
    })?;
    let answer = self
      .send(&url, || {
        self
          .client
          .post(&url)
          .bearer_auth(&self.token)
          .header(CONTENT_TYPE, "application/json")
          .body(body.clone())
      })
      .await?;

    decoded(url, answer).await
  }

  /// Asks for `path` with the query's fields, as a GET, and decodes a successful answer as the operation's, offered
  /// again on the same terms as a post.
  pub async fn get<T: DeserializeOwned>(
    &self,
    path: &str,
    query: &[(&str, &str)],
  ) -> Result<T, PortFailure> {
    let url = self.url(path);
    let answer = self.send(&url, || self.client.get(&url).bearer_auth(&self.token).query(query)).await?;

    decoded(url, answer).await
  }

  /// Opens `path` with the query's fields as a GET whose answer goes on arriving, a stream of server-sent events,
  /// and answers it once its head says it succeeded.
  pub async fn open(&self, path: &str, query: &[(&str, &str)]) -> Result<reqwest::Response, PortFailure> {
    let url = self.url(path);
    let answer = self.send(&url, || self.client.get(&url).bearer_auth(&self.token).query(query)).await?;

    if answer.status().is_success() {
      return Ok(answer);
    }

    Err(failed(url, answer).await)
  }

  // The request sent, offered again only while the connection is refused and the patience window lasts.
  async fn send(
    &self,
    url: &str,
    request: impl Fn() -> reqwest::RequestBuilder,
  ) -> Result<reqwest::Response, PortFailure> {
    let Patience {
      window,
      every,
      waiter,
    } = &self.patience;
    let mut waited = Duration::ZERO;
    loop {
      let error = match request().send().await {
        Ok(answer) => return Ok(answer),
        Err(error) => error,
      };
      if !refused(&error) {
        return Err(PortFailure::Transport {
          url: url.to_owned(),
          why: chain(&error),
        });
      }
      if every.is_zero() || waited + *every > *window {
        return Err(PortFailure::Unreachable {
          url: url.to_owned(),
          waited,
          why: chain(&error),
        });
      }
      waiter.wait(*every).await;
      waited += *every;
    }
  }
}

// A successful answer read as the operation's, and any other as the failure it is.
async fn decoded<T: DeserializeOwned>(url: String, answer: reqwest::Response) -> Result<T, PortFailure> {
  if !answer.status().is_success() {
    return Err(failed(url, answer).await);
  }

  let bytes = answer.bytes().await.map_err(|error| PortFailure::Transport {
    url: url.clone(),
    why: chain(&error),
  })?;

  serde_json::from_slice(&bytes).map_err(|error| PortFailure::Undecodable {
    url,
    why: error.to_string(),
  })
}

// An answer that is no success, with the error shape it carried where it carried one.
async fn failed(url: String, answer: reqwest::Response) -> PortFailure {
  let status = answer.status().as_u16();

  match answer.bytes().await {
    Ok(bytes) => PortFailure::Answered {
      url,
      status,
      answer: serde_json::from_slice(&bytes).ok(),
    },
    Err(error) => PortFailure::Transport {
      url,
      why: chain(&error),
    },
  }
}

/// Whether the connection was refused, so nothing listened and the call cannot have been received.
fn refused(error: &reqwest::Error) -> bool {
  let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
  while let Some(error) = cause {
    if error
      .downcast_ref::<io::Error>()
      .is_some_and(|error| error.kind() == io::ErrorKind::ConnectionRefused)
    {
      return true;
    }
    cause = error.source();
  }
  false
}

/// The error and every cause beneath it, in one line.
fn chain(error: &reqwest::Error) -> String {
  let mut words = error.to_string();
  let mut cause = std::error::Error::source(error);
  while let Some(error) = cause {
    words.push_str(": ");
    words.push_str(&error.to_string());
    cause = error.source();
  }
  words
}

/// The answer of an operation that says only whether it was done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Done {
  pub ok: bool,
}

/// The body of an operation that sends nothing.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Nothing {}
