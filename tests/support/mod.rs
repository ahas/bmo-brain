//! What the service's tests share: a migrated database of the test's own, and the service answering over it on
//! a port of its own, with a client that reaches it.

mod scratch;

use bmo_brain::api::console::Console;
use bmo_brain::api::{Brain, Stopping, router};
use bmo_brain::store::Store;
use bmo_brain_client::BrainClient;
use scratch::Scratch;
use sqlx::PgConnection;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// The token the served brain takes.
pub const TOKEN: &str = "the-brain-token";
/// The console's token the served brain takes.
pub const CONSOLE_TOKEN: &str = "the-console-token";

/// The service over a migrated, empty database of the test's own, answering on a port of its own until the
/// guard goes.
pub struct Served {
  pub store: Store,
  pub client: BrainClient,
  pub base: String,
  serving: JoinHandle<()>,
  stop: watch::Sender<bool>,
  scratch: Scratch,
}

impl Served {
  pub async fn new() -> Served {
    Self::serving(None, vec![], None).await
  }

  /// The service searching by meaning with the embedding model named, its vectors read in and no embedder
  /// standing; a test stands one or fills the backlog itself.
  pub async fn embedding(config: bmo_brain::store::vectors::EmbeddingConfig) -> Served {
    Self::serving(None, vec![], Some(config)).await
  }

  /// The service serving the console bundle built into this folder, where it holds one.
  pub async fn with_bundle(folder: &std::path::Path) -> Served {
    Self::serving(Some(folder), vec![], None).await
  }

  /// The service serving the console bundle built into this folder, its screen calling the services named.
  pub async fn with_bundle_calling(folder: &std::path::Path, calls: &[&str]) -> Served {
    Self::serving(Some(folder), calls.iter().map(|call| (*call).to_owned()).collect(), None).await
  }

  async fn serving(
    bundle: Option<&std::path::Path>,
    calls: Vec<String>,
    embedding: Option<bmo_brain::store::vectors::EmbeddingConfig>,
  ) -> Served {
    let scratch = Scratch::copy().await;
    let pool = PgPoolOptions::new().max_connections(8).connect_with(scratch.options()).await.unwrap();
    let store = Store::over(pool).embedding(embedding);
    store.load_vectors().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let console = match bundle {
      Some(folder) => Console::over(CONSOLE_TOKEN.to_owned(), folder, calls),
      None => Console {
        token: CONSOLE_TOKEN.to_owned(),
        bundle: None,
        calls,
      },
    };
    let (stop, stopping) = Stopping::new();
    let brain = Brain {
      store: store.clone(),
      token: TOKEN.to_owned(),
      console: Some(console),
      stopping,
    };
    let serving = tokio::spawn(async move {
      axum::serve(listener, router(brain)).await.unwrap();
    });
    let client = BrainClient::new(&base, TOKEN).unwrap();
    Served {
      store,
      client,
      base,
      serving,
      stop,
      scratch,
    }
  }

  /// The service's stop raised, as a SIGTERM raises it.
  pub fn stop(&self) {
    self.stop.send_replace(true);
  }

  /// A client presenting another token.
  pub fn with_token(&self, token: &str) -> BrainClient {
    BrainClient::new(&self.base, token).unwrap()
  }

  /// The database's address, as a caller handed one opens it.
  pub fn address(&self) -> String {
    self.scratch.address()
  }

  /// The events published so far, every kind.
  pub async fn event_kinds(&self) -> Vec<String> {
    let page = self.client.events(&bmo_brain_client::EventsQuery::default()).await.unwrap();
    page.items.into_iter().map(|event| event.kind).collect()
  }
}

impl Drop for Served {
  fn drop(&mut self) {
    self.serving.abort();
  }
}

/// An empty database of the test's own, not even the migrations' record in it, for a test that lays down what
/// an install already holds before the service migrates it; dropped when the guard goes.
pub struct Bare {
  scratch: Scratch,
}

impl Bare {
  pub async fn blank() -> Bare {
    Bare {
      scratch: Scratch::blank().await,
    }
  }

  pub async fn connect(&self) -> PgConnection {
    self.scratch.connect().await
  }

  /// A pool over the database, as the service holds one.
  pub async fn pool(&self) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
      .max_connections(2)
      .connect_with(self.scratch.options())
      .await
      .unwrap()
  }
}

/// The failure a call came to: its status and code.
pub fn refused<T: std::fmt::Debug>(answer: bmo_brain_client::Answer<T>) -> (u16, String) {
  match answer {
    Err(bmo_brain_client::ClientFailure::Answered {
      status,
      answer: Some(answer),
    }) => (status, answer.code),
    other => panic!("a refusal was due, and the call came to {other:?}"),
  }
}

/// Labels from pairs.
pub fn labels(pairs: &[(&str, &str)]) -> bmo_brain_client::Labels {
  pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
}
