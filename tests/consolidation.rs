//! The order a consolidation reads its episodes in (docs/design.md, 「Consolidation」): priority, gain × (1 +
//! need), with novelty against the live rows of the episode's subjects and need from their use, and a tag's
//! room episodes written around it read out right after it; each row answered with its reasons.

use crate::support;

use bmo_brain_client::{
  BrainClient, NewBrain, NewMemory, SubjectKey, UnconsolidatedEpisode, UnconsolidatedQuery,
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
  }
}

async fn brain_of(client: &BrainClient) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
    })
    .await
    .unwrap()
    .id
}

async fn read(client: &BrainClient, brain: i64) -> Vec<UnconsolidatedEpisode> {
  client.unconsolidated(brain, &UnconsolidatedQuery { limit: None }).await.unwrap()
}

fn near(value: f64, expected: f64) -> bool {
  (value - expected).abs() < 1e-9
}

#[tokio::test]
async fn an_episode_about_what_is_used_outranks_a_more_important_one_no_one_needs() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "person:a");
  let fact = client
    .memory_add(brain, &memory("fact", "The member collects stamps.", 5, std::slice::from_ref(&person)))
    .await
    .unwrap();
  sqlx::query("UPDATE memories SET hits = 20 WHERE id = $1")
    .bind(fact.id)
    .execute(&served.store.pool)
    .await
    .unwrap();
  let needed = client
    .memory_add(brain, &memory("episode", "Talked about the weather.", 4, std::slice::from_ref(&person)))
    .await
    .unwrap();
  let unneeded = client
    .memory_add(brain, &memory("episode", "A decision was made.", 7, &[]))
    .await
    .unwrap();
  let order = read(client, brain).await;
  assert_eq!(order.iter().map(|e| e.memory.id).collect::<Vec<_>>(), [needed.id, unneeded.id]);
  // Nothing the episode says is in the fact: novelty one, gain twice the importance; the subject's rows were
  // used twenty times: need ln 21.
  let first = &order[0];
  assert!(near(first.gain, 8.0) && near(first.need, 21_f64.ln()), "{first:?}");
  assert!(near(first.priority, 8.0 * (1.0 + 21_f64.ln())), "{first:?}");
  assert_eq!(first.tagged_by, None);
  // No subject: novelty one and no need.
  let second = &order[1];
  assert!(
    near(second.gain, 14.0) && near(second.need, 0.0) && near(second.priority, 14.0),
    "{second:?}"
  );
}

#[tokio::test]
async fn an_episode_its_subjects_facts_already_say_gains_its_importance_alone() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "person:a");
  client
    .memory_add(brain, &memory("fact", "The member collects stamps.", 5, std::slice::from_ref(&person)))
    .await
    .unwrap();
  let said = client
    .memory_add(brain, &memory("episode", "Member collected stamps!", 6, std::slice::from_ref(&person)))
    .await
    .unwrap();
  let order = read(client, brain).await;
  let episode = order.iter().find(|e| e.memory.id == said.id).unwrap();
  // Its stems are the fact's: overlap one, novelty zero. The episode's own use counts in the need: none.
  assert!(near(episode.gain, 6.0), "{episode:?}");
  assert!(near(episode.need, 0.0) && near(episode.priority, 6.0), "{episode:?}");
}

#[tokio::test]
async fn a_tag_pulls_its_rooms_episodes_written_within_a_session_to_right_after_it() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let room = subject("room", "room:a");
  let other_room = subject("room", "room:b");
  let tag = client
    .memory_add(brain, &memory("episode", "The roof leaked badly.", 9, std::slice::from_ref(&room)))
    .await
    .unwrap();
  let middle = client.memory_add(brain, &memory("episode", "Paid the plumber.", 6, &[])).await.unwrap();
  let elsewhere = client
    .memory_add(brain, &memory("episode", "Ordered new curtains.", 5, std::slice::from_ref(&other_room)))
    .await
    .unwrap();
  let long_before = client
    .memory_add(brain, &memory("episode", "Swept the porch.", 3, std::slice::from_ref(&room)))
    .await
    .unwrap();
  let weak = client
    .memory_add(brain, &memory("episode", "Moved the bucket.", 2, std::slice::from_ref(&room)))
    .await
    .unwrap();
  // One of the room's episodes was written two hours before the tag, outside its window.
  sqlx::query("UPDATE memories SET created_at = $2 WHERE id = $1")
    .bind(long_before.id)
    .bind(tag.created_at - 2 * 3_600_000)
    .execute(&served.store.pool)
    .await
    .unwrap();
  let order = read(client, brain).await;
  // By priority alone (twice the importance each): tag, middle, elsewhere, long before, weak.
  let ids: Vec<(i64, Option<i64>)> = order.iter().map(|e| (e.memory.id, e.tagged_by)).collect();
  assert_eq!(
    ids,
    [
      (tag.id, None),
      (weak.id, Some(tag.id)),
      (middle.id, None),
      (elsewhere.id, None),
      (long_before.id, None)
    ]
  );
  assert!(near(order[1].priority, 4.0) && near(order[1].gain, 4.0) && near(order[1].need, 0.0));
  // The limit takes the first of the order as it stands.
  let first_two = client.unconsolidated(brain, &UnconsolidatedQuery { limit: Some(2) }).await.unwrap();
  assert_eq!(first_two.iter().map(|e| e.memory.id).collect::<Vec<_>>(), [tag.id, weak.id]);
}
