//! What a sleep reads of its brain (docs/design.md, 「Sleep」 and 「Use」): the duplicate candidates, pairs of
//! live rows about one subject close in words, each with its reasons; the rows written since the last sleep;
//! and an old rumor nobody uses losing rank in recall, and only rank.

use crate::support;

use bmo_brain_client::{
  BrainClient, ConsolidationQuery, DuplicatesQuery, NewBrain, NewMemory, Recall, RecallAsk, RecallPart,
  SubjectKey,
};
use support::{Served, labels};

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

fn memory(kind: &str, text: &str, importance: i64, subjects: &[SubjectKey]) -> NewMemory {
  NewMemory {
    key: None,
    kind: kind.to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects: subjects.to_vec(),
    importance: Some(importance),
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

fn pairs_of(pairs: &[bmo_brain_client::DuplicateCandidate]) -> Vec<(i64, i64)> {
  pairs.iter().map(|pair| (pair.first.id, pair.second.id)).collect()
}

#[tokio::test]
async fn two_rows_about_one_subject_saying_one_thing_are_a_candidate_and_rows_about_others_are_not() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "person:a");
  let other = subject("person", "person:b");
  let room = subject("room", "room:a");
  let first = client
    .memory_add(
      brain,
      &memory("fact", "The member keeps bees behind the barn.", 5, &[person.clone(), room.clone()]),
    )
    .await
    .unwrap();
  let second = client
    .memory_add(
      brain,
      &memory("fact", "The member keeps bees behind the old barn.", 6, &[person.clone(), room]),
    )
    .await
    .unwrap();
  // Close in words but about someone else, far in words about the same person, and kinds never compared.
  client
    .memory_add(
      brain,
      &memory("fact", "The member keeps bees behind the barn.", 5, std::slice::from_ref(&other)),
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &memory("fact", "The member plays the cello on Sundays.", 5, std::slice::from_ref(&person)),
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &memory(
        "episode",
        "The member said they keep bees behind the barn.",
        5,
        std::slice::from_ref(&person),
      ),
    )
    .await
    .unwrap();

  let pairs = client.duplicates(brain, &DuplicatesQuery::default()).await.unwrap();

  assert_eq!(pairs_of(&pairs), [(first.id, second.id)], "{pairs:?}");
  let pair = &pairs[0];
  // Shared twice over, the person and the room, and answered once with both.
  assert_eq!(pair.subject_ids.len(), 2, "{pair:?}");
  // Stems {member, keep, bee, behind, barn} against the same and {old}: five of six.
  assert!((pair.word_share - 5.0 / 6.0).abs() < 1e-9, "{pair:?}");
  assert_eq!(pair.vector_similarity, None);
}

#[tokio::test]
async fn a_closed_row_is_never_a_candidate_and_words_alone_leave_a_short_name_apart() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "person:a");
  client
    .memory_add(
      brain,
      &memory("fact", "Members call them Sushi Brother.", 5, std::slice::from_ref(&person)),
    )
    .await
    .unwrap();
  client
    .memory_add(brain, &memory("name", "Sushi Brother", 5, std::slice::from_ref(&person)))
    .await
    .unwrap();
  let closed = client
    .memory_add(
      brain,
      &memory("fact", "Members call them Sushi Brother.", 5, std::slice::from_ref(&person)),
    )
    .await
    .unwrap();
  client
    .memory_move(
      brain,
      closed.id,
      &bmo_brain_client::MemoryMove {
        status: bmo_brain_client::MemoryStatus::Invalidated,
        at: None,
        actor_name: "writer-a".to_owned(),
        reason: Some("dropped".to_owned()),
        text: None,
        standing: None,
      },
    )
    .await
    .unwrap();

  let pairs = client.duplicates(brain, &DuplicatesQuery::default()).await.unwrap();

  // The closed copy says what the fact says and meets nothing; the name holds two of the fact's four stems,
  // under the word share, so by words alone the two stand apart (by meaning they meet: tests/vectors.rs).
  assert!(pairs.is_empty(), "{pairs:?}");
}

#[tokio::test]
async fn the_closest_pairs_come_first_and_the_limit_is_held_to_its_bounds() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "person:a");
  let about = std::slice::from_ref(&person);
  let a = client
    .memory_add(brain, &memory("fact", "Orchard ladder barn loft", 5, about))
    .await
    .unwrap();
  let b = client
    .memory_add(brain, &memory("fact", "Orchard ladder barn loft", 5, about))
    .await
    .unwrap();
  let c = client
    .memory_add(brain, &memory("fact", "Orchard ladder barn shed", 5, about))
    .await
    .unwrap();

  let pairs = client.duplicates(brain, &DuplicatesQuery { limit: None }).await.unwrap();
  // a and b hold the same four stems; each with c holds three of five.
  assert_eq!(pairs_of(&pairs), [(a.id, b.id), (a.id, c.id), (b.id, c.id)], "{pairs:?}");

  let first = client.duplicates(brain, &DuplicatesQuery { limit: Some(1) }).await.unwrap();
  assert_eq!(pairs_of(&first), [(a.id, b.id)]);

  for limit in [0, 501] {
    let refused = client.duplicates(brain, &DuplicatesQuery { limit: Some(limit) }).await.unwrap_err();
    assert!(refused.to_string().contains("400"), "{refused}");
  }
}

#[tokio::test]
async fn the_consolidation_state_asked_with_a_moment_counts_the_rows_written_after_it() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "person:a");
  let about = std::slice::from_ref(&person);
  let before = client
    .memory_add(brain, &memory("fact", "The member keeps bees.", 5, about))
    .await
    .unwrap();
  client
    .memory_add(brain, &memory("episode", "Talked about the bees.", 4, about))
    .await
    .unwrap();
  client
    .memory_add(brain, &memory("fact", "The member sells honey.", 5, about))
    .await
    .unwrap();

  let unasked = client.consolidation(brain).await.unwrap();
  assert_eq!((unasked.unconsolidated_count, unasked.written_since), (1, None));

  let since = client
    .consolidation_since(
      brain,
      &ConsolidationQuery {
        since: Some(before.created_at),
      },
    )
    .await
    .unwrap();
  // The row the moment is taken from is not counted; the episode and the fact after it are.
  assert_eq!((since.unconsolidated_count, since.written_since), (1, Some(2)));

  let quiet = client
    .consolidation_since(
      brain,
      &ConsolidationQuery {
        since: Some(before.created_at + 60 * 60 * 1_000),
      },
    )
    .await
    .unwrap();
  assert_eq!(quiet.written_since, Some(0));
}

fn ask(words: &str) -> RecallAsk {
  RecallAsk {
    room: None,
    speakers: vec![],
    words: words.to_owned(),
    session_key: None,
    session_opened_at: None,
    budget_tokens: None,
    record: Some(false),
    triggers: vec![],
  }
}

fn searched(recall: &Recall) -> Vec<i64> {
  recall
    .chosen
    .iter()
    .filter(|c| c.part == RecallPart::Searched)
    .map(|c| c.memory_id)
    .collect()
}

#[tokio::test]
async fn an_old_rumor_nobody_uses_loses_rank_and_stays_a_candidate_while_a_use_lifts_the_fade() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let old = client
    .memory_add(brain, &memory("rumor", "Kites fly over the north field.", 8, &[]))
    .await
    .unwrap();
  let fresh = client
    .memory_add(brain, &memory("rumor", "Kites sold at the north market.", 3, &[]))
    .await
    .unwrap();
  let day = 24 * 60 * 60 * 1_000_i64;

  // Fresh, the more important rumor goes first.
  assert_eq!(searched(&client.recall(brain, &ask("kites north")).await.unwrap()), [old.id, fresh.id]);

  // Written forty days ago and never used: it fades behind the fresher one, and is still handed out.
  sqlx::query(
    "UPDATE memories SET created_at = created_at - $2, valid_from = valid_from - $2 WHERE id = $1",
  )
  .bind(old.id)
  .bind(40 * day)
  .execute(&served.store.pool)
  .await
  .unwrap();
  assert_eq!(searched(&client.recall(brain, &ask("kites north")).await.unwrap()), [fresh.id, old.id]);

  // A use within the span lifts the fade at once.
  client
    .hits_record(
      brain,
      &bmo_brain_client::Hits {
        memory_ids: vec![old.id],
        session_key: None,
        actor_name: "agent-session".to_owned(),
        at: None,
      },
    )
    .await
    .unwrap();
  assert_eq!(searched(&client.recall(brain, &ask("kites north")).await.unwrap()), [old.id, fresh.id]);
}

#[tokio::test]
async fn no_type_but_a_rumor_fades() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let old = client
    .memory_add(brain, &memory("fact", "Kites fly over the north field.", 8, &[]))
    .await
    .unwrap();
  let fresh = client
    .memory_add(brain, &memory("fact", "Kites sold at the north market.", 3, &[]))
    .await
    .unwrap();
  sqlx::query(
    "UPDATE memories SET created_at = created_at - $2, valid_from = valid_from - $2 WHERE id = $1",
  )
  .bind(old.id)
  .bind(400 * 24 * 60 * 60 * 1_000_i64)
  .execute(&served.store.pool)
  .await
  .unwrap();
  assert_eq!(searched(&client.recall(brain, &ask("kites north")).await.unwrap()), [old.id, fresh.id]);
}
