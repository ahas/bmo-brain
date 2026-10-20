//! Memory rows, their matches and notes, through the served routes (docs/boundary-api.md, 「Memory rows」 and
//! 「Notes」).

mod support;

use bmo_brain_client::{
  MemoriesQuery, MemoryChange, MemoryState, NewMemory, NewMemoryMatch, NewNote, NotesQuery, Object,
};
use serde_json::json;
use support::{Served, labels, refused};

fn row(scope: &str, kind: &str, text: &str) -> NewMemory {
  NewMemory {
    scope: scope.to_owned(),
    subject_key: None,
    kind: kind.to_owned(),
    text: text.to_owned(),
    conditions: vec![],
    labels: labels(&[]),
    due_at: None,
    state: None,
  }
}

#[tokio::test]
async fn a_row_is_added_read_and_listed_by_its_fields() {
  let served = Served::new().await;
  let client = &served.client;
  let condition: Object = json!({ "when": "asked" }).as_object().unwrap().clone();
  let first = client
    .memory_add(&NewMemory {
      subject_key: Some("person-1".to_owned()),
      conditions: vec![condition.clone()],
      labels: labels(&[("source", "talk")]),
      due_at: Some(5_000),
      ..row("room:a", "promise", "Bring the Report back")
    })
    .await
    .unwrap();
  client.memory_add(&row("room:b", "check", "Look at the door")).await.unwrap();
  client.memory_add(&row("agent:a", "state", "Halfway")).await.unwrap();

  assert_eq!(first.state, MemoryState::Standing);
  assert_eq!(first.conditions, vec![condition]);
  assert_eq!(client.memory(first.id).await.unwrap(), first);

  let texts = |page: bmo_brain_client::Page<bmo_brain_client::Memory>| {
    page.items.into_iter().map(|memory| memory.text).collect::<Vec<_>>()
  };
  let asked = |query: MemoriesQuery| async move { texts(client.memories(&query).await.unwrap()) };
  assert_eq!(
    asked(MemoriesQuery {
      scope: Some("room:a".into()),
      ..Default::default()
    })
    .await,
    ["Bring the Report back"]
  );
  assert_eq!(
    asked(MemoriesQuery {
      scope_prefix: Some("room:".into()),
      ..Default::default()
    })
    .await
    .len(),
    2
  );
  assert_eq!(
    asked(MemoriesQuery {
      subject_key: Some("person-1".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert_eq!(
    asked(MemoriesQuery {
      kind: Some("check".into()),
      ..Default::default()
    })
    .await,
    ["Look at the door"]
  );
  assert_eq!(
    asked(MemoriesQuery {
      label: Some("source".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert_eq!(
    asked(MemoriesQuery {
      label: Some("source=talk".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert!(
    asked(MemoriesQuery {
      label: Some("source=mail".into()),
      ..Default::default()
    })
    .await
    .is_empty()
  );
  assert_eq!(
    asked(MemoriesQuery {
      due_before: Some(5_000),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert_eq!(
    asked(MemoriesQuery {
      q: Some("report".into()),
      ..Default::default()
    })
    .await
    .len(),
    1
  );
  assert!(
    asked(MemoriesQuery {
      q: Some("50%".into()),
      ..Default::default()
    })
    .await
    .is_empty()
  );

  let page = client
    .memories(&MemoriesQuery {
      limit: Some(2),
      ..Default::default()
    })
    .await
    .unwrap();
  assert!(page.more);
  let rest = client
    .memories(&MemoriesQuery {
      after: Some(page.items[1].id),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!((rest.items.len(), rest.more), (1, false));
  assert_eq!(
    refused(
      client
        .memories(&MemoriesQuery {
          limit: Some(501),
          ..Default::default()
        })
        .await
    ),
    (400, "bad-request".into())
  );
  assert_eq!(refused(client.memory(first.id + 100).await), (404, "not-found".into()));
}

#[tokio::test]
async fn a_row_refuses_a_blank_text_and_a_scope_too_long() {
  let served = Served::new().await;
  assert_eq!(refused(served.client.memory_add(&row("room:a", "promise", "  ")).await).0, 400);
  assert_eq!(
    refused(served.client.memory_add(&row(&"s".repeat(201), "promise", "Words")).await).0,
    400
  );
}

#[tokio::test]
async fn a_change_moves_a_row_only_as_its_states_allow() {
  let served = Served::new().await;
  let client = &served.client;
  let added = client
    .memory_add(&NewMemory {
      due_at: Some(10),
      ..row("room:a", "promise", "Call back")
    })
    .await
    .unwrap();
  let state = |state| MemoryChange {
    state: Some(state),
    ..Default::default()
  };

  let done = client.memory_update(added.id, &state(MemoryState::Done)).await.unwrap();
  assert_eq!(done.state, MemoryState::Done);
  assert_eq!(done.due_at, Some(10));
  assert_eq!(
    client.memory_update(added.id, &state(MemoryState::Done)).await.unwrap().state,
    MemoryState::Done
  );
  assert_eq!(
    refused(client.memory_update(added.id, &state(MemoryState::Candidate)).await),
    (409, "conflict".into())
  );

  let again = client.memory_update(added.id, &state(MemoryState::Standing)).await.unwrap();
  assert_eq!(again.state, MemoryState::Standing);
  let changed = client
    .memory_update(
      added.id,
      &MemoryChange {
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
  assert_eq!(refused(client.memory_update(added.id + 9, &state(MemoryState::Off)).await).0, 404);
  assert_eq!(
    served.event_kinds().await,
    ["memory.created", "memory.changed", "memory.changed", "memory.changed", "memory.changed"]
  );
}

#[tokio::test]
async fn a_match_counts_each_row_it_names_and_refuses_a_row_that_does_not_stand() {
  let served = Served::new().await;
  let client = &served.client;
  let one = client.memory_add(&row("room:a", "rule", "Answer briefly")).await.unwrap();
  let two = client.memory_add(&row("room:a", "rule", "Answer in kind")).await.unwrap();
  let matched = NewMemoryMatch {
    memory_ids: vec![one.id, two.id, one.id],
    actor_name: "reader-a".into(),
    spent_ms: 12,
    token_count: 30,
  };

  let recorded = client.memory_match_add(&matched).await.unwrap();
  assert_eq!(recorded.memory_ids, matched.memory_ids);
  let read = client.memory(one.id).await.unwrap();
  assert_eq!((read.match_count, read.last_matched_at), (1, Some(recorded.created_at)));

  let missing = NewMemoryMatch {
    memory_ids: vec![two.id + 50],
    ..matched
  };
  assert_eq!(refused(client.memory_match_add(&missing).await).0, 400);
  assert_eq!(client.memory(two.id).await.unwrap().match_count, 1);
}

#[tokio::test]
async fn notes_are_added_and_listed_by_scope_and_subject() {
  let served = Served::new().await;
  let client = &served.client;
  let note = |scope: &str, subject: Option<&str>, text: &str| NewNote {
    scope: scope.into(),
    subject_key: subject.map(Into::into),
    text: text.into(),
    actor_name: "writer-a".into(),
  };
  let first = client.note_add(&note("room:a", Some("person-1"), "Likes tea")).await.unwrap();
  client.note_add(&note("room:a", None, "Meets on Fridays")).await.unwrap();
  client.note_add(&note("room:b", None, "Quiet room")).await.unwrap();

  let of_room = client
    .notes(&NotesQuery {
      scope: Some("room:a".into()),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(of_room.items.len(), 2);
  assert_eq!(of_room.items[0], first);
  let of_person = client
    .notes(&NotesQuery {
      subject_key: Some("person-1".into()),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(of_person.items, vec![first]);
  assert_eq!(refused(client.note_add(&note("room:a", None, "")).await).0, 400);
}
