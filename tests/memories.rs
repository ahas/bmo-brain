//! Memory rows, listed and changed through the served routes (docs/boundary-api.md, 「Memory rows」). No route
//! adds a row, so each test puts its rows in the table.

mod support;

use bmo_brain_client::{
  Object, Page, ScopedMemoriesQuery, ScopedMemory, ScopedMemoryChange, ScopedMemoryState,
};
use serde_json::json;
use support::{Row, Served, labels, refused, row};

#[tokio::test]
async fn rows_are_listed_by_their_fields() {
  let served = Served::new().await;
  let client = &served.client;
  let condition: Object = json!({ "when": "asked" }).as_object().unwrap().clone();
  let first = served
    .put_row(Row {
      subject_key: Some("person-1".to_owned()),
      conditions: vec![condition.clone()],
      labels: labels(&[("source", "talk")]),
      due_at: Some(5_000),
      ..row("room:a", "promise", "Bring the Report back")
    })
    .await;
  served.put_row(row("room:b", "check", "Look at the door")).await;
  served.put_row(row("agent:a", "state", "Halfway")).await;

  let all = client.scoped_memories(&ScopedMemoriesQuery::default()).await.unwrap();
  let read = &all.items[0];
  assert_eq!((read.id, read.state), (first, ScopedMemoryState::Standing));
  assert_eq!(read.conditions, vec![condition]);
  assert_eq!((read.match_count, read.last_matched_at), (0, None));

  let texts =
    |page: Page<ScopedMemory>| page.items.into_iter().map(|memory| memory.text).collect::<Vec<_>>();
  let asked =
    |query: ScopedMemoriesQuery| async move { texts(client.scoped_memories(&query).await.unwrap()) };
  assert_eq!(
    asked(ScopedMemoriesQuery {
      scope: Some("room:a".into()),
      ..Default::default()
    })
    .await,
    ["Bring the Report back"]
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      scope_prefix: Some("room:".into()),
      ..Default::default()
    })
    .await
    .len(),
    2
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      subject_key: Some("person-1".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      kind: Some("check".into()),
      ..Default::default()
    })
    .await,
    ["Look at the door"]
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      state: Some(ScopedMemoryState::Standing),
      ..Default::default()
    })
    .await
    .len(),
    3
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      label: Some("source".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      label: Some("source=talk".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert!(
    asked(ScopedMemoriesQuery {
      label: Some("source=mail".into()),
      ..Default::default()
    })
    .await
    .is_empty()
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      due_before: Some(5_000),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert_eq!(
    asked(ScopedMemoriesQuery {
      q: Some("report".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert!(
    asked(ScopedMemoriesQuery {
      q: Some("50%".into()),
      ..Default::default()
    })
    .await
    .is_empty()
  );

  let page = client
    .scoped_memories(&ScopedMemoriesQuery {
      limit: Some(2),
      ..Default::default()
    })
    .await
    .unwrap();
  assert!(page.more);
  let rest = client
    .scoped_memories(&ScopedMemoriesQuery {
      after: Some(page.items[1].id),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!((rest.items.len(), rest.more), (1, false));
  assert_eq!(
    refused(
      client
        .scoped_memories(&ScopedMemoriesQuery {
          limit: Some(501),
          ..Default::default()
        })
        .await
    ),
    (400, "bad-request".into())
  );
}

#[tokio::test]
async fn a_change_moves_a_row_only_as_its_states_allow() {
  let served = Served::new().await;
  let client = &served.client;
  let id = served
    .put_row(Row {
      due_at: Some(10),
      ..row("room:a", "promise", "Call back")
    })
    .await;
  let state = |state| ScopedMemoryChange {
    state: Some(state),
    ..Default::default()
  };

  let done = client.scoped_memory_update(id, &state(ScopedMemoryState::Done)).await.unwrap();
  assert_eq!(done.state, ScopedMemoryState::Done);
  assert_eq!(done.due_at, Some(10));
  assert_eq!(
    client.scoped_memory_update(id, &state(ScopedMemoryState::Done)).await.unwrap().state,
    ScopedMemoryState::Done
  );
  assert_eq!(
    refused(client.scoped_memory_update(id, &state(ScopedMemoryState::Candidate)).await),
    (409, "conflict".into())
  );

  let again = client.scoped_memory_update(id, &state(ScopedMemoryState::Standing)).await.unwrap();
  assert_eq!(again.state, ScopedMemoryState::Standing);
  let changed = client
    .scoped_memory_update(
      id,
      &ScopedMemoryChange {
        text: Some("Call back at noon".into()),
        labels: Some(labels(&[("by", "hand")])),
        due_at: Some(None),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(changed.text, "Call back at noon");
  assert_eq!(changed.labels, labels(&[("by", "hand")]));
  assert_eq!(changed.due_at, None);
  assert!(changed.changed_at > 1);
  let blank = ScopedMemoryChange {
    text: Some("  ".into()),
    ..Default::default()
  };
  assert_eq!(refused(client.scoped_memory_update(id, &blank).await).0, 400);
  assert_eq!(
    refused(client.scoped_memory_update(id + 9, &state(ScopedMemoryState::Off)).await).0,
    404
  );
  assert_eq!(
    served.event_kinds().await,
    ["memory.changed", "memory.changed", "memory.changed", "memory.changed"]
  );
}
