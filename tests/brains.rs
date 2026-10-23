//! The brains and their three tables, through the served routes (docs/boundary-api.md, 「Brains」): a brain
//! made and found, subjects and layers, memories written, versioned and moved, hits and confirmations, the
//! types, recall, the fold, the persona and the brain file.

mod support;

use bmo_brain_client::{
  BrainClient, BrainsQuery, Confirmations, ExportQuery, Fold, Hits, LayersQuery, MemoriesQuery, MemoryMove,
  MemoryStatus, MemoryVersion, NewBrain, NewLayer, NewMemory, NewSubject, RecallAsk, RecallPart,
  SubjectKey, SubjectsQuery,
};
use support::{Served, labels, refused};

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

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
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
  }
}

async fn brain_of(client: &BrainClient, agent: &str) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: agent.to_owned(),
      name: None,
    })
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn a_brain_is_made_once_per_account_and_agent_and_listed_by_them() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
    })
    .await
    .unwrap();
  assert_eq!(brain.name, "agent-a");
  let again = client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: Some("Other".to_owned()),
    })
    .await;
  assert_eq!(refused(again), (409, "conflict".to_owned()));
  brain_of(client, "agent-b").await;
  let listed = client
    .brains(&BrainsQuery {
      account_key: Some("account-a".to_owned()),
      agent_key: Some("agent-b".to_owned()),
      ..BrainsQuery::default()
    })
    .await
    .unwrap();
  assert_eq!(listed.items.len(), 1);
  assert_eq!(listed.items[0].agent_key, "agent-b");
  assert_eq!(client.brain(brain.id).await.unwrap(), brain);
  let gone = client.brain_let_go(brain.id).await.unwrap();
  assert_eq!(gone.id, brain.id);
  assert_eq!(refused(client.brain(brain.id).await), (404, "not-found".to_owned()));
  let kinds = served.event_kinds().await;
  assert!(kinds.contains(&"brain.created".to_owned()) && kinds.contains(&"brain.deleted".to_owned()));
}

#[tokio::test]
async fn a_subject_is_found_by_its_type_and_key_and_a_layer_by_its_name() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let person = client
    .subject_add(
      brain,
      &NewSubject {
        kind: "person".to_owned(),
        key: "1".to_owned(),
        description: "A member".to_owned(),
      },
    )
    .await
    .unwrap();
  let same = client
    .subject_add(
      brain,
      &NewSubject {
        kind: "person".to_owned(),
        key: "1".to_owned(),
        description: String::new(),
      },
    )
    .await
    .unwrap();
  assert_eq!(same.id, person.id);
  assert_eq!(same.description, "A member");
  let listed = client
    .subjects(
      brain,
      &SubjectsQuery {
        kind: Some("person".to_owned()),
        ..SubjectsQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(listed.items, vec![person.clone()]);
  let base = client
    .layer_add(
      brain,
      &NewLayer {
        name: "base".to_owned(),
        description: String::new(),
        position: None,
      },
    )
    .await
    .unwrap();
  let tone = client
    .layer_add(
      brain,
      &NewLayer {
        name: "tone".to_owned(),
        description: String::new(),
        position: None,
      },
    )
    .await
    .unwrap();
  assert_eq!((base.position, tone.position), (0, 1));
  let again = client
    .layer_add(
      brain,
      &NewLayer {
        name: "base".to_owned(),
        description: String::new(),
        position: Some(9),
      },
    )
    .await
    .unwrap();
  assert_eq!(again.id, base.id);
  let layers = client.layers(brain, &LayersQuery::default()).await.unwrap();
  assert_eq!(layers.items.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["base", "tone"]);
  let other = brain_of(client, "agent-b").await;
  assert_eq!(refused(client.subject(other, person.id).await), (404, "not-found".to_owned()));
}

#[tokio::test]
async fn a_memory_is_versioned_never_edited_and_its_key_moves_with_the_live_version() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let first = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("home".to_owned()),
        subjects: vec![subject("person", "1")],
        importance: Some(7),
        ..memory("fact", "The member lives in the north.")
      },
    )
    .await
    .unwrap();
  assert_eq!(first.lineage_id, first.id);
  assert_eq!(first.status, MemoryStatus::Active);
  assert_eq!(first.subjects.len(), 1);
  let taken = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("home".to_owned()),
        ..memory("fact", "Again")
      },
    )
    .await;
  assert_eq!(refused(taken), (409, "conflict".to_owned()));
  let second = client
    .memory_version(
      brain,
      first.id,
      &MemoryVersion {
        text: Some("The member lives in the south.".to_owned()),
        actor_name: "writer-b".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(second.lineage_id, first.id);
  assert_eq!(second.key.as_deref(), Some("home"));
  assert_eq!(second.importance, 7);
  assert_eq!(second.subjects, first.subjects);
  let old = client.memory(brain, first.id).await.unwrap();
  assert_eq!(old.status, MemoryStatus::Superseded);
  assert_eq!(old.superseded_by, Some(second.id));
  assert_eq!(old.valid_to, Some(second.valid_from));
  let over_old = client
    .memory_version(
      brain,
      first.id,
      &MemoryVersion {
        actor_name: "writer-b".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await;
  assert_eq!(refused(over_old), (409, "conflict".to_owned()));
  let lineage = client.lineage(brain, first.id).await.unwrap();
  assert_eq!(lineage.iter().map(|m| m.id).collect::<Vec<_>>(), [first.id, second.id]);
  let done = client
    .memory_move(
      brain,
      second.id,
      &MemoryMove {
        status: MemoryStatus::Invalidated,
        at: Some(5_000_000_000_000),
        actor_name: "writer-b".to_owned(),
      },
    )
    .await
    .unwrap();
  assert_eq!((done.status, done.valid_to), (MemoryStatus::Invalidated, Some(5_000_000_000_000)));
  let back = client
    .memory_move(
      brain,
      second.id,
      &MemoryMove {
        status: MemoryStatus::Active,
        at: None,
        actor_name: "writer-b".to_owned(),
      },
    )
    .await;
  assert_eq!(refused(back), (409, "conflict".to_owned()));
  let third = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("home".to_owned()),
        ..memory("fact", "The key is free again.")
      },
    )
    .await
    .unwrap();
  assert_eq!(third.lineage_id, third.id);
  let listed = client
    .memories(
      brain,
      &MemoriesQuery {
        key: Some("home".to_owned()),
        status: Some(MemoryStatus::Active),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(listed.items.iter().map(|m| m.id).collect::<Vec<_>>(), [third.id]);
  let pending = client
    .memory_add(
      brain,
      &NewMemory {
        status: Some(MemoryStatus::Pending),
        ..memory("fact", "Awaiting")
      },
    )
    .await
    .unwrap();
  let approved = client
    .memory_move(
      brain,
      pending.id,
      &MemoryMove {
        status: MemoryStatus::Active,
        at: None,
        actor_name: "owner".to_owned(),
      },
    )
    .await
    .unwrap();
  assert_eq!(approved.status, MemoryStatus::Active);
  let kinds = served.event_kinds().await;
  assert!(kinds.contains(&"memory.written".to_owned()) && kinds.contains(&"memory.moved".to_owned()));
}

#[tokio::test]
async fn hits_raise_retrieval_strength_and_spaced_confirmations_raise_storage_strength() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let row = client.memory_add(brain, &memory("fact", "A fact")).await.unwrap();
  assert_eq!((row.hits, row.retrieval_strength), (0, 0.0));
  let recorded = client
    .hits_record(
      brain,
      &Hits {
        memory_ids: vec![row.id, row.id, 999],
        session_key: Some("session-a".to_owned()),
        actor_name: "reader-a".to_owned(),
        at: None,
      },
    )
    .await
    .unwrap();
  assert_eq!(recorded.recorded_count, 1);
  let hit = client.memory(brain, row.id).await.unwrap();
  assert_eq!(hit.hits, 1);
  assert!(hit.last_hit_at.is_some() && hit.retrieval_strength > 0.0);
  let day = 86_400_000;
  let written = hit.created_at;
  let soon = client
    .confirmations_record(
      brain,
      &Confirmations {
        memory_ids: vec![row.id],
        actor_name: "writer-a".to_owned(),
        at: Some(written + 1_000),
      },
    )
    .await
    .unwrap();
  assert_eq!(soon.recorded_count, 1);
  let unmoved = client.memory(brain, row.id).await.unwrap();
  assert_eq!(unmoved.strength, 1.0);
  client
    .confirmations_record(
      brain,
      &Confirmations {
        memory_ids: vec![row.id],
        actor_name: "writer-a".to_owned(),
        at: Some(written + 10 * day),
      },
    )
    .await
    .unwrap();
  let stronger = client.memory(brain, row.id).await.unwrap();
  assert!(stronger.strength > 1.0, "{}", stronger.strength);
  assert_eq!(stronger.last_confirmed_at, Some(written + 10 * day));
}

#[tokio::test]
async fn the_types_in_use_are_counted_with_the_reserved_ones_first() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  client.memory_add(brain, &memory("fact", "A")).await.unwrap();
  client.memory_add(brain, &memory("tip", "B")).await.unwrap();
  client.memory_add(brain, &memory("tip", "C")).await.unwrap();
  let types = client.types(brain).await.unwrap();
  let fact = types.iter().find(|t| t.kind == "fact").unwrap();
  let tip = types.iter().find(|t| t.kind == "tip").unwrap();
  assert_eq!((fact.count, fact.reserved, tip.count, tip.reserved), (1, true, 2, false));
  assert!(
    types.iter().position(|t| t.kind == "episode").unwrap()
      < types.iter().position(|t| t.kind == "tip").unwrap()
  );
}

#[tokio::test]
async fn recall_hands_out_the_core_and_the_searched_part_marks_repeats_and_keeps_the_budget() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let room = subject("room", "7");
  let speaker = subject("person", "1");
  let rule = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![room.clone()],
        ..memory("situation", "When asked for the weather: answer in one line.")
      },
    )
    .await
    .unwrap();
  let sheet = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![speaker.clone()],
        ..memory("sheet", "The member keeps a garden.")
      },
    )
    .await
    .unwrap();
  let fact = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![speaker.clone()],
        importance: Some(8),
        ..memory("fact", "The garden grows tomatoes and beans.")
      },
    )
    .await
    .unwrap();
  let rumor = client
    .memory_add(brain, &memory("rumor", "The tomatoes were sold at the market."))
    .await
    .unwrap();
  let unrelated = client
    .memory_add(brain, &memory("fact", "Nothing to do with the question at all."))
    .await
    .unwrap();
  let pending = client
    .memory_add(
      brain,
      &NewMemory {
        status: Some(MemoryStatus::Pending),
        ..memory("fact", "Tomatoes await approval.")
      },
    )
    .await
    .unwrap();
  let ask = RecallAsk {
    room: Some(room.clone()),
    speakers: vec![speaker.clone()],
    words: "how are the tomatoes".to_owned(),
    session_key: Some("session-a".to_owned()),
    session_opened_at: None,
    budget_tokens: None,
    record: None,
  };
  let recall = client.recall(brain, &ask).await.unwrap();
  let chosen: Vec<(i64, RecallPart, bool)> =
    recall.chosen.iter().map(|c| (c.memory_id, c.part, c.marked)).collect();
  assert!(chosen.contains(&(rule.id, RecallPart::Core, false)), "{chosen:?}");
  assert!(chosen.contains(&(sheet.id, RecallPart::Core, false)), "{chosen:?}");
  assert!(chosen.contains(&(fact.id, RecallPart::Searched, false)), "{chosen:?}");
  assert!(chosen.contains(&(rumor.id, RecallPart::Searched, false)), "{chosen:?}");
  assert!(!recall.chosen.iter().any(|c| c.memory_id == pending.id), "a pending row stays out");
  let score_of = |id: i64| recall.chosen.iter().find(|c| c.memory_id == id).map(|c| c.score);
  if let Some(low) = score_of(unrelated.id) {
    assert!(score_of(fact.id).unwrap() > low, "{chosen:?}");
  }
  assert!(recall.block.contains("heard: The tomatoes were sold"));
  assert!(recall.block.contains(&format!("#{} situation since", rule.id)));
  assert_eq!(recall.token_count, recall.chosen.iter().map(|c| c.token_count).sum::<i64>());
  let again = client.recall(brain, &ask).await.unwrap();
  let marks: Vec<bool> = again.chosen.iter().map(|c| c.marked).collect();
  assert!(marks.iter().all(|m| *m), "{marks:?}");
  assert!(again.block.contains("handed to this session already"));
  let newer = client
    .memory_version(
      brain,
      fact.id,
      &MemoryVersion {
        text: Some("The garden grows only beans now.".to_owned()),
        actor_name: "writer-a".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  let third = client.recall(brain, &ask).await.unwrap();
  let fresh = third.chosen.iter().find(|c| c.memory_id == newer.id).unwrap();
  assert!(!fresh.marked);
  assert!(third.block.contains("(before, until"));
  let folded = client
    .fold(
      brain,
      &Fold {
        session_key: "session-a".to_owned(),
      },
    )
    .await
    .unwrap();
  assert!(folded.cleared_count >= 4, "{}", folded.cleared_count);
  let fourth = client.recall(brain, &ask).await.unwrap();
  assert!(fourth.chosen.iter().all(|c| !c.marked));
  let tight = client
    .recall(
      brain,
      &RecallAsk {
        budget_tokens: Some(12),
        session_key: None,
        ..ask.clone()
      },
    )
    .await
    .unwrap();
  assert!(tight.token_count <= 12, "{}", tight.token_count);
  let later = client
    .memory_add(brain, &memory("fact", "Tomatoes said after the session opened."))
    .await
    .unwrap();
  let narrowed = client
    .recall(
      brain,
      &RecallAsk {
        session_opened_at: Some(later.created_at - 1),
        session_key: None,
        ..ask.clone()
      },
    )
    .await
    .unwrap();
  assert!(!narrowed.chosen.iter().any(|c| c.memory_id == later.id));
}

#[tokio::test]
async fn the_persona_renders_in_layer_order_and_the_same_bytes_twice() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let tone = client
    .layer_add(
      brain,
      &NewLayer {
        name: "tone".to_owned(),
        description: String::new(),
        position: Some(1),
      },
    )
    .await
    .unwrap();
  let base = client
    .layer_add(
      brain,
      &NewLayer {
        name: "base".to_owned(),
        description: String::new(),
        position: Some(0),
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &NewMemory {
        layer_id: Some(tone.id),
        ..memory("persona", "Speaks plainly.")
      },
    )
    .await
    .unwrap();
  let character = client
    .memory_add(
      brain,
      &NewMemory {
        layer_id: Some(base.id),
        ..memory("persona", "An assistant.")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &NewMemory {
        layer_id: Some(base.id),
        ..memory("persona", "Named after a game console.\n")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &NewMemory {
        layer_id: Some(base.id),
        ..memory("lore", "Not a persona piece.")
      },
    )
    .await
    .unwrap();
  let text = client.persona(brain).await.unwrap();
  assert_eq!(text, "An assistant.\n\nNamed after a game console.\n\nSpeaks plainly.");
  assert_eq!(client.persona(brain).await.unwrap(), text);
  client
    .memory_version(
      brain,
      character.id,
      &MemoryVersion {
        text: Some("A helper.".to_owned()),
        actor_name: "owner".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(
    client.persona(brain).await.unwrap(),
    "A helper.\n\nNamed after a game console.\n\nSpeaks plainly."
  );
}

#[tokio::test]
async fn a_brain_file_is_exported_and_injected_without_overwriting() {
  let served = Served::new().await;
  let client = &served.client;
  let source = brain_of(client, "agent-a").await;
  let lore = client
    .layer_add(
      source,
      &NewLayer {
        name: "lore".to_owned(),
        description: "The world".to_owned(),
        position: None,
      },
    )
    .await
    .unwrap();
  let other = client
    .layer_add(
      source,
      &NewLayer {
        name: "other".to_owned(),
        description: String::new(),
        position: None,
      },
    )
    .await
    .unwrap();
  let place = client
    .memory_add(
      source,
      &NewMemory {
        key: Some("lore:lake".to_owned()),
        layer_id: Some(lore.id),
        subjects: vec![subject("place", "lake")],
        ..memory("lore", "The lake is wide.")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      source,
      &NewMemory {
        layer_id: Some(other.id),
        ..memory("fact", "Not exported by layer.")
      },
    )
    .await
    .unwrap();
  let file = client
    .export(
      source,
      &ExportQuery {
        layers: Some("lore".to_owned()),
      },
    )
    .await
    .unwrap();
  assert!(file.starts_with(b"SQLite format 3"));
  assert_eq!(
    refused(
      client
        .export(
          source,
          &ExportQuery {
            layers: Some("none".to_owned())
          }
        )
        .await
    ),
    (404, "not-found".to_owned())
  );

  let target = brain_of(client, "agent-b").await;
  let injected = client.inject(target, file.clone()).await.unwrap();
  assert_eq!(
    (
      injected.layers_added,
      injected.subjects_added,
      injected.memories_added,
      injected.memories_superseded
    ),
    (1, 1, 1, 0)
  );
  let landed = client.memories(target, &MemoriesQuery::default()).await.unwrap();
  assert_eq!(landed.items.len(), 1);
  assert_eq!(landed.items[0].key.as_deref(), Some("lore:lake"));
  assert_eq!(landed.items[0].subjects[0].key, "lake");
  let again = client.inject(target, file).await.unwrap();
  assert_eq!((again.memories_added, again.memories_superseded), (0, 0));

  client
    .memory_version(
      source,
      place.id,
      &MemoryVersion {
        text: Some("The lake is deep.".to_owned()),
        actor_name: "owner".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  let newer = client
    .export(
      source,
      &ExportQuery {
        layers: Some("lore".to_owned()),
      },
    )
    .await
    .unwrap();
  let superseding = client.inject(target, newer).await.unwrap();
  assert_eq!((superseding.memories_added, superseding.memories_superseded), (0, 1));
  let live = client
    .memories(
      target,
      &MemoriesQuery {
        status: Some(MemoryStatus::Active),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(live.items.len(), 1);
  assert_eq!(live.items[0].text, "The lake is deep.");
  let whole = client.export(source, &ExportQuery::default()).await.unwrap();
  assert!(!whole.is_empty());
  assert_eq!(
    refused(client.inject(target, b"not a file".to_vec()).await),
    (400, "bad-request".to_owned())
  );
}
