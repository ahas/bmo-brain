//! The brains and their three tables, through the served routes (docs/boundary-api.md, 「Brains」): a brain
//! made and found, subjects and layers, memories written, versioned and moved, hits and confirmations, the
//! types, recall, the fold, the persona and the brain file.

use crate::support;

use bmo_brain_client::{
  BrainClient, BrainsQuery, Confirmations, ExportQuery, Fold, Hits, LayersQuery, MemoriesQuery,
  MemoryMerge, MemoryMove, MemoryStatus, MemoryVersion, NewBrain, NewLayer, NewMemory, NewSubject,
  PersonaQuery, RecallAsk, RecallPart, SubjectKey, SubjectsQuery,
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
    recall_condition: None,
    release_condition: None,
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
async fn the_layer_page_walks_the_list_in_position_order_past_the_layer_last_seen() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let mut ids = vec![];
  for (name, position) in [("lore", 2), ("persona", 0), ("tone", 1)] {
    let layer = client
      .layer_add(
        brain,
        &NewLayer {
          name: name.to_owned(),
          description: String::new(),
          position: Some(position),
        },
      )
      .await
      .unwrap();
    ids.push(layer.id);
  }
  let mut walked = vec![];
  let mut after = None;
  loop {
    let page = client
      .layers(
        brain,
        &LayersQuery {
          after,
          limit: Some(1),
        },
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    walked.push(page.items[0].name.clone());
    after = Some(page.items[0].id);
    if !page.more {
      break;
    }
  }
  assert_eq!(walked, ["persona", "tone", "lore"]);
  assert!(ids[0] < ids[1], "the first layer added has the lowest id and the last position");
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
        reason: None,
        text: None,
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
        reason: None,
        text: None,
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
        status: Some("active".to_owned()),
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
        reason: None,
        text: None,
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
async fn the_types_in_use_count_their_live_rows_with_the_reserved_ones_first() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let closing = |status| MemoryMove {
    status,
    at: None,
    actor_name: "writer-a".to_owned(),
    reason: Some("closed".to_owned()),
    text: None,
  };
  client.memory_add(brain, &memory("fact", "A")).await.unwrap();
  let versioned = client.memory_add(brain, &memory("fact", "A2")).await.unwrap();
  client
    .memory_version(
      brain,
      versioned.id,
      &MemoryVersion {
        text: Some("A2, written again".to_owned()),
        actor_name: "writer-a".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  client.memory_add(brain, &memory("tip", "B")).await.unwrap();
  client.memory_add(brain, &memory("tip", "C")).await.unwrap();
  let done = client.memory_add(brain, &memory("tip", "D")).await.unwrap();
  client.memory_move(brain, done.id, &closing(MemoryStatus::Done)).await.unwrap();
  let gone = client.memory_add(brain, &memory("retired", "E")).await.unwrap();
  client.memory_move(brain, gone.id, &closing(MemoryStatus::Invalidated)).await.unwrap();
  let types = client.types(brain).await.unwrap();
  let fact = types.iter().find(|t| t.kind == "fact").unwrap();
  let tip = types.iter().find(|t| t.kind == "tip").unwrap();
  // The superseded version and the closed rows count for nothing, and a type whose rows are all closed is not in
  // use; a reserved type stands with no live row.
  assert_eq!((fact.count, fact.reserved, tip.count, tip.reserved), (2, true, 2, false));
  assert!(types.iter().all(|t| t.kind != "retired"), "{types:?}");
  assert_eq!(types.iter().find(|t| t.kind == "rumor").map(|t| t.count), Some(0));
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
        ..memory("fact", "The member keeps a garden.")
      },
    )
    .await
    .unwrap();
  let room_sheet = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![speaker.clone(), room.clone()],
        ..memory("fact", "In this room the member asks about the weather.")
      },
    )
    .await
    .unwrap();
  let elsewhere = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![speaker.clone(), subject("room", "8")],
        ..memory("fact", "In another room the member sells seedlings.")
      },
    )
    .await
    .unwrap();
  let fact = client
    .memory_add(
      brain,
      &NewMemory {
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
  assert!(chosen.contains(&(room_sheet.id, RecallPart::Core, false)), "{chosen:?}");
  assert!(
    !chosen.iter().any(|(id, part, _)| *id == elsewhere.id && *part == RecallPart::Core),
    "{chosen:?}"
  );
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
  let text = client.persona(brain, &PersonaQuery::default()).await.unwrap();
  assert_eq!(text, "An assistant.\n\nNamed after a game console.\n\nSpeaks plainly.");
  assert_eq!(client.persona(brain, &PersonaQuery::default()).await.unwrap(), text);
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
    client.persona(brain, &PersonaQuery::default()).await.unwrap(),
    "A helper.\n\nNamed after a game console.\n\nSpeaks plainly."
  );
  let base_only = PersonaQuery {
    layers: Some("base".to_owned()),
  };
  assert_eq!(
    client.persona(brain, &base_only).await.unwrap(),
    "A helper.\n\nNamed after a game console."
  );
  let both = PersonaQuery {
    layers: Some(" tone, base ".to_owned()),
  };
  assert_eq!(
    client.persona(brain, &both).await.unwrap(),
    "A helper.\n\nNamed after a game console.\n\nSpeaks plainly."
  );
  let missing = PersonaQuery {
    layers: Some("base,voice".to_owned()),
  };
  assert_eq!(refused(client.persona(brain, &missing).await), (404, "not-found".to_owned()));
}

#[tokio::test]
async fn a_key_may_hold_a_slash_and_an_injected_key_is_held_to_the_same_rule() {
  let served = Served::new().await;
  let client = &served.client;
  let source = brain_of(client, "agent-a").await;
  let piece = client
    .memory_add(
      source,
      &NewMemory {
        key: Some("lore/lake".to_owned()),
        ..memory("lore", "The lake is wide.")
      },
    )
    .await
    .unwrap();
  assert_eq!(piece.key.as_deref(), Some("lore/lake"));
  let bad = client
    .memory_add(
      source,
      &NewMemory {
        key: Some("lore lake".to_owned()),
        ..memory("lore", "The lake is wide.")
      },
    )
    .await;
  assert_eq!(refused(bad), (400, "bad-request".to_owned()));
  let file = client.export(source, &ExportQuery::default()).await.unwrap();
  let path = std::env::temp_dir().join(format!("brain-file-{}.sqlite", std::process::id()));
  std::fs::write(&path, &file).unwrap();
  {
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE memories SET key = 'lore lake'", []).unwrap();
  }
  let broken = std::fs::read(&path).unwrap();
  std::fs::remove_file(&path).ok();
  let target = brain_of(client, "agent-b").await;
  assert_eq!(refused(client.inject(target, broken).await), (400, "bad-request".to_owned()));
  let kept = client
    .memories(
      target,
      &MemoriesQuery {
        status: Some("active".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert!(kept.items.is_empty(), "nothing of a refused file is read in");
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
        status: Some("active".to_owned()),
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

#[tokio::test]
async fn a_merge_writes_one_row_over_several_and_supersedes_each() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let one = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![subject("person", "1")],
        importance: Some(4),
        ..memory("fact", "Likes the north.")
      },
    )
    .await
    .unwrap();
  let two = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![subject("person", "1"), subject("room", "7")],
        importance: Some(7),
        ..memory("fact", "Lives in the north.")
      },
    )
    .await
    .unwrap();
  client
    .hits_record(
      brain,
      &Hits {
        memory_ids: vec![two.id],
        actor_name: "reader-a".to_owned(),
        session_key: Some("s1".to_owned()),
        at: None,
      },
    )
    .await
    .unwrap();
  let merged = client
    .memory_merge(
      brain,
      &MemoryMerge {
        memory_ids: vec![one.id, two.id],
        kind: "fact".to_owned(),
        text: "Lives in the north and likes it.".to_owned(),
        key: Some("north".to_owned()),
        actor_name: "writer-b".to_owned(),
        ..MemoryMerge::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(merged.lineage_id, merged.id);
  assert_eq!(merged.status, MemoryStatus::Active);
  assert_eq!(merged.importance, 7);
  assert_eq!(merged.hits, 1);
  assert_eq!(merged.subjects.len(), 2);
  assert_eq!(merged.key.as_deref(), Some("north"));
  for id in [one.id, two.id] {
    let old = client.memory(brain, id).await.unwrap();
    assert_eq!(old.status, MemoryStatus::Superseded);
    assert_eq!(old.superseded_by, Some(merged.id));
    assert_eq!(old.valid_to, Some(merged.valid_from));
  }
  let third = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![subject("person", "1")],
        ..memory("fact", "Also north.")
      },
    )
    .await
    .unwrap();
  let under_held_key = client
    .memory_merge(
      brain,
      &MemoryMerge {
        memory_ids: vec![merged.id, third.id],
        kind: "fact".to_owned(),
        text: "Lives in the north, likes it, and says so.".to_owned(),
        key: Some("north".to_owned()),
        actor_name: "writer-b".to_owned(),
        ..MemoryMerge::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(under_held_key.key.as_deref(), Some("north"));
  assert_eq!(client.memory(brain, merged.id).await.unwrap().superseded_by, Some(under_held_key.id));
  let again = client
    .memory_merge(
      brain,
      &MemoryMerge {
        memory_ids: vec![one.id, under_held_key.id],
        kind: "fact".to_owned(),
        text: "Again".to_owned(),
        actor_name: "writer-b".to_owned(),
        ..MemoryMerge::default()
      },
    )
    .await;
  assert_eq!(refused(again), (409, "conflict".to_owned()));
  let few = client
    .memory_merge(
      brain,
      &MemoryMerge {
        memory_ids: vec![merged.id],
        kind: "fact".to_owned(),
        text: "Alone".to_owned(),
        actor_name: "writer-b".to_owned(),
        ..MemoryMerge::default()
      },
    )
    .await;
  assert_eq!(refused(few), (400, "bad-request".to_owned()));
}

#[tokio::test]
async fn recall_core_carries_no_rule_and_a_rule_is_found_by_search_when_its_words_match() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let room = subject("room", "7");
  let speaker = subject("person", "1");
  let mut rules = Vec::new();
  for n in 0..12 {
    let body = NewMemory {
      subjects: vec![room.clone()],
      importance: Some(9),
      ..memory("rule", &format!("Standing instruction number {n} for this room."))
    };
    rules.push(client.memory_add(brain, &body).await.unwrap().id);
  }
  let harvest = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![room.clone()],
        ..memory("rule", "Never quote harvest prices.")
      },
    )
    .await
    .unwrap()
    .id;
  rules.push(harvest);
  let situation = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![room.clone()],
        ..memory("situation", "When asked for the weather: answer in one line.")
      },
    )
    .await
    .unwrap()
    .id;
  let fact = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![speaker.clone()],
        ..memory("fact", "The member keeps a garden.")
      },
    )
    .await
    .unwrap()
    .id;
  let ask = |words: &str, budget: Option<i64>| RecallAsk {
    room: Some(room.clone()),
    speakers: vec![speaker.clone()],
    words: words.to_owned(),
    session_key: None,
    session_opened_at: None,
    budget_tokens: budget,
    record: Some(false),
  };
  let core_of = |recall: &bmo_brain_client::Recall| -> Vec<i64> {
    recall
      .chosen
      .iter()
      .filter(|c| c.part == RecallPart::Core)
      .map(|c| c.memory_id)
      .collect()
  };

  let neutral = client.recall(brain, &ask("good morning", None)).await.unwrap();
  assert_eq!(core_of(&neutral), [situation, fact]);
  assert!(neutral.token_count <= neutral.budget_tokens);
  assert_eq!(neutral.token_count, neutral.chosen.iter().map(|c| c.token_count).sum::<i64>());

  let asked = client.recall(brain, &ask("what are the harvest prices", None)).await.unwrap();
  assert!(!core_of(&asked).iter().any(|id| rules.contains(id)), "{:?}", asked.chosen);
  assert!(
    asked.chosen.iter().any(|c| c.memory_id == harvest && c.part == RecallPart::Searched),
    "a rule reaches the turn through the searched part when its words do: {:?}",
    asked.chosen
  );
  let tight = client.recall(brain, &ask("what are the harvest prices", Some(30))).await.unwrap();
  assert!(tight.token_count <= 30, "{} tokens", tight.token_count);
}

#[tokio::test]
async fn a_memory_carries_the_situation_that_recalls_it_and_the_one_that_ends_it() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let recall = "the member asks about the harvest";
  let release = "the harvest is sold";
  let written = client
    .memory_add(
      brain,
      &NewMemory {
        recall_condition: Some(recall.to_owned()),
        release_condition: Some(release.to_owned()),
        ..memory("fact", "The member's field yields barley.")
      },
    )
    .await
    .unwrap();
  let read = client.memory(brain, written.id).await.unwrap();
  assert_eq!(
    (read.recall_condition.as_deref(), read.release_condition.as_deref()),
    (Some(recall), Some(release))
  );

  // The list is narrowed by the words a situation holds against the recall condition.
  let found = |words: &str| {
    let query = MemoriesQuery {
      situation: Some(words.to_owned()),
      ..Default::default()
    };
    async move { client.memories(brain, &query).await.unwrap() }
  };
  assert_eq!(found("harvest").await.items.len(), 1);
  assert!(found("tomatoes").await.items.is_empty());

  // A version changes one and keeps the other where it gives none, and the version it supersedes keeps its own.
  let changed = client
    .memory_version(
      brain,
      written.id,
      &MemoryVersion {
        recall_condition: Some(Some("the member asks about barley".to_owned())),
        actor_name: "writer-b".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(
    (changed.recall_condition.as_deref(), changed.release_condition.as_deref()),
    (Some("the member asks about barley"), Some(release))
  );
  let superseded = client.memory(brain, written.id).await.unwrap();
  assert_eq!(
    (superseded.status, superseded.recall_condition.as_deref()),
    (MemoryStatus::Superseded, Some(recall))
  );

  // A version fills a condition a row carried none of, and the row it replaced stands as it was.
  let bare = client
    .memory_add(brain, &memory("fact", "The bins are collected on Tuesday."))
    .await
    .unwrap();
  let bare_id = bare.id;
  assert_eq!((bare.recall_condition, bare.release_condition), (None, None));
  let filled = client
    .memory_version(
      brain,
      bare_id,
      &MemoryVersion {
        recall_condition: Some(Some("the bins come up".to_owned())),
        release_condition: Some(Some("the bins are collected".to_owned())),
        actor_name: "writer-b".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(
    (filled.recall_condition.as_deref(), filled.release_condition.as_deref()),
    (Some("the bins come up"), Some("the bins are collected"))
  );
  let stood = client.memory(brain, bare_id).await.unwrap();
  assert_eq!(
    (stood.status, stood.recall_condition, stood.release_condition),
    (MemoryStatus::Superseded, None, None)
  );

  // A condition is cleared by a version that says null, and the other stands.
  let cleared = client
    .memory_version(
      brain,
      changed.id,
      &MemoryVersion {
        release_condition: Some(None),
        actor_name: "writer-b".to_owned(),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(
    (cleared.recall_condition.as_deref(), cleared.release_condition),
    (Some("the member asks about barley"), None)
  );

  // A condition is words: blank, or longer than a condition holds, is refused.
  let blank = client
    .memory_add(
      brain,
      &NewMemory {
        recall_condition: Some("  ".to_owned()),
        ..memory("fact", "Nothing at all.")
      },
    )
    .await;
  assert_eq!(refused(blank), (400, "bad-request".to_owned()));
  let long = client
    .memory_add(
      brain,
      &NewMemory {
        release_condition: Some("x".repeat(1001)),
        ..memory("fact", "Nothing at all.")
      },
    )
    .await;
  assert_eq!(refused(long), (400, "bad-request".to_owned()));
}

#[tokio::test]
async fn recall_brings_a_memory_out_by_its_situation_and_ends_the_one_a_turn_releases() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let room = subject("room", "7");
  let roof = client
    .memory_add(
      brain,
      &NewMemory {
        recall_condition: Some("asks about the roof".to_owned()),
        ..memory("fact", "Gutters cleared in spring.")
      },
    )
    .await
    .unwrap();
  let promise = client
    .memory_add(
      brain,
      &NewMemory {
        recall_condition: Some("the fence comes up in conversation".to_owned()),
        release_condition: Some("the back fence is painted".to_owned()),
        ..memory("promise", "Paint the back fence before the frost.")
      },
    )
    .await
    .unwrap();
  let plain = client
    .memory_add(brain, &memory("fact", "The back gate latches loosely."))
    .await
    .unwrap();
  let ask = |words: &str| RecallAsk {
    room: Some(room.clone()),
    speakers: vec![],
    words: words.to_owned(),
    session_key: None,
    session_opened_at: None,
    budget_tokens: None,
    record: Some(false),
  };
  let chosen =
    |recall: &bmo_brain_client::Recall| -> Vec<i64> { recall.chosen.iter().map(|c| c.memory_id).collect() };

  // The turn says the row's situation though its words never name the row: the condition brings it out.
  let asked = client.recall(brain, &ask("does anyone know about the roof")).await.unwrap();
  assert!(chosen(&asked).contains(&roof.id), "{:?}", asked.chosen);
  assert!(asked.released.is_empty(), "{:?}", asked.released);

  // A turn that says neither reaches neither.
  let quiet = client.recall(brain, &ask("what is for dinner tonight")).await.unwrap();
  assert!(!chosen(&quiet).contains(&roof.id), "{:?}", quiet.chosen);

  // The turn says the promise's release condition: the row is not handed out, and is ended with its reason.
  let ended = client.recall(brain, &ask("the back fence is painted at last")).await.unwrap();
  assert_eq!(ended.released, [promise.id]);
  assert!(!chosen(&ended).contains(&promise.id), "{:?}", ended.chosen);
  let closed = client.memory(brain, promise.id).await.unwrap();
  assert_eq!(
    (closed.status, closed.reason.as_deref()),
    (MemoryStatus::Invalidated, Some("the back fence is painted"))
  );
  assert!(closed.valid_to.is_some());
  // A row with no condition is reached by its words as it was.
  assert!(chosen(&ended).contains(&plain.id), "{:?}", ended.chosen);
  assert!(
    served.event_kinds().await.iter().any(|kind| kind == "memory.moved"),
    "the release is published"
  );
}
