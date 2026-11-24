//! Removing an account's or a unit's brains (docs/boundary-api.md, 「Removing an account's or a unit's brains」):
//! the count changes nothing and names what the removal then takes, a second removal takes nothing, another key's
//! brains and the base stay, and the console's token reaches none of it.

use crate::support;

use bmo_brain_client::{BrainClient, BrainKind, BrainsHeld, BrainsQuery, NewBrain, NewMemory};
use support::{CONSOLE_TOKEN, Served, labels, refused};

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

async fn remember(client: &BrainClient, brain_id: i64, text: &str) {
  client
    .memory_add(
      brain_id,
      &NewMemory {
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
      },
    )
    .await
    .unwrap();
}

async fn standing(client: &BrainClient) -> Vec<(String, String)> {
  let page = client.brains(&BrainsQuery::default()).await.unwrap();
  page.items.into_iter().map(|brain| (brain.account_key, brain.agent_key)).collect()
}

#[tokio::test]
async fn an_accounts_agent_brains_go_whole_and_nothing_else_does() {
  let served = Served::new().await;
  let client = &served.client;
  let first = brain(client, BrainKind::Agent, "7", "agent-a").await;
  let second = brain(client, BrainKind::Agent, "7", "agent-b").await;
  brain(client, BrainKind::Agent, "8", "agent-a").await;
  brain(client, BrainKind::Org, "7", "70").await;
  remember(client, first, "One thing.").await;
  remember(client, first, "Another thing.").await;
  remember(client, second, "A third thing.").await;

  let counted = client.account_removal("7").await.unwrap();
  assert_eq!(
    counted.counts,
    BrainsHeld {
      brains: 2,
      memories: 3
    }
  );
  assert_eq!(standing(client).await.len(), 4, "a count changes nothing");

  let removed = client.account_remove("7").await.unwrap();
  assert_eq!(removed.removed, counted.counts);
  let left = standing(client).await;
  assert!(left.contains(&("8".to_owned(), "agent-a".to_owned())), "another account's brain stays");
  assert!(left.contains(&("7".to_owned(), "70".to_owned())), "an org brain keyed alike stays");
  assert_eq!(left.len(), 2);

  let again = client.account_remove("7").await.unwrap();
  assert_eq!(again.removed, BrainsHeld::default(), "a second removal takes nothing");
  assert!(served.event_kinds().await.iter().filter(|kind| *kind == "brain.deleted").count() >= 2);
}

#[tokio::test]
async fn a_units_org_brain_goes_and_the_base_never_does() {
  let served = Served::new().await;
  let client = &served.client;
  let unit = brain(client, BrainKind::Org, "2", "13").await;
  brain(client, BrainKind::Org, "2", "14").await;
  remember(client, unit, "The unit's hours.").await;

  assert_eq!(
    client.org_unit_removal("13", "2").await.unwrap().counts,
    BrainsHeld {
      brains: 1,
      memories: 1
    }
  );
  assert_eq!(
    client.org_unit_remove("13", "2").await.unwrap().removed,
    BrainsHeld {
      brains: 1,
      memories: 1
    }
  );
  assert_eq!(standing(client).await, vec![("2".to_owned(), "14".to_owned())]);
  assert_eq!(client.org_unit_remove("13", "2").await.unwrap().removed, BrainsHeld::default());

  let (status, _) = refused(client.org_unit_remove("platform", "platform").await);
  assert_eq!(status, 409, "the base is never a unit's removal");
}

#[tokio::test]
async fn the_consoles_token_reaches_no_removal() {
  let served = Served::new().await;
  let console = served.with_token(CONSOLE_TOKEN);
  let (status, _) = refused(console.account_remove("7").await);
  assert_eq!(status, 403);
  let (status, _) = refused(console.org_unit_removal("13", "2").await);
  assert_eq!(status, 403);
}
