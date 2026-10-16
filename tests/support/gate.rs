//! The gate in memory: the sanitiser hands every text back as it came unless a test set what a path comes
//! back as, the sheet judgement answers the sheets a test queued, and every call is recorded.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use bmo_brain::ports::gate::{Gate, Sanitised, SheetAsk};
use bmo_brain::ports::http::PortFailure;
use futures_util::future::BoxFuture;
use serde_json::json;

use super::calls::Calls;

#[derive(Debug, Default)]
pub struct FakeGate {
  /// What a document at a path comes back as.
  pub sanitised: Mutex<BTreeMap<String, Sanitised>>,
  /// The sheets the judgements come to, one per ask; an ask past them comes to none.
  pub sheets: Mutex<VecDeque<Option<String>>>,
  pub calls: Calls,
}

impl Gate for FakeGate {
  fn sanitise<'a>(&'a self, path: &'a str, text: &'a str) -> BoxFuture<'a, Result<Sanitised, PortFailure>> {
    self.calls.take("sanitise", json!({ "path": path, "text": text }));
    let set = self.sanitised.lock().unwrap().get(path).cloned();
    let answer = set.unwrap_or_else(|| Sanitised::Text(text.to_owned()));
    Box::pin(async move { Ok(answer) })
  }

  fn judge_sheet<'a>(&'a self, ask: &'a SheetAsk) -> BoxFuture<'a, Result<Option<String>, PortFailure>> {
    self.calls.take("judge_sheet", ask);
    let sheet = self.sheets.lock().unwrap().pop_front().flatten();
    Box::pin(async move { Ok(sheet) })
  }
}
