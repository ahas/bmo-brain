//! The brain's clock (docs/boundary-api.md, 「Runs launched apart」; docs/memories.md, 「Watches」): one watch sweep
//! ending the watches of closed memories, firing due timers once and launching the checks nobody holds; and, on
//! paused time, the memo digest three minutes in and hourly, the person book five minutes in with every
//! recollecting agent's sort, and a pass that fails going on to the next.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bmo_brain::editors::fragments::store::now_seconds;
use bmo_brain::schedules::clock::{self, WATCH_SWEEP, every, watch_sweep};
use serde_json::Value;

use support::served::{Served, served};
use support::{AGENT, ROOM};

impl Served {
  // The units launched so far, in order.
  fn launched(&self) -> Vec<String> {
    self
      .agency
      .calls
      .of("launch_unit")
      .iter()
      .map(|launch| launch["unit"].as_str().unwrap().to_owned())
      .collect()
  }

  fn memos(&self) -> Vec<Value> {
    self.agency.calls.of("quiet_instruction")
  }

  async fn state_of(&self, id: i64) -> String {
    let row = sea_orm::ConnectionTrait::query_one(
      self.scratch.db(),
      sea_orm::Statement::from_sql_and_values(
        sea_orm::DbBackend::Postgres,
        r#"SELECT "state" FROM "watches" WHERE "id" = $1"#,
        vec![id.into()],
      ),
    )
    .await
    .unwrap()
    .unwrap();

    row.try_get("", "state").unwrap()
  }

  async fn watch(&self, kind: &str, check: Option<&str>, due_at: Option<i64>, memory: Option<i64>) -> i64 {
    let now = now_seconds();

    self
      .scratch
      .run(
        r#"INSERT INTO "watches" ("agent", "chat_id", "by_chat", "kind", "condition", "note", "dedupe_key",
             "interval_seconds", "due_at", "deadline_at", "state", "memory_id", "created_at")
           VALUES ($1, $2, $2, $3, $4, 'look again', $5, 60, $6, $7, 'active', $8, $9)"#,
        vec![
          AGENT.into(),
          ROOM.into(),
          kind.into(),
          check.into(),
          format!("{kind}-{check:?}-{memory:?}-{due_at:?}").into(),
          due_at.into(),
          (now + 3_600).into(),
          memory.into(),
          now.into(),
        ],
      )
      .await;

    self.scratch.count(r#"SELECT MAX("id") FROM "watches""#, vec![]).await
  }
}

async fn wait(seconds: u64) {
  tokio::time::sleep(Duration::from_secs(seconds)).await;
}

// The sweep's work is the database's, which paused time cannot wait on, so it is asked on real time; the clock's
// periods are asked on paused time below, with passes that touch only the agency in memory.
#[tokio::test]
async fn a_sweep_ends_closed_memories_watches_fires_due_timers_once_and_launches_the_unheld() {
  let api = served().await;

  api
    .scratch
    .run(
      r#"INSERT INTO "memories" ("slug", "type", "chat_id", "reach", "condition", "action", "set_at", "active", "hits",
           "event", "agent", "done_at") VALUES ('done-one', 'promise', $1, 'all', '', 'it is done', 1, 1, 0, '', $2, 5)"#,
      vec![ROOM.into(), AGENT.into()],
    )
    .await;

  let closed = api
    .scratch
    .count(r#"SELECT "id" FROM "memories" WHERE "slug" = 'done-one'"#, vec![])
    .await;
  let timer = api.watch("timer", None, Some(now_seconds() - 10), None).await;
  let later = api.watch("timer", None, Some(now_seconds() + 3_600), None).await;
  let serving = api.watch("condition", Some("test -e /tmp/never"), None, Some(closed)).await;
  let unheld = api.watch("condition", Some("test -e /tmp/never-either"), None, None).await;

  watch_sweep(&api.api).await.unwrap();

  assert_eq!(api.state_of(serving).await, "cancelled");
  assert_eq!(api.state_of(timer).await, "fired");
  assert_eq!(api.state_of(later).await, "active");
  assert_eq!(api.state_of(unheld).await, "active");
  assert_eq!(api.memos().len(), 1);
  assert!(
    api.memos()[0]["text"]
      .as_str()
      .unwrap()
      .starts_with("The moment you set for yourself has come.")
  );
  assert_eq!(
    (api.memos()[0]["agent"].clone(), api.memos()[0]["kind"].clone()),
    (AGENT.into(), "watch".into())
  );

  let launched = api.launched();

  assert_eq!(launched.len(), 1);
  assert!(launched[0].starts_with(&format!("bmo-watch-{unheld}-")), "{launched:?}");

  // The next sweep fires nothing twice and launches nothing a run holds.
  watch_sweep(&api.api).await.unwrap();
  assert_eq!(api.memos().len(), 1);
  assert_eq!(api.launched().len(), 1);
}

#[tokio::test]
async fn the_due_digests_are_launched_on_their_periods() {
  let api = served().await;

  // The clock stands on paused time once the stage is laid.
  tokio::time::pause();

  let passes = clock::stand(api.api.clone());
  let digests = |api: &Served| {
    api
      .launched()
      .into_iter()
      .filter(|unit| !unit.starts_with("bmo-watch-"))
      .collect::<Vec<_>>()
  };

  wait(170).await;
  assert!(digests(&api).is_empty());

  // Three minutes in, the memo digest; five minutes in, the person book and every recollecting agent's sort.
  wait(20).await;
  assert_eq!(digests(&api), ["bmo-digest"]);
  wait(120).await;
  assert_eq!(
    digests(&api),
    [
      "bmo-digest".to_owned(),
      "bmo-people-digest".to_owned(),
      format!("bmo-people-sort-{AGENT}")
    ]
  );

  // An hour after its first the memo digest is due again; the person book waits its three hours.
  wait(3_600).await;
  assert_eq!(digests(&api).iter().filter(|unit| *unit == "bmo-digest").count(), 2);
  assert_eq!(digests(&api).iter().filter(|unit| *unit == "bmo-people-digest").count(), 1);
  wait(3 * 3_600).await;
  assert_eq!(digests(&api).iter().filter(|unit| *unit == "bmo-people-digest").count(), 2);
  assert_eq!(WATCH_SWEEP, Duration::from_secs(30));

  for pass in passes {
    pass.abort();
  }
}

#[tokio::test]
async fn a_pass_that_fails_is_said_and_the_next_one_goes_on() {
  tokio::time::pause();

  let runs = Arc::new(AtomicUsize::new(0));
  let counted = runs.clone();
  let pass = tokio::spawn(every("test pass", Duration::from_secs(1), Duration::from_secs(10), move || {
    let runs = counted.clone();

    async move {
      runs.fetch_add(1, Ordering::SeqCst);
      Err("it failed".to_owned())
    }
  }));

  wait(25).await;
  assert_eq!(runs.load(Ordering::SeqCst), 3);
  pass.abort();
}
