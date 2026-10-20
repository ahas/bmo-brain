//! Watches and reminders, and the clock's pass that fires them (docs/boundary-api.md, 「Watches」 and
//! 「Reminders」). The pass is called with the moment it reads, so no test waits on the clock.

mod support;

use bmo_brain::clock::tick;
use bmo_brain_client::{
  MemoryChange, MemoryState, NewMemory, NewReminder, NewWatch, Object, RemindersQuery, WatchChange,
  WatchState, WatchesQuery,
};
use serde_json::{Value, json};
use support::{Served, labels, refused};

fn condition(value: Value) -> Object {
  value.as_object().unwrap().clone()
}

fn watch(condition: Object) -> NewWatch {
  NewWatch {
    scope: "room:a".into(),
    condition,
    setter_name: "setter-a".into(),
    labels: labels(&[]),
  }
}

fn moved(state: WatchState, holder: Option<&str>) -> WatchChange {
  WatchChange {
    state,
    holder_name: holder.map(Into::into),
  }
}

#[tokio::test]
async fn a_watch_on_a_moment_fires_once_the_clock_reaches_it() {
  let served = Served::new().await;
  let set = served
    .client
    .watch_add(&watch(condition(json!({ "kind": "at", "at": 2_000 }))))
    .await
    .unwrap();
  assert_eq!(set.state, WatchState::Armed);

  tick(&served.store, 1_999).await.unwrap();
  assert_eq!(served.client.watch(set.id).await.unwrap().state, WatchState::Armed);
  tick(&served.store, 2_000).await.unwrap();
  let fired = served.client.watch(set.id).await.unwrap();
  assert_eq!((fired.state, fired.fired_at), (WatchState::Fired, Some(2_000)));
  assert_eq!(served.event_kinds().await, ["watch.changed", "watch.fired"]);
}

#[tokio::test]
async fn a_watch_on_a_row_fires_as_the_row_reaches_its_state_or_at_once_where_it_has() {
  let served = Served::new().await;
  let client = &served.client;
  let row = client
    .memory_add(&NewMemory {
      scope: "room:a".into(),
      subject_key: None,
      kind: "promise".into(),
      text: "Send the photos".into(),
      conditions: vec![],
      labels: labels(&[]),
      due_at: None,
      state: None,
    })
    .await
    .unwrap();
  let on_done = client
    .watch_add(&watch(condition(json!({ "kind": "memory", "memoryId": row.id, "state": "done" }))))
    .await
    .unwrap();
  let on_standing = client
    .watch_add(&watch(condition(json!({ "kind": "memory", "memoryId": row.id, "state": "standing" }))))
    .await
    .unwrap();
  assert_eq!((on_done.state, on_standing.state), (WatchState::Armed, WatchState::Fired));
  assert_eq!(client.memory(row.id).await.unwrap().watches, vec![on_done.clone(), on_standing]);

  client
    .memory_update(
      row.id,
      &MemoryChange {
        state: Some(MemoryState::Done),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(client.watch(on_done.id).await.unwrap().state, WatchState::Fired);
}

#[tokio::test]
async fn a_watch_condition_of_no_known_shape_is_refused() {
  let served = Served::new().await;
  for refused_condition in [
    json!({ "kind": "sometime" }),
    json!({ "kind": "at" }),
    json!({ "kind": "memory", "memoryId": 1, "state": "done" }),
    json!({ "kind": "memory", "memoryId": 1, "state": "elsewhere" }),
    json!({ "kind": "external", "key": " " }),
  ] {
    assert_eq!(refused(served.client.watch_add(&watch(condition(refused_condition))).await).0, 400);
  }
}

#[tokio::test]
async fn an_external_watch_is_fired_claimed_by_one_holder_and_ended() {
  let served = Served::new().await;
  let client = &served.client;
  let set = client
    .watch_add(&watch(condition(json!({ "kind": "external", "key": "item-7" }))))
    .await
    .unwrap();

  assert_eq!(
    refused(client.watch_update(set.id, &moved(WatchState::Claimed, Some("holder-a"))).await).0,
    409
  );
  assert_eq!(
    client.watch_update(set.id, &moved(WatchState::Fired, None)).await.unwrap().state,
    WatchState::Fired
  );
  assert_eq!(refused(client.watch_update(set.id, &moved(WatchState::Claimed, None)).await).0, 400);
  let claimed = client
    .watch_update(set.id, &moved(WatchState::Claimed, Some("holder-a")))
    .await
    .unwrap();
  assert_eq!(claimed.holder_name.as_deref(), Some("holder-a"));
  assert_eq!(
    client
      .watch_update(set.id, &moved(WatchState::Claimed, Some("holder-a")))
      .await
      .unwrap(),
    claimed
  );
  assert_eq!(
    refused(client.watch_update(set.id, &moved(WatchState::Claimed, Some("holder-b"))).await),
    (409, "conflict".into())
  );
  let ended = client.watch_update(set.id, &moved(WatchState::Ended, None)).await.unwrap();
  assert_eq!((ended.state, ended.holder_name.as_deref()), (WatchState::Ended, Some("holder-a")));
  assert_eq!(refused(client.watch_update(set.id, &moved(WatchState::Cancelled, None)).await).0, 409);

  let held = client
    .watches(&WatchesQuery {
      holder_name: Some("holder-a".into()),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(held.items, vec![ended]);
  assert_eq!(refused(client.watch(set.id + 5).await).0, 404);
}

#[tokio::test]
async fn a_reminder_is_due_once_its_moment_comes_and_a_cancelled_one_never_is() {
  let served = Served::new().await;
  let client = &served.client;
  let reminder = |due_at: i64, text: &str| NewReminder {
    scope: "room:a".into(),
    due_at,
    text: text.into(),
    setter_name: "setter-a".into(),
    labels: labels(&[("repeat", "daily")]),
  };
  let first = client.reminder_add(&reminder(1_000, "Water the plants")).await.unwrap();
  let second = client.reminder_add(&reminder(3_000, "Lock up")).await.unwrap();
  let third = client.reminder_add(&reminder(500, "Never mind")).await.unwrap();
  client.reminder_cancel(third.id).await.unwrap();
  assert_eq!(refused(client.reminder_cancel(third.id).await), (404, "not-found".into()));

  tick(&served.store, 2_000).await.unwrap();
  tick(&served.store, 2_500).await.unwrap();
  let fired = client
    .reminders(&RemindersQuery {
      fired: Some(true),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(fired.items.len(), 1);
  assert_eq!((fired.items[0].id, fired.items[0].fired_at), (first.id, Some(2_000)));
  let waiting = client
    .reminders(&RemindersQuery {
      fired: Some(false),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(waiting.items, vec![second]);
  assert_eq!(
    served.event_kinds().await,
    [
      "reminder.created",
      "reminder.created",
      "reminder.created",
      "reminder.deleted",
      "reminder.due"
    ]
  );
}
