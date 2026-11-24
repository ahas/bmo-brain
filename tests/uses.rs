//! A memory's use (docs/design.md, 「Use」): every answer carries how often and when last a row was used, an
//! explicit read records a use and the screen's list does not, recall breaks a tie by the latest use, and the
//! migration lets the storage strength go and keeps the use.

use crate::support;

use bmo_brain_client::{BrainClient, Hits, MemoriesQuery, NewBrain, NewMemory, RecallAsk, RecallPart};
use support::{Served, labels};

fn memory(kind: &str, text: &str) -> NewMemory {
  NewMemory {
    key: None,
    kind: kind.to_owned(),
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

#[tokio::test]
async fn an_explicit_read_records_a_use_and_a_list_that_is_none_does_not() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let row = client
    .memory_add(brain, &memory("fact", "The harvest starts in autumn."))
    .await
    .unwrap();
  let other = client.memory_add(brain, &memory("fact", "The well is deep.")).await.unwrap();
  // A list by status alone is no explicit read, and a read that says not to record records nothing.
  for query in [
    MemoriesQuery {
      status: Some("active".to_owned()),
      ..MemoriesQuery::default()
    },
    MemoriesQuery {
      q: Some("harvest".to_owned()),
      record: Some(false),
      ..MemoriesQuery::default()
    },
  ] {
    client.memories(brain, &query).await.unwrap();
    assert_eq!(client.memory(brain, row.id).await.unwrap().use_count, 0);
  }
  // A read by words records a use of the rows it answers, and answers them as the read left them.
  let read = client
    .memories(
      brain,
      &MemoriesQuery {
        q: Some("harvest".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(read.items.iter().map(|m| (m.id, m.use_count)).collect::<Vec<_>>(), [(row.id, 1)]);
  assert!(read.items[0].last_used_at.is_some());
  // A read by type is explicit too; a read of one row by its id is not.
  client
    .memories(
      brain,
      &MemoriesQuery {
        kind: Some("fact".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  client.memory(brain, other.id).await.unwrap();
  assert_eq!(client.memory(brain, row.id).await.unwrap().use_count, 2);
  assert_eq!(client.memory(brain, other.id).await.unwrap().use_count, 1);
  // The list orders by use.
  let ordered = client
    .memories(
      brain,
      &MemoriesQuery {
        order: Some("-useCount".to_owned()),
        record: Some(false),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(ordered.items.iter().map(|m| m.id).collect::<Vec<_>>(), [row.id, other.id]);
}

#[tokio::test]
async fn recall_puts_the_row_used_last_first_among_rows_that_score_the_same() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let first = client
    .memory_add(brain, &memory("fact", "Tomatoes grow in the first bed by the gate."))
    .await
    .unwrap();
  let second = client
    .memory_add(brain, &memory("fact", "Tomatoes ripen slowly near the shed."))
    .await
    .unwrap();
  let ask = RecallAsk {
    words: "tomatoes".to_owned(),
    record: Some(false),
    ..RecallAsk::default()
  };
  let searched = |recall: &bmo_brain_client::Recall| -> Vec<i64> {
    recall
      .chosen
      .iter()
      .filter(|c| c.part == RecallPart::Searched)
      .map(|c| c.memory_id)
      .collect()
  };
  let before = client.recall(brain, &ask).await.unwrap();
  assert_eq!(searched(&before), [first.id, second.id], "{:?}", before.chosen);
  client
    .hits_record(
      brain,
      &Hits {
        memory_ids: vec![second.id],
        session_key: None,
        actor_name: "agent-session".to_owned(),
        at: None,
      },
    )
    .await
    .unwrap();
  let after = client.recall(brain, &ask).await.unwrap();
  assert_eq!(searched(&after), [second.id, first.id], "{:?}", after.chosen);
}
