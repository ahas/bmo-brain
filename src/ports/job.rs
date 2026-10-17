//! The bmo-job port: whether TODO items are finished, which a fragment's status and a trim's stretch are settled
//! against (`docs/boundary-api.md`, 「bmo-job」). The call keeps bmo-job's own conventions: snake_case keys and an
//! answer read as it stands.

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::ports::http::{Caller, Peer, PortFailure};

/// One item's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemState {
  pub id: i64,
  /// Whether the item is finished.
  pub done: bool,
}

/// What the brain asks of bmo-job.
pub trait Job: Send + Sync {
  /// Whether each item is finished, in the order asked.
  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>>;
}

/// bmo-job over HTTP.
#[derive(Debug, Clone)]
pub struct HttpJob {
  caller: Caller,
}

impl HttpJob {
  pub fn new(peer: &Peer) -> Self {
    Self::with_caller(Caller::new(peer))
  }

  pub fn with_caller(caller: Caller) -> Self {
    Self { caller }
  }
}

#[derive(Serialize)]
struct ItemsAsk<'a> {
  items: &'a [i64],
}

#[derive(Deserialize)]
struct Items {
  items: Vec<ItemState>,
}

impl Job for HttpJob {
  /// Where bmo-job serves no such route yet (it answers 404), no item reads as finished.
  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>> {
    Box::pin(async move {
      match self.caller.post::<Items>("/todos/states", &ItemsAsk { items }).await {
        Ok(answer) => Ok(answer.items),
        Err(PortFailure::Answered { status: 404, .. }) => {
          Ok(items.iter().map(|&id| ItemState { id, done: false }).collect())
        }
        Err(failure) => Err(failure),
      }
    })
  }
}
