//! Fragments, the knowledge read over them, and libraries synced into them (docs/boundary-api.md,
//! 「Fragments」, 「Libraries」 and 「Knowledge」).

mod support;

use bmo_brain::store::version;
use bmo_brain_client::{
  DriftChange, FragmentHits, FragmentMerge, FragmentPiece, FragmentSplit, FragmentState, FragmentSwitch,
  FragmentWrite, FragmentsQuery, KindWords, KnowledgeQuery, LibraryDocumentPut, LibraryPut, LibrarySyncAsk,
  MemoryState, NameCursor, NewMemory,
};
use support::{Served, labels, refused};

fn fragment(kind: &str, scope: Option<&str>, text: &str) -> FragmentWrite {
  FragmentWrite {
    kind: kind.into(),
    text: text.into(),
    scope: scope.map(Into::into),
    ..Default::default()
  }
}

fn keys(fragments: &[bmo_brain_client::Fragment]) -> Vec<&str> {
  fragments.iter().map(|fragment| fragment.key.as_str()).collect()
}

#[tokio::test]
async fn a_fragment_is_written_whole_and_keeps_its_switches_where_a_write_names_none() {
  let served = Served::new().await;
  let client = &served.client;
  let written = client.fragment_put("tea", &fragment("knowledge", None, "Tea at four")).await.unwrap();
  assert_eq!(
    (written.state, written.version.clone()),
    (FragmentState::Active, version("Tea at four"))
  );

  let switched = client
    .fragment_switch(
      "tea",
      &FragmentSwitch {
        switched_off: Some(vec!["room:b".into()]),
        state: Some(FragmentState::Off),
      },
    )
    .await
    .unwrap();
  assert_eq!(switched.switched_off, ["room:b"]);
  let rewritten = client.fragment_put("tea", &fragment("knowledge", None, "Tea at five")).await.unwrap();
  assert_eq!((rewritten.id, rewritten.state), (written.id, FragmentState::Off));
  assert_eq!(rewritten.switched_off, ["room:b"]);
  assert_eq!(rewritten.text, "Tea at five");

  assert_eq!(refused(client.fragment_put("a b", &fragment("knowledge", None, "x")).await).0, 400);
  assert_eq!(
    refused(client.fragment_put(&"k".repeat(201), &fragment("knowledge", None, "x")).await).0,
    400
  );
  assert_eq!(refused(client.fragment_switch("coffee", &FragmentSwitch::default()).await).0, 404);
  client.fragment_remove("tea").await.unwrap();
  assert_eq!(refused(client.fragment_remove("tea").await), (404, "not-found".into()));
  assert_eq!(
    served.event_kinds().await,
    ["fragment.changed", "fragment.changed", "fragment.changed", "fragment.deleted"]
  );
}

#[tokio::test]
async fn fragments_are_listed_by_their_fields() {
  let served = Served::new().await;
  let client = &served.client;
  let sheet = client
    .fragment_put(
      "person-1",
      &FragmentWrite {
        subject_key: Some("person-1".into()),
        title: Some("Who they are".into()),
        ..fragment("person", Some("agent:a"), "Plays chess")
      },
    )
    .await
    .unwrap();
  client
    .fragment_put("rule-1", &fragment("rule", Some("room:a"), "Speak softly"))
    .await
    .unwrap();

  let listed = |query: FragmentsQuery| async move { client.fragments(&query).await.unwrap().items };
  assert_eq!(
    listed(FragmentsQuery {
      kind: Some("person".into()),
      ..Default::default()
    })
    .await,
    vec![sheet.clone()]
  );
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        scope: Some("room:a".into()),
        ..Default::default()
      })
      .await
    ),
    ["rule-1"]
  );
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        q: Some("WHO THEY".into()),
        ..Default::default()
      })
      .await
    ),
    ["person-1"]
  );
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        keys: Some("rule-1,none".into()),
        ..Default::default()
      })
      .await
    ),
    ["rule-1"]
  );
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        ids: Some(sheet.id.to_string()),
        ..Default::default()
      })
      .await
    ),
    ["person-1"]
  );
  assert_eq!(
    refused(
      client
        .fragments(&FragmentsQuery {
          ids: Some("x".into()),
          ..Default::default()
        })
        .await
    )
    .0,
    400
  );
}

#[tokio::test]
async fn knowledge_reads_the_scopes_own_and_shared_fragments_and_their_standing_rows() {
  let served = Served::new().await;
  let client = &served.client;
  client.fragment_put("shared", &fragment("knowledge", None, "Shared")).await.unwrap();
  client.fragment_put("own", &fragment("knowledge", Some("room:a"), "Own")).await.unwrap();
  client
    .fragment_put("other", &fragment("knowledge", Some("room:b"), "Other"))
    .await
    .unwrap();
  client.fragment_put("rule", &fragment("rule", Some("room:a"), "Rule")).await.unwrap();
  client
    .fragment_put(
      "hidden",
      &FragmentWrite {
        switched_off: Some(vec!["room:a".into()]),
        ..fragment("knowledge", None, "Hidden")
      },
    )
    .await
    .unwrap();
  client
    .fragment_put(
      "off",
      &FragmentWrite {
        state: Some(FragmentState::Off),
        ..fragment("knowledge", None, "Off")
      },
    )
    .await
    .unwrap();
  let row = |scope: &str, state: Option<MemoryState>| NewMemory {
    scope: scope.into(),
    subject_key: None,
    kind: "rule".into(),
    text: "Be brief".into(),
    conditions: vec![],
    labels: labels(&[]),
    due_at: None,
    state,
  };
  let standing = client.memory_add(&row("room:a", None)).await.unwrap();
  client.memory_add(&row("room:a", Some(MemoryState::Candidate))).await.unwrap();
  client.memory_add(&row("room:b", None)).await.unwrap();

  let read = client
    .knowledge(&KnowledgeQuery {
      scopes: "room:a,agent:a".into(),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(read.scopes, ["room:a", "agent:a"]);
  assert_eq!(keys(&read.fragments), ["shared", "own", "rule"]);
  assert_eq!(read.memories, vec![standing]);

  let narrowed = client
    .knowledge(&KnowledgeQuery {
      scopes: "room:a".into(),
      slugs: Some("rule,shared,other".into()),
      kinds: Some("knowledge,rule".into()),
      subject_key: None,
    })
    .await
    .unwrap();
  assert_eq!(keys(&narrowed.fragments), ["rule", "shared"]);
  assert_eq!(refused(client.knowledge(&KnowledgeQuery::default()).await).0, 400);
}

#[tokio::test]
async fn fragments_merge_into_one_and_split_into_pieces() {
  let served = Served::new().await;
  let client = &served.client;
  client.fragment_put("a", &fragment("knowledge", Some("room:a"), "First")).await.unwrap();
  client
    .fragment_put("b", &fragment("knowledge", Some("room:a"), "Second"))
    .await
    .unwrap();

  let merged = client
    .fragment_merge(&FragmentMerge {
      keys: vec!["b".into(), "a".into()],
      into: "a".into(),
      text: None,
      title: Some("Both".into()),
    })
    .await
    .unwrap();
  assert_eq!((merged.text.as_str(), merged.title.as_deref()), ("Second\n\nFirst", Some("Both")));
  assert_eq!(keys(&client.fragments(&FragmentsQuery::default()).await.unwrap().items), ["a"]);
  let lonely = FragmentMerge {
    keys: vec!["a".into()],
    into: "a".into(),
    text: None,
    title: None,
  };
  assert_eq!(refused(client.fragment_merge(&lonely).await).0, 400);

  let piece = |key: &str, text: &str| FragmentPiece {
    key: key.into(),
    text: text.into(),
    kind: None,
    subject_key: None,
    title: None,
    path: None,
    labels: None,
  };
  let split = client
    .fragment_split(&FragmentSplit {
      key: "a".into(),
      pieces: vec![piece("a-1", "Second"), piece("a-2", "First")],
    })
    .await
    .unwrap();
  assert_eq!(keys(&split.items), ["a-1", "a-2"]);
  assert_eq!(split.items[0].scope.as_deref(), Some("room:a"));
  assert_eq!(keys(&client.fragments(&FragmentsQuery::default()).await.unwrap().items), ["a-1", "a-2"]);
  let short = FragmentSplit {
    key: "a-1".into(),
    pieces: vec![piece("a-1", "Second")],
  };
  assert_eq!(refused(client.fragment_split(&short).await).0, 400);
}

#[tokio::test]
async fn hits_are_counted_on_the_fragments_that_stand() {
  let served = Served::new().await;
  let client = &served.client;
  client.fragment_put("a", &fragment("knowledge", None, "First")).await.unwrap();
  let recorded = client
    .fragment_hits(&FragmentHits {
      keys: vec!["a".into(), "gone".into(), "a".into()],
      actor_name: "reader-a".into(),
      scope: Some("room:a".into()),
    })
    .await
    .unwrap();
  assert_eq!(recorded.recorded_count, 1);
  let read = client.fragments(&FragmentsQuery::default()).await.unwrap().items.remove(0);
  assert_eq!(read.hit_count, 1);
  assert!(read.last_hit_at.is_some());
}

#[tokio::test]
async fn a_library_is_put_whole_and_its_fragments_synced_to_it() {
  let served = Served::new().await;
  let client = &served.client;
  let document = |key: &str, text: &str| LibraryDocumentPut {
    key: key.into(),
    kind: "knowledge".into(),
    scope: None,
    subject_key: None,
    title: None,
    path: Some(format!("library/{key}.md")),
    text: text.into(),
  };
  client
    .fragment_put(
      "claimed",
      &FragmentWrite {
        labels: Some(labels(&[("kept", "yes")])),
        ..fragment("knowledge", None, "By hand")
      },
    )
    .await
    .unwrap();
  let put = LibraryPut {
    documents: vec![document("new", "New text"), document("claimed", "From the library")],
    kinds: vec![KindWords {
      name: "knowledge".into(),
      description: "What is known".into(),
    }],
  };
  let library = client.library_put("shelf", &put).await.unwrap();
  assert_eq!(library.documents[0].version, version("New text"));
  assert_eq!(client.library("shelf").await.unwrap(), library);

  let ask = LibrarySyncAsk {
    library_name: "shelf".into(),
  };
  let previewed = client.library_sync(&ask, true).await.unwrap();
  let changes: Vec<(&str, DriftChange)> =
    previewed.drift.iter().map(|drift| (drift.key.as_str(), drift.change)).collect();
  assert_eq!(changes, [("claimed", DriftChange::Claimed), ("new", DriftChange::Added)]);
  assert!(!previewed.applied);
  assert_eq!(client.fragments(&FragmentsQuery::default()).await.unwrap().items.len(), 1);

  assert!(client.library_sync(&ask, false).await.unwrap().applied);
  let claimed = client
    .fragments(&FragmentsQuery {
      keys: Some("claimed".into()),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(claimed.items[0].text, "From the library");
  assert_eq!(claimed.items[0].labels, labels(&[("kept", "yes")]));
  assert_eq!(claimed.items[0].library_name.as_deref(), Some("shelf"));
  assert!(client.library_sync(&ask, true).await.unwrap().drift.is_empty());

  let smaller = LibraryPut {
    documents: vec![document("new", "Newer text")],
    kinds: vec![],
  };
  client.library_put("shelf", &smaller).await.unwrap();
  let synced = client.library_sync(&ask, false).await.unwrap();
  let changes: Vec<(&str, DriftChange)> =
    synced.drift.iter().map(|drift| (drift.key.as_str(), drift.change)).collect();
  assert_eq!(changes, [("claimed", DriftChange::Removed), ("new", DriftChange::Changed)]);
  assert_eq!(keys(&client.fragments(&FragmentsQuery::default()).await.unwrap().items), ["new"]);

  let kinds = client.fragment_kinds(&NameCursor::default()).await.unwrap();
  assert_eq!(kinds.items.len(), 1);
  assert_eq!((kinds.items[0].fragment_count, kinds.items[0].description.as_str()), (1, ""));
  assert_eq!(client.libraries(&NameCursor::default()).await.unwrap().items.len(), 1);
  assert_eq!(refused(client.library("none").await).0, 404);
  let unknown = LibrarySyncAsk {
    library_name: "none".into(),
  };
  assert_eq!(refused(client.library_sync(&unknown, false).await).0, 404);
  let twice = LibraryPut {
    documents: vec![document("x", "One"), document("x", "Two")],
    kinds: vec![],
  };
  assert_eq!(refused(client.library_put("shelf", &twice).await).0, 400);
}
