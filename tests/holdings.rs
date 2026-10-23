//! Locks on the store and what is held under a scope (docs/boundary-api.md, 「Locks」 and 「Scopes」). The
//! clock's passes are called with the moment they read, so no test waits on the clock.

mod support;

use bmo_brain::clock::{slow, tick};
use bmo_brain::store::now;
use bmo_brain::store::scopes::WITHER_AFTER_MS;
use bmo_brain_client::{
  FragmentHits, FragmentWrite, LocksQuery, NewLock, NewNote, NewScopedMemory, NewScopedMemoryMatch,
  ScopeChange, ScopeCounts, ScopeState, ScopedMemoryState,
};
use support::{Served, labels, refused};

fn lock(holder: &str, ttl_ms: Option<i64>) -> NewLock {
  NewLock {
    holder_name: holder.into(),
    reason: "Rewriting the shelf".into(),
    ttl_ms,
  }
}

#[tokio::test]
async fn one_holder_holds_the_store_until_it_lets_go_or_its_lock_runs_out() {
  let served = Served::new().await;
  let client = &served.client;
  let taken = client.lock(&lock("writer-a", None)).await.unwrap();
  assert_eq!(refused(client.lock(&lock("writer-b", None)).await), (409, "conflict".into()));
  let renewed = client.lock(&lock("writer-a", Some(60_000))).await.unwrap();
  assert_eq!(renewed.id, taken.id);
  assert!(renewed.expires_at.is_some());
  assert_eq!(client.locks(&LocksQuery::default()).await.unwrap().items, vec![renewed.clone()]);

  client.unlock(taken.id).await.unwrap();
  assert_eq!(refused(client.unlock(taken.id).await).0, 404);
  let short = client.lock(&lock("writer-b", Some(1_000))).await.unwrap();
  tick(&served.store, short.expires_at.unwrap()).await.unwrap();
  assert!(client.locks(&LocksQuery::default()).await.unwrap().items.is_empty());
  assert_eq!(refused(client.lock(&lock("writer-b", Some(0))).await).0, 400);
}

#[tokio::test]
async fn a_wait_for_the_store_answers_once_the_holder_lets_go() {
  let served = Served::new().await;
  let taken = served.client.lock(&lock("writer-a", None)).await.unwrap();
  let waiting = {
    let client = served.client.clone();
    tokio::spawn(async move {
      client
        .locks(&LocksQuery {
          wait_ms: Some(30_000),
          ..Default::default()
        })
        .await
    })
  };
  served.client.unlock(taken.id).await.unwrap();
  let answered = tokio::time::timeout(std::time::Duration::from_secs(20), waiting)
    .await
    .unwrap()
    .unwrap()
    .unwrap();
  assert!(answered.items.is_empty());
  let too_long = LocksQuery {
    wait_ms: Some(600_001),
    ..Default::default()
  };
  assert_eq!(refused(served.client.locks(&too_long).await).0, 400);
}

fn row(scope: &str) -> NewScopedMemory {
  NewScopedMemory {
    scope: scope.into(),
    subject_key: None,
    kind: "rule".into(),
    text: "Keep it short".into(),
    conditions: vec![],
    labels: labels(&[]),
    due_at: None,
    state: None,
  }
}

#[tokio::test]
async fn a_scope_is_counted_and_let_go_of_whole() {
  let served = Served::new().await;
  let client = &served.client;
  let kept = client.scoped_memory_add(&row("room:a")).await.unwrap();
  client.scoped_memory_add(&row("room:b")).await.unwrap();
  client
    .scoped_memory_match_add(&NewScopedMemoryMatch {
      memory_ids: vec![kept.id],
      actor_name: "reader-a".into(),
      spent_ms: 1,
      token_count: 1,
    })
    .await
    .unwrap();
  client
    .note_add(&NewNote {
      scope: "room:a".into(),
      subject_key: None,
      text: "A note".into(),
      actor_name: "writer-a".into(),
    })
    .await
    .unwrap();
  let piece = FragmentWrite {
    kind: "rule".into(),
    text: "Quiet".into(),
    scope: Some("room:a".into()),
    ..Default::default()
  };
  client.fragment_put("quiet", &piece).await.unwrap();
  client
    .fragment_hits(&FragmentHits {
      keys: vec!["quiet".into()],
      actor_name: "reader-a".into(),
      scope: Some("room:a".into()),
    })
    .await
    .unwrap();

  let counts = ScopeCounts {
    memories: 1,
    memory_matches: 1,
    notes: 1,
    watches: 0,
    reminders: 0,
    fragments: 1,
    fragment_hits: 1,
  };
  let held = client.scope("room:a").await.unwrap();
  assert_eq!((held.counts.clone(), held.state), (counts.clone(), ScopeState::Remembered));
  assert_eq!(client.scope_let_go("room:a", true).await.unwrap(), held);
  assert_eq!(client.scope("room:a").await.unwrap(), held);
  assert_eq!(client.scope_let_go("room:a", false).await.unwrap(), held);
  assert_eq!(refused(client.scope("room:a").await), (404, "not-found".into()));
  assert_eq!(client.scope("room:b").await.unwrap().counts.memories, 1);
  assert_eq!(refused(client.scope_let_go("room:c", true).await).0, 404);
}

#[tokio::test]
async fn a_forgotten_scope_withers_its_unmatched_rows_and_a_remembered_one_does_not() {
  let served = Served::new().await;
  let client = &served.client;
  let forgotten = client.scoped_memory_add(&row("room:a")).await.unwrap();
  let remembered = client.scoped_memory_add(&row("room:b")).await.unwrap();
  let marked = client
    .scope_mark(
      "room:a",
      &ScopeChange {
        state: ScopeState::Forgotten,
      },
    )
    .await
    .unwrap();
  assert_eq!(marked.state, ScopeState::Forgotten);
  assert_eq!(
    refused(
      client
        .scope_mark(
          "room:c",
          &ScopeChange {
            state: ScopeState::Forgotten
          }
        )
        .await
    )
    .0,
    404
  );

  slow(&served.store, now()).await.unwrap();
  assert_eq!(client.scoped_memory(forgotten.id).await.unwrap().state, ScopedMemoryState::Standing);
  slow(&served.store, now() + WITHER_AFTER_MS + 1_000).await.unwrap();
  assert_eq!(client.scoped_memory(forgotten.id).await.unwrap().state, ScopedMemoryState::Off);
  assert_eq!(client.scoped_memory(remembered.id).await.unwrap().state, ScopedMemoryState::Standing);
}
