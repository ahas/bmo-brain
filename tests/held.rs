//! What a holder leaves (docs/design.md, 「What a holder leaves」; docs/boundary-api.md, 「Declarations and
//! holders」): a declaration stands its set in every agent's brain and repeats as nothing, no write changes or
//! closes a row of it, every turn is handed it and no turn ends it, a brain made later is given it, a
//! withdrawal of the key closes it and nothing else, and no brain file carries it.

use crate::support;

use bmo_brain_client::{
  BrainClient, BrainKind, Declaration, DeclaredMemory, Holder, HolderReach, HoldersQuery, MemoriesQuery,
  MemoryVersion, NewBrain, NewMemory, RecallAsk, Restoration, SubjectKey,
};
use bmo_brain_client::{MemoryMerge, MemoryMove, MemoryStatus};
use support::{CONSOLE_TOKEN, Served, labels, refused};

/// A row of one holder's set, as a declaration carries it.
fn declared(key: &str, text: &str) -> DeclaredMemory {
  DeclaredMemory {
    key: key.to_owned(),
    kind: "situation".to_owned(),
    text: text.to_owned(),
    actor_name: None,
    subjects: vec![SubjectKey {
      kind: "application".to_owned(),
      key: "holder-a".to_owned(),
    }],
    importance: Some(8),
    valid_from: None,
    recall_condition: Some("a turn asks about plans".to_owned()),
    release_condition: None,
    labels: labels(&[]),
    triggers: vec![],
    every_turn: None,
  }
}

async fn declare(
  client: &BrainClient,
  holder: &str,
  memories: Vec<DeclaredMemory>,
) -> bmo_brain_client::Declared {
  client
    .declare(&Declaration {
      within: None,
      held_by: holder.to_owned(),
      memories,
    })
    .await
    .unwrap()
}

/// A status move naming nothing but the status: what a caller asks when it would close a row.
fn moved_to(status: MemoryStatus) -> MemoryMove {
  MemoryMove {
    status,
    at: None,
    actor_name: "writer-a".to_owned(),
    reason: None,
    text: None,
    standing: None,
  }
}

/// A learning of an agent's own hand, its words and kind named and every judgement left to the call that
/// takes it.
fn learning(kind: &str, text: &str) -> bmo_brain_client::Learning {
  bmo_brain_client::Learning {
    kind: kind.to_owned(),
    text: text.to_owned(),
    subjects: vec![],
    importance: None,
    source: bmo_brain_client::Source::Own,
    confirms: vec![],
    strengthens: None,
    replaces: vec![],
    grounds: vec![],
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    key: None,
    layer_id: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
    locked: None,
    internal_only: None,
    shared: None,
    standing: None,
  }
}

async fn agent_brain(client: &BrainClient, account: &str, agent: &str) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: account.to_owned(),
      agent_key: agent.to_owned(),
      name: None,
      kind: Some(BrainKind::Agent),
    })
    .await
    .unwrap()
    .id
}

/// What stands for a holder in one brain: its live rows, by the holder's key and then the key the holder
/// gave each. The list itself stands in no such order — `GET /memories` keeps its own order and names no
/// status unless the caller does — so the read names the live statuses and puts them in the holder's order.
async fn held_of(client: &BrainClient, brain_id: i64) -> Vec<(Option<String>, String, String)> {
  let page = client
    .memories(
      brain_id,
      &MemoriesQuery {
        held: Some(true),
        status: Some("active,pending".to_owned()),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  let mut rows: Vec<(Option<String>, String, String)> = page
    .items
    .into_iter()
    .map(|row| (row.held_by, row.key.unwrap_or_default(), row.text))
    .collect();
  rows.sort_by(|first, second| first.0.cmp(&second.0).then_with(|| first.1.cmp(&second.1)));
  rows
}

async fn own_of(client: &BrainClient, brain_id: i64) -> Vec<String> {
  let page = client
    .memories(
      brain_id,
      &MemoriesQuery {
        held: Some(false),
        status: Some("active,pending".to_owned()),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  page.items.into_iter().map(|row| row.text).collect()
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

#[tokio::test]
async fn a_declaration_stands_its_set_in_every_agents_brain_and_repeating_it_is_nothing() {
  let served = Served::new().await;
  let client = &served.client;
  let first = agent_brain(client, "account-a", "agent-a").await;
  let second = agent_brain(client, "account-a", "agent-b").await;

  let set = vec![
    declared("routes-plans", "plans go to the holder named by the subject, never elsewhere."),
    declared("manual", "how the holder is used, in full."),
  ];
  let stood = declare(client, "holder-a", set.clone()).await;
  assert_eq!(stood.brains, 2);
  assert_eq!(stood.written, 4);
  assert_eq!(stood.closed, 0);
  assert_eq!(stood.memories, 4);

  for brain in [first, second] {
    let rows = held_of(client, brain).await;
    assert_eq!(
      rows,
      vec![
        (
          Some("holder-a".to_owned()),
          "manual".to_owned(),
          "how the holder is used, in full.".to_owned()
        ),
        (
          Some("holder-a".to_owned()),
          "routes-plans".to_owned(),
          "plans go to the holder named by the subject, never elsewhere.".to_owned()
        ),
      ],
      "brain {brain} holds the set"
    );
    assert!(own_of(client, brain).await.is_empty(), "the brain's own rows stay none");
  }

  // The same declaration over a set that stands as declared writes nothing and closes nothing.
  let again = client
    .declare(&Declaration {
      within: None,
      held_by: "holder-a".to_owned(),
      memories: set,
    })
    .await;
  assert!(again.is_ok(), "the repeat answers the standing set");

  // The holders that stand are listed by key.
  let holders = client.holders(&HoldersQuery::default()).await.unwrap();
  assert_eq!(
    holders.items,
    vec![Holder {
      held_by: "holder-a".to_owned(),
      brains: 2,
      memories: 4
    }]
  );
}

#[tokio::test]
async fn a_declaration_that_rewrites_one_row_closes_the_row_it_stands_for() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = agent_brain(client, "account-a", "agent-a").await;
  declare(client, "holder-a", vec![declared("routes-plans", "the first words.")]).await;

  let changed = declared("routes-plans", "the words after.");
  let stood = declare(client, "holder-a", vec![changed]).await;
  assert_eq!((stood.written, stood.closed), (1, 1));

  let rows = held_of(client, brain).await;
  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0].2, "the words after.");

  // The closed row stands in the history, invalidated for the reason a rewrite records.
  let past = client
    .memories(
      brain,
      &MemoriesQuery {
        status: Some("invalidated".to_owned()),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(past.items.len(), 1);
  assert_eq!(past.items[0].reason.as_deref(), Some("holder-replaced"));
  assert_eq!(past.items[0].held_by.as_deref(), Some("holder-a"));
}

#[tokio::test]
async fn nothing_a_caller_may_write_changes_or_closes_a_held_row() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = agent_brain(client, "account-a", "agent-a").await;
  declare(
    client,
    "holder-a",
    vec![declared("first", "the first words."), declared("second", "the second.")],
  )
  .await;
  let rows = client
    .memories(
      brain,
      &MemoriesQuery {
        held: Some(true),
        ..Default::default()
      },
    )
    .await
    .unwrap()
    .items;
  let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();

  let version = refused(
    client
      .memory_version(
        brain,
        ids[0],
        &MemoryVersion {
          text: Some("overwritten".to_owned()),
          actor_name: "writer-a".to_owned(),
          ..Default::default()
        },
      )
      .await,
  );
  assert_eq!(version, (409, "conflict".to_owned()));

  let merge = refused(
    client
      .memory_merge(
        brain,
        &MemoryMerge {
          memory_ids: ids.clone(),
          kind: "fact".to_owned(),
          text: "merged".to_owned(),
          actor_name: "writer-a".to_owned(),
          ..Default::default()
        },
      )
      .await,
  );
  assert_eq!(merge, (409, "conflict".to_owned()));

  let move_it = refused(client.memory_move(brain, ids[0], &moved_to(MemoryStatus::Invalidated)).await);
  assert_eq!(move_it, (409, "conflict".to_owned()));

  // A learning that would version a held row, and one that would close it, are both refused.
  let strengthens = refused(
    client
      .learn(
        brain,
        &bmo_brain_client::Learning {
          strengthens: Some(ids[0]),
          ..learning("fact", "a fuller saying")
        },
      )
      .await,
  );
  assert_eq!(strengthens, (409, "conflict".to_owned()));

  let replaces = refused(
    client
      .learn(
        brain,
        &bmo_brain_client::Learning {
          replaces: vec![ids[0]],
          ..learning("fact", "another saying")
        },
      )
      .await,
  );
  assert_eq!(replaces, (409, "conflict".to_owned()));

  // The refusals name whose row it is, so the caller is told who may end it.
  let answer = client
    .memory_version(
      brain,
      ids[0],
      &MemoryVersion {
        text: Some("overwritten".to_owned()),
        actor_name: "writer-a".to_owned(),
        ..Default::default()
      },
    )
    .await;
  let Err(bmo_brain_client::ClientFailure::Answered {
    answer: Some(answer),
    ..
  }) = answer
  else {
    panic!("the version was due to be refused");
  };
  assert!(answer.error.contains("holder-a"), "the sentence names the holder: {}", answer.error);

  // And every held row still stands, whole.
  assert_eq!(held_of(client, brain).await.len(), 2);
}

#[tokio::test]
async fn every_turn_is_handed_a_held_row_and_no_turn_ends_it() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = agent_brain(client, "account-a", "agent-a").await;
  let mut rule = declared("routes-plans", "plans go to the holder, never elsewhere.");
  rule.release_condition = Some("the holder is gone".to_owned());
  declare(client, "holder-a", vec![rule]).await;

  // A turn that says none of the row's words is handed it: it stands in the core.
  let turn = client.recall(brain, &ask("unrelated weather report")).await.unwrap();
  let held: Vec<i64> = turn
    .chosen
    .iter()
    .filter(|choice| choice.part == bmo_brain_client::RecallPart::Core)
    .map(|choice| choice.memory_id)
    .collect();
  assert_eq!(held.len(), 1, "the held row stands in the core");
  assert!(turn.released.is_empty(), "nothing was ended");

  // A turn that says the row's own release condition neither ends it nor leaves it out.
  let said = client.recall(brain, &ask("the holder is gone now")).await.unwrap();
  assert!(said.released.is_empty(), "a turn never withdraws what a holder left");
  assert_eq!(said.chosen.len(), 1, "the row is still handed out");

  // A withdrawal of the key ends it, and the next turn holds nothing of it.
  let closed = client.holder_withdraw("holder-a").await.unwrap();
  assert_eq!((closed.closed.brains, closed.closed.memories), (1, 1));
  let after = client.recall(brain, &ask("unrelated weather report")).await.unwrap();
  assert!(after.chosen.is_empty());
}

#[tokio::test]
async fn a_held_row_off_every_turn_comes_by_its_trigger_or_its_situation_and_is_fetched_by_its_key() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = agent_brain(client, "account-a", "agent-a").await;
  let mut when = declared("holder-a.when", "reach for the holder's tools when plans are due.");
  when.recall_condition = None;
  let mut manual =
    declared("holder-a.manual", "call holder/plan_put with the plan's words, one plan a call.");
  manual.every_turn = Some(false);
  manual.triggers = vec!["holder/plan_put".to_owned()];
  manual.recall_condition = Some("filing a garden plan".to_owned());
  declare(client, "holder-a", vec![when, manual]).await;
  let core = |recall: &bmo_brain_client::Recall| -> Vec<i64> {
    recall
      .chosen
      .iter()
      .filter(|choice| choice.part == bmo_brain_client::RecallPart::Core)
      .map(|choice| choice.memory_id)
      .collect()
  };

  // A turn meeting no trigger and saying nothing of the row is handed the every-turn row alone.
  let plain = client.recall(brain, &ask("unrelated weather report")).await.unwrap();
  assert_eq!(plain.chosen.len(), 1, "{:?}", plain.chosen);

  // A turn naming the row's trigger stands it in the core whatever its words say.
  let mut keyed = ask("unrelated weather report");
  keyed.triggers = vec!["holder/plan_put".to_owned()];
  let keyed = client.recall(brain, &keyed).await.unwrap();
  assert_eq!(core(&keyed).len(), 2, "{:?}", keyed.chosen);

  // A turn saying the row's situation reaches it through the searched part.
  let said = client.recall(brain, &ask("we are filing a garden plan today")).await.unwrap();
  assert!(
    said.chosen.iter().any(|choice| choice.part == bmo_brain_client::RecallPart::Searched),
    "{:?}",
    said.chosen
  );

  // A list by the trigger fetches the row exactly.
  let fetched = client
    .memories(
      brain,
      &MemoriesQuery {
        trigger: Some("holder/plan_put".to_owned()),
        record: Some(false),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  let keys: Vec<Option<String>> = fetched.items.iter().map(|memory| memory.key.clone()).collect();
  assert_eq!(keys, vec![Some("holder-a.manual".to_owned())]);
  assert_eq!(fetched.items[0].triggers, vec!["holder/plan_put".to_owned()]);
  assert!(!fetched.items[0].every_turn);

  // A row off every turn naming neither a trigger nor a situation could never come out, and is refused.
  let mut lost = declared("holder-a.lost", "nothing brings this out.");
  lost.every_turn = Some(false);
  lost.recall_condition = None;
  let refusal = client
    .declare(&Declaration {
      within: None,
      held_by: "holder-a".to_owned(),
      memories: vec![lost],
    })
    .await;
  assert_eq!(refused(refusal).0, 400);
}

#[tokio::test]
async fn a_brain_made_after_a_declaration_is_given_what_stands() {
  let served = Served::new().await;
  let client = &served.client;
  agent_brain(client, "account-a", "agent-a").await;
  declare(client, "holder-a", vec![declared("manual", "how the holder is used.")]).await;

  let late = agent_brain(client, "account-b", "agent-b").await;
  assert_eq!(
    held_of(client, late).await,
    vec![(Some("holder-a".to_owned()), "manual".to_owned(), "how the holder is used.".to_owned())]
  );

  // An org brain is given nothing: a holder's words stand in agents' brains.
  let org = client
    .brain_add(&NewBrain {
      account_key: "company-a".to_owned(),
      agent_key: "unit-a".to_owned(),
      name: None,
      kind: Some(BrainKind::Org),
    })
    .await
    .unwrap()
    .id;
  assert!(held_of(client, org).await.is_empty());
}

#[tokio::test]
async fn a_withdrawal_closes_every_row_of_its_key_and_touches_nothing_else() {
  let served = Served::new().await;
  let client = &served.client;
  let first = agent_brain(client, "account-a", "agent-a").await;
  let second = agent_brain(client, "account-a", "agent-b").await;
  declare(client, "holder-a", vec![declared("first", "the first words.")]).await;
  declare(client, "holder-b", vec![declared("other", "another holder's words.")]).await;
  client
    .memory_add(
      first,
      &NewMemory {
        key: None,
        kind: "fact".to_owned(),
        text: "the brain's own row".to_owned(),
        actor_name: "writer-a".to_owned(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

  let counts = client.holder_removal("holder-a").await.unwrap();
  assert_eq!((counts.counts.brains, counts.counts.memories), (2, 2), "the count changes nothing");
  assert_eq!(client.holders(&HoldersQuery::default()).await.unwrap().items.len(), 2);

  let closed = client.holder_withdraw("holder-a").await.unwrap();
  assert_eq!((closed.closed.brains, closed.closed.memories), (2, 2));

  // What was withdrawn is closed for its reason in every brain, and nothing else moved: the other holder's
  // row stands where it stood, since a withdrawal ends its own key alone.
  for brain in [first, second] {
    let left: Vec<_> = held_of(client, brain)
      .await
      .into_iter()
      .filter(|row| row.0.as_deref() == Some("holder-a"))
      .collect();
    assert!(left.is_empty(), "every row of the withdrawn key is closed: {left:?}");
    assert_eq!(
      held_of(client, brain)
        .await
        .iter()
        .filter(|row| row.0.as_deref() == Some("holder-b"))
        .count(),
      1,
      "the other holder's row stands untouched"
    );
    let past = client
      .memories(
        brain,
        &MemoriesQuery {
          status: Some("invalidated".to_owned()),
          ..Default::default()
        },
      )
      .await
      .unwrap();
    assert_eq!(past.items[0].reason.as_deref(), Some("holder-withdrawn"));
  }
  assert_eq!(
    held_of(client, first)
      .await
      .iter()
      .filter(|row| row.0.as_deref() == Some("holder-a"))
      .count(),
    0,
    "nothing of the withdrawn key is left standing"
  );
  assert_eq!(own_of(client, first).await, vec!["the brain's own row".to_owned()]);
  let left = client
    .memories(
      second,
      &MemoriesQuery {
        held: Some(true),
        status: Some("active,pending".to_owned()),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(
    left.items.iter().map(|row| row.held_by.clone()).collect::<Vec<_>>(),
    vec![Some("holder-b".to_owned())],
    "another holder's live row stands, and the withdrawn one is not among the live reads"
  );

  // A closed held row is not restored, and the withdrawal is the same again where nothing stands.
  let restored = refused(
    client
      .memory_restore(
        first,
        past_of(client, first).await,
        &Restoration {
          actor_name: "writer-a".to_owned(),
          ..Default::default()
        },
      )
      .await,
  );
  assert_eq!(restored, (409, "conflict".to_owned()));

  let again = client.holder_withdraw("holder-a").await.unwrap();
  assert_eq!((again.closed.brains, again.closed.memories), (0, 0));
}

/// The one invalidated row of a brain, by its id.
async fn past_of(client: &BrainClient, brain_id: i64) -> i64 {
  let page = client
    .memories(
      brain_id,
      &MemoriesQuery {
        status: Some("invalidated".to_owned()),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  page.items[0].id
}

#[tokio::test]
async fn a_held_row_never_travels_in_a_brain_file() {
  let served = Served::new().await;
  let client = &served.client;
  let first = agent_brain(client, "account-a", "agent-a").await;
  declare(client, "holder-a", vec![declared("manual", "how the holder is used.")]).await;

  // No writing path reaches a holder's key. A first version naming a key a held row stands on is refused
  // outright, the sentence naming the row that holds it, and a version of the held row itself is refused
  // with the holder named, so the key stays the holder's alone.
  let taken = refused(
    client
      .memory_add(
        first,
        &NewMemory {
          key: Some("manual".to_owned()),
          kind: "fact".to_owned(),
          text: "the brain's own saying".to_owned(),
          actor_name: "writer-a".to_owned(),
          ..Default::default()
        },
      )
      .await,
  );
  assert_eq!(taken, (409, "conflict".to_owned()));

  let own = client
    .memory_add(
      first,
      &NewMemory {
        key: Some("own-saying".to_owned()),
        kind: "fact".to_owned(),
        text: "the brain's own saying".to_owned(),
        actor_name: "writer-a".to_owned(),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(own.held_by, None);

  // Export writes the brain's own row alone, and a file therefore carries no holder's row to stand over a
  // held key: what a holder stands for comes back from its declaration, in every agent's brain at once.
  let file = client.export(first, &bmo_brain_client::ExportQuery::default()).await.unwrap();
  let late = agent_brain(client, "account-b", "agent-b").await;
  let injected = client.inject(late, file).await.unwrap();
  assert_eq!((injected.memories_added, injected.memories_superseded), (1, 0));
  assert_eq!(own_of(client, late).await, vec!["the brain's own saying".to_owned()]);
  assert_eq!(
    held_of(client, late).await,
    held_of(client, first).await,
    "a brain made after a declaration is given what stands, and no file carries it"
  );
}

#[tokio::test]
async fn the_consoles_token_reaches_no_declaration_route() {
  let served = Served::new().await;
  let console = served.with_token(CONSOLE_TOKEN);
  agent_brain(&served.client, "account-a", "agent-a").await;

  assert_eq!(
    refused(
      console
        .declare(&Declaration {
          within: None,
          held_by: "holder-a".to_owned(),
          memories: vec![declared("first", "words.")],
        })
        .await,
    ),
    (403, "forbidden".to_owned())
  );
  assert_eq!(refused(console.holders(&HoldersQuery::default()).await), (403, "forbidden".to_owned()));
  assert_eq!(refused(console.holder_withdraw("holder-a").await), (403, "forbidden".to_owned()));
}

#[tokio::test]
async fn a_declaration_names_its_key_and_its_rows_to_the_shape() {
  let served = Served::new().await;
  let client = &served.client;
  agent_brain(client, "account-a", "agent-a").await;

  // A key the shape refuses, an empty set, a key named twice, and no brain to hold it.
  assert_eq!(
    refused(
      client
        .declare(&Declaration {
          within: None,
          held_by: "no key here".to_owned(),
          memories: vec![declared("first", "words.")],
        })
        .await,
    ),
    (400, "bad-request".to_owned())
  );
  assert_eq!(
    refused(
      client
        .declare(&Declaration {
          within: None,
          held_by: "holder-a".to_owned(),
          memories: vec![],
        })
        .await,
    ),
    (400, "bad-request".to_owned())
  );
  assert_eq!(
    refused(
      client
        .declare(&Declaration {
          within: None,
          held_by: "holder-a".to_owned(),
          memories: vec![declared("same", "one."), declared("same", "two.")],
        })
        .await,
    ),
    (400, "bad-request".to_owned())
  );

  let blank = Served::new().await;
  assert_eq!(
    refused(
      blank
        .client
        .declare(&Declaration {
          within: None,
          held_by: "holder-a".to_owned(),
          memories: vec![declared("first", "words.")],
        })
        .await,
    ),
    (409, "conflict".to_owned())
  );
}

async fn declare_within(
  client: &BrainClient,
  holder: &str,
  within: HolderReach,
  memories: Vec<DeclaredMemory>,
) -> Result<bmo_brain_client::Declared, String> {
  client
    .declare(&Declaration {
      within: Some(within),
      held_by: holder.to_owned(),
      memories,
    })
    .await
    .map_err(|why| why.to_string())
}

fn account_reach(account: &str) -> HolderReach {
  HolderReach {
    kind: None,
    account_key: account.to_owned(),
    agent_key: None,
  }
}

#[tokio::test]
async fn a_declaration_within_an_account_stands_in_that_accounts_agents_alone_and_in_one_made_later() {
  let served = Served::new().await;
  let client = &served.client;
  let mine = agent_brain(client, "account-a", "agent-a").await;
  let theirs = agent_brain(client, "account-b", "agent-b").await;
  let stood =
    declare_within(client, "app-a", account_reach("account-a"), vec![declared("app-a.when", "use app a.")])
      .await
      .unwrap();

  assert_eq!(stood.brains, 1);
  assert_eq!(held_of(client, mine).await.len(), 1);
  assert!(held_of(client, theirs).await.is_empty(), "another account's brain holds none of it");

  // A second agent of the same account is given it as it is made; another account's new agent is not.
  let later = agent_brain(client, "account-a", "agent-c").await;
  let other_later = agent_brain(client, "account-b", "agent-d").await;
  assert_eq!(held_of(client, later).await.len(), 1);
  assert!(held_of(client, other_later).await.is_empty());

  // A holder with no reach still reaches every agent's brain made later.
  declare(client, "service-a", vec![declared("service-a.when", "use service a.")]).await;
  let newest = agent_brain(client, "account-b", "agent-e").await;
  assert_eq!(
    held_of(client, newest).await,
    vec![(Some("service-a".to_owned()), "service-a.when".to_owned(), "use service a.".to_owned())]
  );
}

#[tokio::test]
async fn a_declaration_naming_another_reach_moves_the_set_and_a_withdrawal_forgets_the_reach() {
  let served = Served::new().await;
  let client = &served.client;
  let first = agent_brain(client, "account-a", "agent-a").await;
  let second = agent_brain(client, "account-b", "agent-b").await;
  declare_within(client, "app-a", account_reach("account-a"), vec![declared("app-a.when", "use app a.")])
    .await
    .unwrap();

  let moved =
    declare_within(client, "app-a", account_reach("account-b"), vec![declared("app-a.when", "use app a.")])
      .await
      .unwrap();
  assert_eq!((moved.written, moved.closed), (1, 1));
  assert!(
    held_of(client, first).await.is_empty(),
    "the brain the new reach leaves out holds none of it"
  );
  assert_eq!(held_of(client, second).await.len(), 1);

  client.holder_withdraw("app-a").await.unwrap();
  assert!(held_of(client, second).await.is_empty());
  let later = agent_brain(client, "account-b", "agent-c").await;
  assert!(
    held_of(client, later).await.is_empty(),
    "a withdrawn holder leaves nothing to a brain made later"
  );
}

#[tokio::test]
async fn a_declaration_within_an_org_brain_stands_there_alone_and_a_reach_naming_none_is_refused() {
  let served = Served::new().await;
  let client = &served.client;
  let agent = agent_brain(client, "account-a", "agent-a").await;
  let org = client
    .brain_add(&NewBrain {
      account_key: "company-a".to_owned(),
      agent_key: "unit-a".to_owned(),
      name: None,
      kind: Some(BrainKind::Org),
    })
    .await
    .unwrap()
    .id;
  let unit = HolderReach {
    kind: Some(BrainKind::Org),
    account_key: "company-a".to_owned(),
    agent_key: Some("unit-a".to_owned()),
  };
  let stood = declare_within(client, "app-b", unit, vec![declared("app-b.when", "use app b.")])
    .await
    .unwrap();

  assert_eq!(stood.brains, 1);
  assert_eq!(held_of(client, org).await.len(), 1);
  assert!(held_of(client, agent).await.is_empty());

  let nowhere =
    declare_within(client, "app-c", account_reach("account-z"), vec![declared("app-c.when", "use app c.")])
      .await;
  assert!(nowhere.unwrap_err().contains("409"), "a reach naming no brain is refused");
}
