//! The events a row waits on: read as the grammar writes them, and woken by the same kind and item.

use memory_rows::events::{Event, event_wakes, read_event};

fn event(kind: &str, item: Option<i64>) -> Event {
  Event {
    kind: kind.to_owned(),
    item,
  }
}

#[test]
fn an_event_is_read_as_the_grammar_writes_it() {
  assert_eq!(read_event("deployed:#773"), Some(event("deployed", Some(773))));
  assert_eq!(read_event("queue-changed"), Some(event("queue-changed", None)));
  assert_eq!(read_event("queue-changed:5"), None);
  assert_eq!(read_event("elsewhere"), None);
}

#[test]
fn a_row_wakes_on_its_kind_and_on_its_item_where_it_names_one() {
  assert!(event_wakes(&event("deployed", None), &event("deployed", Some(5))));
  assert!(event_wakes(&event("deployed", Some(5)), &event("deployed", Some(5))));
  assert!(!event_wakes(&event("deployed", Some(5)), &event("deployed", Some(6))));
  assert!(!event_wakes(&event("job-ended", None), &event("deployed", None)));
}
