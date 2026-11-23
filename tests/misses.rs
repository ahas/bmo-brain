//! What a brain counts of its asks by words (docs/design.md, 「What the brain counts」): an ask that finds
//! nothing kept as a miss with its words and stems, one that finds a row counted only, and the misses read
//! newest first.

use crate::support;

use bmo_brain_client::{
  BrainClient, MemoriesQuery, MissesQuery, NewBrain, NewMemory, SimilarAsk, SubjectKey,
};
use support::{Served, labels};

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

fn similar(text: &str, subjects: Vec<SubjectKey>) -> SimilarAsk {
  SimilarAsk {
    text: text.to_owned(),
    subjects,
    limit: None,
    as_of: None,
  }
}

fn by_words(words: &str, record: Option<bool>) -> MemoriesQuery {
  MemoriesQuery {
    q: Some(words.to_owned()),
    record,
    ..MemoriesQuery::default()
  }
}

#[tokio::test]
async fn an_ask_that_finds_nothing_is_kept_as_a_miss_and_one_that_finds_a_row_is_counted() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  client.memory_add(brain, &memory("The kiln fires at dawn.")).await.unwrap();
  assert!(
    client
      .similar(brain, &similar("Where are the garden tomatoes?", vec![]))
      .await
      .unwrap()
      .is_empty()
  );
  assert_eq!(
    client.similar(brain, &similar("When does the kiln fire?", vec![])).await.unwrap().len(),
    1
  );
  assert!(client.memories(brain, &by_words("pottery", None)).await.unwrap().items.is_empty());
  assert_eq!(client.memories(brain, &by_words("kiln", None)).await.unwrap().items.len(), 1);
  // Neither the screen's read nor an ask with no words counts.
  client.memories(brain, &by_words("pottery", Some(false))).await.unwrap();
  let person = SubjectKey {
    kind: "person".to_owned(),
    key: "person:a".to_owned(),
  };
  client.similar(brain, &similar("", vec![person])).await.unwrap();

  let health = client.health().await.unwrap();
  let counted: Vec<(i64, i64)> = health
    .searches
    .iter()
    .filter(|count| count.brain_id == brain)
    .map(|count| (count.asks, count.misses))
    .collect();
  assert_eq!(counted, [(4, 2)]);
  let misses = client.misses(brain, &MissesQuery::default()).await.unwrap();
  let kept: Vec<(&str, &str, Vec<&str>)> = misses
    .items
    .iter()
    .map(|miss| {
      (
        miss.route.as_str(),
        miss.words.as_str(),
        miss.stems.iter().map(String::as_str).collect(),
      )
    })
    .collect();
  assert_eq!(
    kept,
    [
      ("memories", "pottery", vec!["potteri"]),
      ("similar", "Where are the garden tomatoes?", vec!["garden", "tomato"])
    ]
  );
  assert!(misses.items.iter().all(|miss| miss.brain_id == brain && miss.at > 0));
}

#[tokio::test]
async fn the_misses_are_read_newest_first_a_page_at_a_time() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  for words in ["first", "second", "third"] {
    client.memories(brain, &by_words(words, None)).await.unwrap();
  }
  let first = client
    .misses(
      brain,
      &MissesQuery {
        after: None,
        limit: Some(2),
      },
    )
    .await
    .unwrap();
  assert_eq!(
    first.items.iter().map(|miss| miss.words.as_str()).collect::<Vec<_>>(),
    ["third", "second"]
  );
  assert!(first.more);
  let rest = client
    .misses(
      brain,
      &MissesQuery {
        after: first.items.last().map(|miss| miss.id),
        limit: Some(2),
      },
    )
    .await
    .unwrap();
  assert_eq!(rest.items.iter().map(|miss| miss.words.as_str()).collect::<Vec<_>>(), ["first"]);
  assert!(!rest.more);
  assert_eq!(support::refused(client.misses(brain + 1, &MissesQuery::default()).await).0, 404);
}
