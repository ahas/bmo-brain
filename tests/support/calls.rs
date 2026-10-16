//! What a fake was asked, in the order it was asked: each call's operation and the body it would have
//! sent, so a test reads a fake's calls as it reads a stub server's.

use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

/// One call a fake took.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
  pub operation: &'static str,
  pub body: Value,
}

/// The calls a fake took.
#[derive(Debug, Default)]
pub struct Calls(Mutex<Vec<Call>>);

impl Calls {
  pub fn take(&self, operation: &'static str, body: impl Serialize) {
    let body = serde_json::to_value(body).expect("a call's body is JSON");
    self.0.lock().unwrap().push(Call { operation, body });
  }

  /// Every call so far.
  pub fn all(&self) -> Vec<Call> {
    self.0.lock().unwrap().clone()
  }

  /// The bodies of the calls of one operation.
  pub fn of(&self, operation: &str) -> Vec<Value> {
    let calls = self.0.lock().unwrap();
    calls
      .iter()
      .filter(|call| call.operation == operation)
      .map(|call| call.body.clone())
      .collect()
  }
}
