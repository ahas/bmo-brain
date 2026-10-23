//! The write judgement's material and mechanics, through the served routes (docs/boundary-api.md, 「Writing」):
//! the likes of a candidate, a learning applied whole by its judgement and source, and the consolidation's
//! reads and marks with the due signal in recall.

mod support;

use bmo_brain_client::{
  BrainClient, Consolidated, Learning, MemoryStatus, NewBrain, NewMemory, RecallAsk, SimilarAsk, Source,
  SubjectKey, UnconsolidatedQuery,
};
use support::{Served, labels, refused};

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

fn memory(kind: &str, text: &str, subjects: Vec<SubjectKey>) -> NewMemory {
  NewMemory {
    key: None,
    kind: kind.to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects,
    importance: None,
    valid_from: None,
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
  }
}

fn learning(kind: &str, text: &str, source: Source) -> Learning {
  Learning {
    kind: kind.to_owned(),
    text: text.to_owned(),
    subjects: vec![subject("person", "1")],
    importance: Some(6),
    source,
    confirms: vec![],
    strengthens: None,
    replaces: vec![],
    grounds: vec![],
    valid_from: None,
    key: None,
    layer_id: None,
    actor_name: "agent-session".to_owned(),
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

#[tokio::test]
async fn the_likes_of_a_candidate_are_the_live_rows_about_its_subjects_or_holding_its_words() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "1");
  let home = client
    .memory_add(brain, &memory("fact", "The member lives in the north.", vec![person.clone()]))
    .await
    .unwrap();
  let garden = client
    .memory_add(brain, &memory("fact", "The garden grows tomatoes.", vec![]))
    .await
    .unwrap();
  client
    .memory_add(brain, &memory("episode", "Said: tomatoes are ripe.", vec![person.clone()]))
    .await
    .unwrap();
  client.memory_add(brain, &memory("fact", "Unrelated weather.", vec![])).await.unwrap();
  let likes = client
    .similar(
      brain,
      &SimilarAsk {
        text: "The member's garden has tomatoes".to_owned(),
        subjects: vec![person.clone()],
        limit: None,
      },
    )
    .await
    .unwrap();
  let ids: Vec<i64> = likes.iter().map(|m| m.id).collect();
  assert!(ids.contains(&home.id) && ids.contains(&garden.id), "{ids:?}");
  assert!(!likes.iter().any(|m| m.kind == "episode" || m.text.contains("weather")), "{ids:?}");
  let none = client
    .similar(
      brain,
      &SimilarAsk {
        text: String::new(),
        subjects: vec![],
        limit: None,
      },
    )
    .await
    .unwrap();
  assert!(none.is_empty());
  assert_eq!(
    refused(
      client
        .similar(
          brain,
          &SimilarAsk {
            text: "x".to_owned(),
            subjects: vec![],
            limit: Some(0)
          }
        )
        .await
    ),
    (400, "bad-request".to_owned())
  );
}

#[tokio::test]
async fn a_learning_is_applied_whole_by_its_judgement_and_its_source() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "1");
  let north = client
    .memory_add(brain, &memory("fact", "The member lives in the north.", vec![person.clone()]))
    .await
    .unwrap();

  // A duplicate: confirmed, nothing written.
  let confirmed = client
    .learn(
      brain,
      &Learning {
        confirms: vec![north.id],
        ..learning("fact", "Lives up north.", Source::Own)
      },
    )
    .await
    .unwrap();
  assert_eq!((confirmed.confirmed_count, confirmed.written.is_none()), (1, true));
  let after = client.memory(brain, north.id).await.unwrap();
  assert!(after.last_confirmed_at.is_some());

  // A strengthening: a version over the old one.
  let stronger = client
    .learn(
      brain,
      &Learning {
        strengthens: Some(north.id),
        ..learning("fact", "The member lives in the north, by the lake.", Source::Own)
      },
    )
    .await
    .unwrap();
  let written = stronger.written.unwrap();
  assert!(stronger.versioned);
  assert_eq!(written.lineage_id, north.id);
  assert_eq!(client.memory(brain, north.id).await.unwrap().status, MemoryStatus::Superseded);

  // A contradiction from the person's own word closes the old fact.
  let moved = client
    .learn(
      brain,
      &Learning {
        replaces: vec![written.id],
        ..learning("fact", "The member moved to the south.", Source::Owner)
      },
    )
    .await
    .unwrap();
  assert_eq!(moved.invalidated_ids, vec![written.id]);
  let south = moved.written.unwrap();
  assert_eq!((south.kind.as_str(), south.status), ("fact", MemoryStatus::Active));
  assert_eq!(south.labels.get("source").map(String::as_str), Some("owner"));
  assert_eq!(client.memory(brain, written.id).await.unwrap().status, MemoryStatus::Invalidated);

  // A contradiction told by someone else is heard: a rumor beside the fact, closing nothing.
  let heard = client
    .learn(
      brain,
      &Learning {
        replaces: vec![south.id],
        ..learning("fact", "They say the member went abroad.", Source::Told)
      },
    )
    .await
    .unwrap();
  assert!(heard.invalidated_ids.is_empty());
  let rumor = heard.written.unwrap();
  assert_eq!(rumor.kind, "rumor");
  assert_eq!(client.memory(brain, south.id).await.unwrap().status, MemoryStatus::Active);

  // Grounds link episodes as subjects, and only episodes.
  let episode = client
    .memory_add(brain, &memory("episode", "Said: I moved south.", vec![person.clone()]))
    .await
    .unwrap();
  let grounded = client
    .learn(
      brain,
      &Learning {
        grounds: vec![episode.id],
        ..learning("tip", "The member likes the south.", Source::Inferred)
      },
    )
    .await
    .unwrap();
  let tip = grounded.written.unwrap();
  assert!(tip.subjects.iter().any(|s| s.kind == "episode" && s.key == episode.id.to_string()));
  assert_eq!(tip.kind, "tip");
  assert_eq!(
    refused(
      client
        .learn(
          brain,
          &Learning {
            grounds: vec![south.id],
            ..learning("tip", "x", Source::Own)
          }
        )
        .await
    ),
    (400, "bad-request".to_owned())
  );
  let kinds = served.event_kinds().await;
  assert!(kinds.contains(&"memory.written".to_owned()) && kinds.contains(&"memory.moved".to_owned()));
}

#[tokio::test]
async fn unconsolidated_episodes_are_read_most_important_first_and_marked_with_the_due_signal_in_recall() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = subject("person", "1");
  let small = client
    .memory_add(
      brain,
      &NewMemory {
        importance: Some(2),
        ..memory("episode", "Small talk.", vec![person.clone()])
      },
    )
    .await
    .unwrap();
  let big = client
    .memory_add(
      brain,
      &NewMemory {
        importance: Some(9),
        ..memory("episode", "A decision was made.", vec![person.clone()])
      },
    )
    .await
    .unwrap();
  let before = client
    .recall(
      brain,
      &RecallAsk {
        speakers: vec![person.clone()],
        words: "decision".to_owned(),
        record: Some(false),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!((before.unconsolidated_importance, before.consolidation_due), (11, false));
  let listed = client.unconsolidated(brain, &UnconsolidatedQuery { limit: None }).await.unwrap();
  assert_eq!(listed.iter().map(|m| m.id).collect::<Vec<_>>(), [big.id, small.id]);
  for _ in 0..16 {
    client
      .memory_add(
        brain,
        &NewMemory {
          importance: Some(9),
          ..memory("episode", "More news.", vec![])
        },
      )
      .await
      .unwrap();
  }
  let due = client
    .recall(
      brain,
      &RecallAsk {
        record: Some(false),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert!(
    due.consolidation_due && due.unconsolidated_importance >= 150,
    "{}",
    due.unconsolidated_importance
  );
  let marked = client
    .consolidated(
      brain,
      &Consolidated {
        episode_ids: vec![big.id, small.id, 999],
      },
    )
    .await
    .unwrap();
  assert_eq!(marked.recorded_count, 2);
  let left = client.unconsolidated(brain, &UnconsolidatedQuery { limit: Some(100) }).await.unwrap();
  assert!(!left.iter().any(|m| m.id == big.id || m.id == small.id));
  assert!(client.memory(brain, big.id).await.unwrap().consolidated_at.is_some());
  let again = client
    .consolidated(
      brain,
      &Consolidated {
        episode_ids: vec![big.id],
      },
    )
    .await
    .unwrap();
  assert_eq!(again.recorded_count, 0);
  assert_eq!(
    refused(
      client
        .consolidated(
          brain,
          &Consolidated {
            episode_ids: vec![]
          }
        )
        .await
    ),
    (400, "bad-request".to_owned())
  );
}
