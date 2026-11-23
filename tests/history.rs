//! A memory's history (docs/design.md, 「Versions」): when a row stopped being current, a statement taken back,
//! the old and new words of a version in the event stream, the brain read as of a past moment, and a row told
//! after a later one taking its place behind it.

use crate::support;

use bmo_brain_client::{
  BrainClient, EventsQuery, Learning, MemoriesQuery, MemoryMerge, MemoryMove, MemoryStatus, MemoryVersion,
  NewBrain, NewMemory, RecallAsk, SimilarAsk, Source, SubjectKey,
};
use support::{Bare, Served, labels};

fn memory(text: &str) -> NewMemory {
  NewMemory {
    key: None,
    kind: "fact".to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects: vec![],
    importance: None,
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
    internal_only: None,
    shared: None,
    locked: None,
    standing: None,
  }
}

async fn brain_of(client: &BrainClient) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
      kind: None,
    })
    .await
    .unwrap()
    .id
}

fn version(text: &str, valid_from: Option<i64>) -> MemoryVersion {
  MemoryVersion {
    text: Some(text.to_owned()),
    valid_from,
    actor_name: "writer-a".to_owned(),
    ..MemoryVersion::default()
  }
}

#[tokio::test]
async fn a_row_records_when_it_stopped_being_current() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let first = client.memory_add(brain, &memory("The member lives in the north.")).await.unwrap();
  assert_eq!(first.superseded_at, None);
  let second = client
    .memory_version(brain, first.id, &version("The member lives in the south.", None))
    .await
    .unwrap();
  let replaced = client.memory(brain, first.id).await.unwrap();
  assert_eq!(replaced.superseded_at, Some(second.created_at));
  assert_eq!(second.supersedes, [first.id]);
  let other = client.memory_add(brain, &memory("The member keeps bees.")).await.unwrap();
  let merged = client
    .memory_merge(
      brain,
      &MemoryMerge {
        memory_ids: vec![second.id, other.id],
        kind: "fact".to_owned(),
        text: "The member lives in the south and keeps bees.".to_owned(),
        actor_name: "writer-a".to_owned(),
        ..MemoryMerge::default()
      },
    )
    .await
    .unwrap();
  for id in [second.id, other.id] {
    assert_eq!(client.memory(brain, id).await.unwrap().superseded_at, Some(merged.created_at));
  }
  assert_eq!(merged.supersedes, [second.id, other.id]);
  let moved = client
    .memory_move(
      brain,
      merged.id,
      &MemoryMove {
        status: MemoryStatus::Invalidated,
        at: Some(1_000),
        actor_name: "writer-a".to_owned(),
        reason: None,
        text: None,
        standing: None,
      },
    )
    .await
    .unwrap();
  // A move closes the row at the moment it is made, whatever moment it names for the span's end.
  assert_eq!(moved.valid_to, Some(1_000));
  assert!(
    moved.superseded_at.is_some_and(|at| at >= merged.created_at),
    "{:?}",
    moved.superseded_at
  );
}

#[tokio::test]
async fn the_migration_gives_a_superseded_row_its_successors_moment() {
  let bare = Bare::through(9).await;
  let mut db = bare.connect().await;
  let brain: i64 = sqlx::query_scalar(
    "INSERT INTO brains (account_key, agent_key, name, created_at) VALUES ('account-a', 'agent-a', 'a', 1) \
     RETURNING id",
  )
  .fetch_one(&mut db)
  .await
  .unwrap();
  let insert = "INSERT INTO memories (brain_id, type, text, status, lineage_id, valid_from, importance, \
    actor_name, labels, created_at) VALUES ($1, 'fact', $2, $3, 0, 1, 5, 'writer-a', '{}', $4) RETURNING id";
  let old: i64 = sqlx::query_scalar(insert)
    .bind(brain)
    .bind("Old")
    .bind("superseded")
    .bind(100_i64)
    .fetch_one(&mut db)
    .await
    .unwrap();
  let new: i64 = sqlx::query_scalar(insert)
    .bind(brain)
    .bind("New")
    .bind("active")
    .bind(200_i64)
    .fetch_one(&mut db)
    .await
    .unwrap();
  let closed: i64 = sqlx::query_scalar(insert)
    .bind(brain)
    .bind("Closed")
    .bind("invalidated")
    .bind(300_i64)
    .fetch_one(&mut db)
    .await
    .unwrap();
  sqlx::query("UPDATE memories SET superseded_by = $2 WHERE id = $1")
    .bind(old)
    .bind(new)
    .execute(&mut db)
    .await
    .unwrap();
  bmo_brain::store::MIGRATOR.run(&mut db).await.unwrap();
  let at: Vec<(i64, Option<i64>)> = sqlx::query_as("SELECT id, superseded_at FROM memories ORDER BY id")
    .fetch_all(&mut db)
    .await
    .unwrap();
  assert_eq!(at, [(old, Some(200)), (new, None), (closed, None)]);
}

#[tokio::test]
async fn a_retracted_statement_reads_as_retracted_in_its_successors_line() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let said = client.memory_add(brain, &memory("The member lives in Lyon.")).await.unwrap();
  let retracted = client
    .memory_move(
      brain,
      said.id,
      &MemoryMove {
        status: MemoryStatus::Retracted,
        at: None,
        actor_name: "writer-a".to_owned(),
        reason: Some("the member said it wrong".to_owned()),
        text: Some("The member lives in Lille.".to_owned()),
        standing: None,
      },
    )
    .await
    .unwrap();
  assert_eq!(retracted.status, MemoryStatus::Retracted);
  assert_eq!(retracted.reason.as_deref(), Some("the member said it wrong"));
  let meant = client.memory(brain, retracted.superseded_by.unwrap()).await.unwrap();
  assert_eq!((meant.status, meant.valid_from), (MemoryStatus::Active, said.valid_from));
  let recall = client
    .recall(
      brain,
      &RecallAsk {
        words: "where does the member live".to_owned(),
        record: Some(false),
        ..RecallAsk::default()
      },
    )
    .await
    .unwrap();
  assert!(
    recall.block.contains("(before, retracted: The member lives in Lyon.)"),
    "{}",
    recall.block
  );
  // A retracted statement never held: no reading as of any moment answers it, and it is not restored.
  let then = client
    .memories(
      brain,
      &MemoriesQuery {
        as_of: Some(said.valid_from),
        record: Some(false),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(then.items.iter().map(|m| m.id).collect::<Vec<_>>(), [meant.id]);
  // A retraction with no words in its place only closes the row.
  let other = client.memory_add(brain, &memory("The member owns a boat.")).await.unwrap();
  let taken = client
    .memory_move(
      brain,
      other.id,
      &MemoryMove {
        status: MemoryStatus::Retracted,
        at: None,
        actor_name: "writer-a".to_owned(),
        reason: None,
        text: None,
        standing: None,
      },
    )
    .await
    .unwrap();
  assert_eq!((taken.status, taken.superseded_by), (MemoryStatus::Retracted, None));
}

#[tokio::test]
async fn a_version_event_carries_the_old_words_and_the_new_beside_each_other() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let first = client.memory_add(brain, &memory("The kiln fires at dawn.")).await.unwrap();
  let second = client
    .memory_version(brain, first.id, &version("The kiln fires at noon.", None))
    .await
    .unwrap();
  let page = client.events(&EventsQuery::default()).await.unwrap();
  let last: Vec<&bmo_brain_client::Event> = page.items.iter().rev().take(2).collect();
  let (written, moved) = (last[0], last[1]);
  assert_eq!((moved.kind.as_str(), written.kind.as_str()), ("memory.moved", "memory.written"));
  assert_eq!(moved.data["text"], "The kiln fires at dawn.");
  assert_eq!(moved.data["supersededBy"], second.id);
  assert_eq!(written.data["text"], "The kiln fires at noon.");
  assert_eq!(written.data["supersedes"], serde_json::json!([first.id]));
}

#[tokio::test]
async fn a_reading_as_of_a_past_moment_answers_the_version_that_held_then() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let then = client
    .memory_add(
      brain,
      &NewMemory {
        valid_from: Some(1_000_000),
        ..memory("The member works at the bakery.")
      },
    )
    .await
    .unwrap();
  let now = client
    .memory_version(brain, then.id, &version("The member works at the library.", Some(2_000_000)))
    .await
    .unwrap();
  let as_of = |at: i64| MemoriesQuery {
    as_of: Some(at),
    record: Some(false),
    ..MemoriesQuery::default()
  };
  let ids = |page: bmo_brain_client::Page<bmo_brain_client::Memory>| -> Vec<i64> {
    page.items.into_iter().map(|m| m.id).collect()
  };
  assert_eq!(ids(client.memories(brain, &as_of(1_500_000)).await.unwrap()), [then.id]);
  assert_eq!(ids(client.memories(brain, &as_of(2_500_000)).await.unwrap()), [now.id]);
  assert!(ids(client.memories(brain, &as_of(500_000)).await.unwrap()).is_empty());
  let likes = client
    .similar(
      brain,
      &SimilarAsk {
        text: "Where does the member work?".to_owned(),
        subjects: vec![],
        limit: None,
        as_of: Some(1_500_000),
      },
    )
    .await
    .unwrap();
  assert_eq!(likes.iter().map(|m| m.id).collect::<Vec<_>>(), [then.id]);
}

#[tokio::test]
async fn a_row_told_after_a_later_one_takes_its_place_behind_it() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let live = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("home".to_owned()),
        valid_from: Some(2_000_000),
        ..memory("The member lives in Lille.")
      },
    )
    .await
    .unwrap();
  let earlier = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("home".to_owned()),
        valid_from: Some(1_000_000),
        ..memory("The member lived in Lyon.")
      },
    )
    .await
    .unwrap();
  assert_eq!(earlier.status, MemoryStatus::Superseded);
  assert_eq!(earlier.superseded_by, Some(live.id));
  assert_eq!(earlier.valid_to, Some(2_000_000));
  assert_eq!(earlier.lineage_id, live.lineage_id);
  assert_eq!(earlier.reason.as_deref(), Some("earlier-than-live"));
  assert_eq!(client.memory(brain, live.id).await.unwrap().status, MemoryStatus::Active);
  // A later start than the live row's is a conflict, as a key held always was.
  let later = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("home".to_owned()),
        valid_from: Some(3_000_000),
        ..memory("The member lives in Paris.")
      },
    )
    .await;
  assert_eq!(support::refused(later).0, 409);
  // A learning that began before the row it replaces closes nothing and is written behind it.
  let learned = client
    .learn(
      brain,
      &Learning {
        kind: "fact".to_owned(),
        text: "The member worked as a baker.".to_owned(),
        subjects: vec![SubjectKey {
          kind: "person".to_owned(),
          key: "person:a".to_owned(),
        }],
        importance: None,
        source: Source::Own,
        confirms: vec![],
        strengthens: None,
        replaces: vec![live.id],
        grounds: vec![],
        valid_from: Some(500_000),
        recall_condition: None,
        release_condition: None,
        key: None,
        layer_id: None,
        actor_name: "writer-a".to_owned(),
        labels: labels(&[]),
        internal_only: None,
        shared: None,
        locked: None,
        standing: None,
      },
    )
    .await
    .unwrap();
  assert!(learned.invalidated_ids.is_empty());
  let written = learned.written.unwrap();
  assert_eq!((written.status, written.superseded_by), (MemoryStatus::Superseded, Some(live.id)));
  assert_eq!(written.reason.as_deref(), Some("earlier-than-live"));
  assert_eq!(client.memory(brain, live.id).await.unwrap().status, MemoryStatus::Active);
}
