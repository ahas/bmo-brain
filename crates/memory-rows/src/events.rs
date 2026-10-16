//! The events a memory row may wait on (docs/memories.md): what a row names, and whether one that happened
//! wakes it.

use brain_support::js;
use serde::{Deserialize, Serialize};

/// One event as it happened, or as a row waits on it: none for the item is any item, or none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
  pub kind: String,
  pub item: Option<i64>,
}

/// The events a row may wait on, each with whether it names an item.
pub const EVENT_KINDS: [(&str, bool); 3] =
  [("deployed", true), ("job-ended", true), ("queue-changed", false)];

/// An event as a row names it, or none where it names one no longer in the grammar.
pub fn read_event(text: &str) -> Option<Event> {
  let parts: Vec<&str> = js::trim(text).split(':').collect();
  let (kind, names_item) = EVENT_KINDS.iter().find(|(kind, _)| *kind == js::trim(parts[0]))?;

  match parts.as_slice() {
    [_] => Some(Event {
      kind: (*kind).to_owned(),
      item: None,
    }),
    [_, item] if *names_item => {
      let item: i64 = js::trim(item).trim_start_matches('#').parse().ok()?;

      (item > 0).then(|| Event {
        kind: (*kind).to_owned(),
        item: Some(item),
      })
    }
    _ => None,
  }
}

/// Whether a row waiting on one event is woken by another: the same kind, and the same item where it names one.
pub fn event_wakes(waited: &Event, came: &Event) -> bool {
  waited.kind == came.kind && (waited.item.is_none() || waited.item == came.item)
}
