//! Fragments, listed and switched through the served routes, and their kinds (docs/boundary-api.md,
//! 「Fragments」). No route writes a fragment, so each test puts its fragments in the table.

mod support;

use bmo_brain_client::{FragmentState, FragmentSwitch, FragmentsQuery, NameCursor};
use support::{Piece, Served, piece, refused};

fn keys(fragments: &[bmo_brain_client::Fragment]) -> Vec<&str> {
  fragments.iter().map(|fragment| fragment.key.as_str()).collect()
}

#[tokio::test]
async fn a_fragment_is_switched_off_for_scopes_and_on_again() {
  let served = Served::new().await;
  let client = &served.client;
  served.put_fragment(piece("tea", "knowledge", None, "Tea at four")).await;

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
  assert_eq!(
    (switched.state, switched.switched_off.clone()),
    (FragmentState::Off, vec!["room:b".into()])
  );
  let kept = client
    .fragment_switch(
      "tea",
      &FragmentSwitch {
        switched_off: None,
        state: Some(FragmentState::Active),
      },
    )
    .await
    .unwrap();
  assert_eq!((kept.state, kept.switched_off), (FragmentState::Active, vec!["room:b".to_owned()]));
  assert_eq!(kept.text, "Tea at four");

  let too_long = FragmentSwitch {
    switched_off: Some(vec!["s".repeat(201)]),
    state: None,
  };
  assert_eq!(refused(client.fragment_switch("tea", &too_long).await).0, 400);
  assert_eq!(
    refused(client.fragment_switch("coffee", &FragmentSwitch::default()).await),
    (404, "not-found".into())
  );
  assert_eq!(served.event_kinds().await, ["fragment.changed", "fragment.changed"]);
}

#[tokio::test]
async fn fragments_are_listed_by_their_fields() {
  let served = Served::new().await;
  let client = &served.client;
  let sheet = served
    .put_fragment(Piece {
      subject_key: Some("person-1".into()),
      title: Some("Who they are".into()),
      library_name: Some("shelf".into()),
      ..piece("person-1", "person", Some("agent:a"), "Plays chess")
    })
    .await;
  served.put_fragment(piece("rule-1", "rule", Some("room:a"), "Speak softly")).await;

  let listed = |query: FragmentsQuery| async move { client.fragments(&query).await.unwrap().items };
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        kind: Some("person".into()),
        ..Default::default()
      })
      .await
    ),
    ["person-1"]
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
        subject_key: Some("person-1".into()),
        ..Default::default()
      })
      .await
    ),
    ["person-1"]
  );
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        library_name: Some("shelf".into()),
        ..Default::default()
      })
      .await
    ),
    ["person-1"]
  );
  assert_eq!(
    keys(
      &listed(FragmentsQuery {
        state: Some(FragmentState::Active),
        ..Default::default()
      })
      .await
    ),
    ["person-1", "rule-1"]
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
        ids: Some(sheet.to_string()),
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
async fn the_kinds_are_every_kind_a_fragment_names_or_a_description_is_held_for() {
  let served = Served::new().await;
  let client = &served.client;
  served.put_fragment(piece("tea", "knowledge", None, "Tea at four")).await;
  served.put_fragment(piece("cake", "knowledge", None, "Cake on Fridays")).await;
  served.put_fragment(piece("person-1", "person", None, "Plays chess")).await;
  served.put_kind("knowledge", "What is known").await;
  served.put_kind("lore", "The world's tales").await;

  let kinds = client.fragment_kinds(&NameCursor::default()).await.unwrap();
  let read: Vec<(&str, &str, i64)> = kinds
    .items
    .iter()
    .map(|kind| (kind.name.as_str(), kind.description.as_str(), kind.fragment_count))
    .collect();
  assert_eq!(
    read,
    [("knowledge", "What is known", 2), ("lore", "The world's tales", 0), ("person", "", 1)]
  );

  let first = client
    .fragment_kinds(&NameCursor {
      after: None,
      limit: Some(1),
    })
    .await
    .unwrap();
  assert!(first.more);
  let rest = client
    .fragment_kinds(&NameCursor {
      after: Some(first.items[0].name.clone()),
      limit: None,
    })
    .await
    .unwrap();
  assert_eq!(rest.items.iter().map(|kind| kind.name.as_str()).collect::<Vec<_>>(), ["lore", "person"]);
}
