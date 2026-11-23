//! Org brains, through the served routes (docs/design.md, 「Org brains」): a unit's brain keyed by the company
//! and the unit, holding a person only in a shared memory, its memories marked locked, internal-only and shared,
//! written by heads and by the person a shared memory is about and proposed by members, read along a member's
//! units as the topics that stand for them, and carried in the brain file.

use crate::support;

use bmo_brain_client::{
  Audience, BrainClient, BrainKind, BrainsQuery, ExportQuery, Learning, MemoryMerge, MemoryMove,
  MemoryStatus, MemoryVersion, NewBrain, NewMemory, NewSubject, OrgTopicsQuery, Restoration, Source,
  Standing, SubjectKey,
};
use support::{Served, refused};

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

/// A head's fact of a topic.
fn topic_fact(topic: &str, text: &str) -> NewMemory {
  NewMemory {
    kind: "fact".to_owned(),
    text: text.to_owned(),
    subjects: vec![subject("topic", topic)],
    actor_name: "writer-a".to_owned(),
    standing: Some(Standing::Head),
    ..NewMemory::default()
  }
}

async fn brain(client: &BrainClient, kind: BrainKind, account: &str, agent: &str) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: account.to_owned(),
      agent_key: agent.to_owned(),
      name: None,
      kind: Some(kind),
    })
    .await
    .unwrap()
    .id
}

fn topics_of(units: &[i64], audience: Option<Audience>) -> OrgTopicsQuery {
  OrgTopicsQuery {
    brain_ids: units.iter().map(i64::to_string).collect::<Vec<_>>().join(","),
    audience,
  }
}

#[tokio::test]
async fn an_org_brain_is_a_kind_of_its_own_at_a_pair_of_keys_no_agent_brain_holds() {
  let served = Served::new().await;
  let client = &served.client;
  let unit = brain(client, BrainKind::Org, "company-a", "unit-a").await;
  let agent = brain(client, BrainKind::Agent, "account-a", "agent-a").await;
  assert_eq!(client.brain(unit).await.unwrap().kind, BrainKind::Org);
  let made = client
    .brain_add(&NewBrain {
      account_key: "account-b".to_owned(),
      agent_key: "agent-b".to_owned(),
      ..NewBrain::default()
    })
    .await
    .unwrap();
  assert_eq!(made.kind, BrainKind::Agent);
  let twice = client
    .brain_add(&NewBrain {
      account_key: "company-a".to_owned(),
      agent_key: "unit-a".to_owned(),
      ..NewBrain::default()
    })
    .await;
  assert_eq!(refused(twice), (409, "conflict".to_owned()));
  let orgs = client
    .brains(&BrainsQuery {
      kind: Some(BrainKind::Org),
      ..BrainsQuery::default()
    })
    .await
    .unwrap();
  assert_eq!(orgs.items.iter().map(|brain| brain.id).collect::<Vec<_>>(), vec![unit]);
  let agents = client
    .brains(&BrainsQuery {
      kind: Some(BrainKind::Agent),
      ..BrainsQuery::default()
    })
    .await
    .unwrap();
  assert_eq!(agents.items.iter().map(|brain| brain.id).collect::<Vec<_>>(), vec![agent, made.id]);
}

#[tokio::test]
async fn an_org_brain_holds_a_person_in_a_shared_memory_alone_and_an_agent_brain_takes_no_mark() {
  let served = Served::new().await;
  let client = &served.client;
  let unit = brain(client, BrainKind::Org, "company-a", "unit-a").await;
  let agent = brain(client, BrainKind::Agent, "account-a", "agent-a").await;

  let about_a_person = NewMemory {
    subjects: vec![subject("topic", "industry"), subject("person", "1")],
    ..topic_fact("industry", "The member is new.")
  };
  assert_eq!(refused(client.memory_add(unit, &about_a_person).await), (400, "bad-request".to_owned()));
  let person = NewSubject {
    kind: "person".to_owned(),
    key: "1".to_owned(),
    description: String::new(),
  };
  client.subject_add(unit, &person).await.unwrap();
  client.subject_add(agent, &person).await.unwrap();
  let shared = client
    .memory_add(
      unit,
      &NewMemory {
        kind: "fact".to_owned(),
        text: "The member leads the site team.".to_owned(),
        subjects: vec![subject("person", "1")],
        shared: Some(true),
        actor_name: "writer-a".to_owned(),
        standing: Some(Standing::Head),
        ..NewMemory::default()
      },
    )
    .await
    .unwrap();
  assert_eq!((shared.shared, shared.internal_only, shared.kind.as_str()), (true, true, "fact"));
  // A version that takes the person away keeps the mark, and a shared memory about nobody is refused.
  let unshared = MemoryVersion {
    subjects: Some(vec![subject("topic", "industry")]),
    actor_name: "writer-a".to_owned(),
    standing: Some(Standing::Head),
    ..MemoryVersion::default()
  };
  assert_eq!(
    refused(client.memory_version(unit, shared.id, &unshared).await),
    (400, "bad-request".to_owned())
  );
  let about_nobody = NewMemory {
    shared: Some(true),
    ..topic_fact("industry", "The company builds.")
  };
  assert_eq!(refused(client.memory_add(unit, &about_nobody).await), (400, "bad-request".to_owned()));
  let shared_in_agent = NewMemory {
    subjects: vec![subject("person", "1")],
    shared: Some(true),
    ..topic_fact("industry", "The member leads the site team.")
  };
  assert_eq!(
    refused(client.memory_add(agent, &shared_in_agent).await),
    (400, "bad-request".to_owned())
  );

  let first = client.memory_add(unit, &topic_fact("industry", "The company builds.")).await.unwrap();
  assert_eq!((first.locked, first.internal_only), (false, true));
  let version = client
    .memory_version(
      unit,
      first.id,
      &MemoryVersion {
        text: Some("The company builds and designs.".to_owned()),
        locked: Some(true),
        actor_name: "writer-a".to_owned(),
        standing: Some(Standing::Head),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!((version.locked, version.internal_only), (true, true));
  let open = client
    .memory_add(
      unit,
      &NewMemory {
        internal_only: Some(false),
        shared: None,
        ..topic_fact("report-day", "Reports are due on Friday.")
      },
    )
    .await
    .unwrap();
  let merged = client
    .memory_merge(
      unit,
      &MemoryMerge {
        memory_ids: vec![version.id, open.id],
        kind: "fact".to_owned(),
        text: "The company builds, and reports on Friday.".to_owned(),
        actor_name: "writer-a".to_owned(),
        standing: Some(Standing::Head),
        ..MemoryMerge::default()
      },
    )
    .await
    .unwrap();
  assert_eq!((merged.locked, merged.internal_only), (true, true));

  let marked = NewMemory {
    locked: Some(true),
    ..topic_fact("industry", "The agent's own note.")
  };
  assert_eq!(refused(client.memory_add(agent, &marked).await), (400, "bad-request".to_owned()));
  let plain = client
    .memory_add(agent, &topic_fact("industry", "The agent's own note."))
    .await
    .unwrap();
  assert_eq!((plain.locked, plain.internal_only, plain.kind.as_str()), (false, false, "fact"));
}

#[tokio::test]
async fn the_person_a_shared_memory_is_about_writes_it_as_a_head_and_proposes_anything_else() {
  let served = Served::new().await;
  let client = &served.client;
  let unit = brain(client, BrainKind::Org, "company-a", "unit-a").await;
  let own = |text: &str| NewMemory {
    kind: "fact".to_owned(),
    text: text.to_owned(),
    subjects: vec![subject("person", "1")],
    shared: Some(true),
    actor_name: "person-1".to_owned(),
    standing: Some(Standing::Person),
    ..NewMemory::default()
  };
  let written = client.memory_add(unit, &own("The member prefers mail to calls.")).await.unwrap();
  assert_eq!((written.kind.as_str(), written.status), ("fact", MemoryStatus::Active));

  // Their own word on a matter of the unit, or on two people, is a proposal.
  let topic = NewMemory {
    standing: Some(Standing::Person),
    ..topic_fact("industry", "The company designs.")
  };
  assert_eq!(client.memory_add(unit, &topic).await.unwrap().kind, "rumor");
  let two = NewMemory {
    subjects: vec![subject("person", "1"), subject("person", "2")],
    ..own("The two members share a desk.")
  };
  assert_eq!(client.memory_add(unit, &two).await.unwrap().kind, "rumor");

  // They change and close their own shared memory; a version bringing in another person is refused.
  let version = client
    .memory_version(
      unit,
      written.id,
      &MemoryVersion {
        text: Some("The member prefers chat to calls.".to_owned()),
        actor_name: "person-1".to_owned(),
        standing: Some(Standing::Person),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  let widened = MemoryVersion {
    subjects: Some(vec![subject("person", "1"), subject("person", "2")]),
    actor_name: "person-1".to_owned(),
    standing: Some(Standing::Person),
    ..MemoryVersion::default()
  };
  assert_eq!(
    refused(client.memory_version(unit, version.id, &widened).await),
    (403, "forbidden".to_owned())
  );
  let moved = client
    .memory_move(
      unit,
      version.id,
      &MemoryMove {
        status: MemoryStatus::Invalidated,
        at: None,
        actor_name: "person-1".to_owned(),
        reason: None,
        text: None,
        standing: Some(Standing::Person),
      },
    )
    .await
    .unwrap();
  assert_eq!(moved.status, MemoryStatus::Invalidated);
  let head = client.memory_add(unit, &topic_fact("industry", "The company builds.")).await.unwrap();
  let theirs = MemoryMove {
    status: MemoryStatus::Invalidated,
    at: None,
    actor_name: "person-1".to_owned(),
    reason: None,
    text: None,
    standing: Some(Standing::Person),
  };
  assert_eq!(refused(client.memory_move(unit, head.id, &theirs).await), (403, "forbidden".to_owned()));

  // A learning of their own about themselves is their word too.
  let learned = client
    .learn(
      unit,
      &Learning {
        kind: "fact".to_owned(),
        text: "The member works from the site office.".to_owned(),
        subjects: vec![subject("person", "1")],
        importance: None,
        source: Source::Own,
        confirms: vec![],
        strengthens: None,
        replaces: vec![],
        grounds: vec![],
        valid_from: None,
        recall_condition: None,
        release_condition: None,
        key: None,
        layer_id: None,
        actor_name: "person-1".to_owned(),
        labels: Default::default(),
        locked: None,
        internal_only: None,
        shared: Some(true),
        standing: Some(Standing::Person),
      },
    )
    .await
    .unwrap();
  assert_eq!(learned.written.unwrap().kind, "fact");
}

#[tokio::test]
async fn a_members_word_is_a_proposal_a_head_accepts_and_every_other_write_is_a_heads() {
  let served = Served::new().await;
  let client = &served.client;
  let unit = brain(client, BrainKind::Org, "company-a", "unit-a").await;
  let hours = client
    .memory_add(unit, &topic_fact("working-hours", "Work runs 08:30 to 17:30."))
    .await
    .unwrap();

  let proposal = client
    .memory_add(
      unit,
      &NewMemory {
        standing: None,
        ..topic_fact("working-hours", "Work runs 07:00 to 16:00.")
      },
    )
    .await
    .unwrap();
  assert_eq!(proposal.kind, "rumor");
  let learned = client
    .learn(
      unit,
      &Learning {
        kind: "fact".to_owned(),
        text: "Work runs 09:00 to 18:00.".to_owned(),
        subjects: vec![subject("topic", "working-hours")],
        importance: None,
        source: Source::Owner,
        confirms: vec![],
        strengthens: None,
        replaces: vec![hours.id],
        grounds: vec![],
        valid_from: None,
        recall_condition: None,
        release_condition: None,
        key: None,
        layer_id: None,
        actor_name: "agent-session".to_owned(),
        labels: Default::default(),
        locked: None,
        internal_only: None,
        shared: None,
        standing: Some(Standing::Member),
      },
    )
    .await
    .unwrap();
  assert_eq!(learned.written.unwrap().kind, "rumor");
  assert!(learned.invalidated_ids.is_empty());
  assert_eq!(client.memory(unit, hours.id).await.unwrap().status, MemoryStatus::Active);

  let forbidden = (403, "forbidden".to_owned());
  let strengthening = Learning {
    kind: "fact".to_owned(),
    text: "Work runs 08:30 to 17:30 on weekdays.".to_owned(),
    subjects: vec![],
    importance: None,
    source: Source::Owner,
    confirms: vec![],
    strengthens: Some(hours.id),
    replaces: vec![],
    grounds: vec![],
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    key: None,
    layer_id: None,
    actor_name: "agent-session".to_owned(),
    labels: Default::default(),
    locked: None,
    internal_only: None,
    shared: None,
    standing: None,
  };
  assert_eq!(refused(client.learn(unit, &strengthening).await), forbidden);
  let version = MemoryVersion {
    text: Some("Work runs 07:30 to 16:30.".to_owned()),
    actor_name: "writer-b".to_owned(),
    ..MemoryVersion::default()
  };
  assert_eq!(refused(client.memory_version(unit, hours.id, &version).await), forbidden);
  let invalidated = MemoryMove {
    status: MemoryStatus::Invalidated,
    at: None,
    actor_name: "writer-b".to_owned(),
    reason: None,
    text: None,
    standing: None,
  };
  assert_eq!(refused(client.memory_move(unit, hours.id, &invalidated).await), forbidden);
  let merge = MemoryMerge {
    memory_ids: vec![hours.id, proposal.id],
    kind: "fact".to_owned(),
    text: "Work runs 08:00.".to_owned(),
    actor_name: "writer-b".to_owned(),
    ..MemoryMerge::default()
  };
  assert_eq!(refused(client.memory_merge(unit, &merge).await), forbidden);
  let closed = client
    .memory_move(
      unit,
      hours.id,
      &MemoryMove {
        standing: Some(Standing::Head),
        ..invalidated.clone()
      },
    )
    .await
    .unwrap();
  let restoration = Restoration {
    text: None,
    actor_name: "writer-b".to_owned(),
    standing: Some(Standing::Member),
  };
  assert_eq!(refused(client.memory_restore(unit, closed.id, &restoration).await), forbidden);

  let accepted = client
    .memory_version(
      unit,
      proposal.id,
      &MemoryVersion {
        kind: Some("fact".to_owned()),
        actor_name: "writer-a".to_owned(),
        standing: Some(Standing::Head),
        ..MemoryVersion::default()
      },
    )
    .await
    .unwrap();
  assert_eq!((accepted.kind.as_str(), accepted.lineage_id), ("fact", proposal.id));
}

#[tokio::test]
async fn a_topic_stands_from_the_nearest_unit_unless_a_farther_one_locked_it() {
  let served = Served::new().await;
  let client = &served.client;
  let company = brain(client, BrainKind::Org, "company-a", "company").await;
  let team = brain(client, BrainKind::Org, "company-a", "team-a").await;
  let part = brain(client, BrainKind::Org, "company-a", "part-a").await;
  let second_post = brain(client, BrainKind::Org, "company-a", "team-b").await;

  let company_hours = client
    .memory_add(company, &topic_fact("working-hours", "08:30 to 17:30."))
    .await
    .unwrap();
  let team_hours = client.memory_add(team, &topic_fact("working-hours", "07:00 to 16:00.")).await.unwrap();
  let policy = client
    .memory_add(
      company,
      &NewMemory {
        locked: Some(true),
        internal_only: Some(false),
        shared: None,
        ..topic_fact("photos", "Site photos go to the company drive alone.")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(part, &topic_fact("photos", "Site photos go to the part's folder."))
    .await
    .unwrap();
  let report = client
    .memory_add(
      second_post,
      &NewMemory {
        internal_only: Some(false),
        shared: None,
        ..topic_fact("report-day", "Reports are due on Friday.")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(second_post, &topic_fact("working-hours", "10:00 to 19:00."))
    .await
    .unwrap();
  client
    .memory_add(
      part,
      &NewMemory {
        standing: Some(Standing::Member),
        ..topic_fact("working-hours", "06:00 to 15:00.")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      part,
      &NewMemory {
        subjects: vec![],
        ..topic_fact("unused", "The part meets on Monday.")
      },
    )
    .await
    .unwrap();

  let path = [second_post, company, team, part];
  let standing = client.org_topics(&topics_of(&path, None)).await.unwrap();
  let read: Vec<(&str, i64, bool, Vec<i64>)> = standing
    .iter()
    .map(|topic| {
      let ids = topic.memories.iter().map(|memory| memory.id).collect();
      (topic.topic.as_str(), topic.brain_id, topic.locked, ids)
    })
    .collect();
  assert_eq!(
    read,
    vec![
      ("report-day", second_post, false, vec![report.id]),
      ("photos", company, true, vec![policy.id]),
      ("working-hours", team, false, vec![team_hours.id]),
    ]
  );

  let outside = client.org_topics(&topics_of(&path, Some(Audience::External))).await.unwrap();
  let kept: Vec<(&str, i64)> = outside.iter().map(|topic| (topic.topic.as_str(), topic.brain_id)).collect();
  assert_eq!(kept, vec![("report-day", second_post), ("photos", company)]);

  let company_only = client.org_topics(&topics_of(&[company], None)).await.unwrap();
  assert_eq!(company_only[1].memories[0].id, company_hours.id);

  let agent = brain(client, BrainKind::Agent, "account-a", "agent-a").await;
  let bad = (400, "bad-request".to_owned());
  assert_eq!(refused(client.org_topics(&topics_of(&[company, agent], None)).await), bad);
  assert_eq!(refused(client.org_topics(&topics_of(&[company, company], None)).await), bad);
  assert_eq!(refused(client.org_topics(&topics_of(&[], None)).await), bad);
  assert_eq!(
    refused(client.org_topics(&topics_of(&[company, 999_999], None)).await),
    (404, "not-found".to_owned())
  );
}

#[tokio::test]
async fn a_brain_file_carries_the_marks_into_an_org_brain_and_a_person_in_a_shared_memory_alone() {
  let served = Served::new().await;
  let client = &served.client;
  let source = brain(client, BrainKind::Org, "company-a", "unit-a").await;
  let target = brain(client, BrainKind::Org, "company-a", "unit-b").await;
  let agent = brain(client, BrainKind::Agent, "account-a", "agent-a").await;
  client
    .memory_add(
      source,
      &NewMemory {
        locked: Some(true),
        internal_only: Some(false),
        shared: None,
        ..topic_fact("photos", "Site photos go to the company drive alone.")
      },
    )
    .await
    .unwrap();
  let file = client.export(source, &ExportQuery::default()).await.unwrap();

  client.inject(target, file.clone()).await.unwrap();
  let carried = client.org_topics(&topics_of(&[target], None)).await.unwrap();
  let marks = carried[0]
    .memories
    .iter()
    .map(|memory| (memory.locked, memory.internal_only))
    .collect::<Vec<_>>();
  assert_eq!(marks, vec![(true, false)]);

  client.inject(agent, file).await.unwrap();
  let left_off = client
    .memories(agent, &bmo_brain_client::MemoriesQuery::default())
    .await
    .unwrap()
    .items
    .iter()
    .map(|memory| (memory.locked, memory.internal_only))
    .collect::<Vec<_>>();
  assert_eq!(left_off, vec![(false, false)]);

  client
    .memory_add(
      agent,
      &NewMemory {
        subjects: vec![subject("person", "1")],
        ..topic_fact("industry", "The member is new.")
      },
    )
    .await
    .unwrap();
  let with_a_person = client.export(agent, &ExportQuery::default()).await.unwrap();
  assert_eq!(refused(client.inject(target, with_a_person).await), (400, "bad-request".to_owned()));

  client
    .memory_add(
      source,
      &NewMemory {
        kind: "fact".to_owned(),
        text: "The member leads the site team.".to_owned(),
        subjects: vec![subject("person", "1")],
        shared: Some(true),
        actor_name: "writer-a".to_owned(),
        standing: Some(Standing::Head),
        ..NewMemory::default()
      },
    )
    .await
    .unwrap();
  let shared = client.export(source, &ExportQuery::default()).await.unwrap();
  let other = brain(client, BrainKind::Org, "company-a", "unit-c").await;
  client.inject(other, shared).await.unwrap();
  let carried = client
    .memories(other, &bmo_brain_client::MemoriesQuery::default())
    .await
    .unwrap()
    .items
    .into_iter()
    .filter(|memory| memory.subjects.iter().any(|subject| subject.kind == "person"))
    .map(|memory| memory.shared)
    .collect::<Vec<_>>();
  assert_eq!(carried, vec![true]);
}
