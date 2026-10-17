//! bmo-job in memory: the items a test sets finished, and every call recorded.

use std::collections::BTreeSet;
use std::sync::Mutex;

use bmo_brain::ports::http::PortFailure;
use bmo_brain::ports::job::{ItemState, Job};
use futures_util::future::BoxFuture;
use serde_json::json;

use super::calls::Calls;

#[derive(Debug, Default)]
pub struct FakeJob {
  pub finished: Mutex<BTreeSet<i64>>,
  pub calls: Calls,
}

impl Job for FakeJob {
  fn item_states<'a>(&'a self, items: &'a [i64]) -> BoxFuture<'a, Result<Vec<ItemState>, PortFailure>> {
    self.calls.take("item_states", json!({ "items": items }));
    let finished = self.finished.lock().unwrap();
    let states = items
      .iter()
      .map(|&id| ItemState {
        id,
        done: finished.contains(&id),
      })
      .collect();
    Box::pin(async move { Ok(states) })
  }
}
