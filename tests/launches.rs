//! What the brain's scheduled work starts apart, on the fake agency: the due digests and every agent's sort, one
//! room's digest and worker, the room names a run is titled by, and the watches nobody holds launched again.

mod support;

use std::collections::HashSet;

use bmo_brain::ports::agency::Registration;
use bmo_brain::ports::receiver::{RoadKind, Roads, RoomAnswerers, RoomTitle};
use bmo_brain::schedules::apart::{Launch, LaunchOutcome};
use bmo_brain::schedules::launches::{Launches, RoomDigest, Standing, to_relaunch, watch_unit};
use sea_orm::Value;
use serde_json::{Value as Json, json};

use support::agency::registration;
use support::editing::Stage;
use support::{AGENT, OTHER_AGENT, OWNER, ROOM};

fn watch(id: i64, kind: &str, unit: Option<&str>, armed_at: Option<i64>) -> Standing {
  Standing {
    id,
    chat_id: 77,
    by_chat: 78,
    kind: kind.to_owned(),
    unit: unit.map(str::to_owned),
    armed_at,
    deadline_at: 1_000,
    interval_seconds: Some(60),
  }
}

#[test]
fn a_check_is_launched_again_where_no_run_holds_it_once_its_grace_is_past() {
  let held = HashSet::from(["bmo-watch-2-1".to_owned()]);
  let active = [
    watch(1, "condition", None, None),
    watch(2, "condition", Some("bmo-watch-2-1"), Some(0)),
    watch(3, "condition", Some("bmo-watch-3-1"), Some(950)),
    watch(4, "condition", Some("bmo-watch-4-1"), Some(900)),
    watch(5, "timer", None, None),
  ];
  let ids: Vec<i64> = to_relaunch(&active, &held, 1_000).iter().map(|watch| watch.id).collect();

  assert_eq!(ids, [1, 4]);
  assert_eq!(watch_unit(3, 1_700_000_000_123), "bmo-watch-3-1700000000123");
}

fn program() -> Vec<String> {
  vec!["brain".to_owned(), "--config".to_owned(), "brain.toml".to_owned()]
}

fn launches<'a>(stage: &'a Stage, program: &'a [String]) -> Launches<'a> {
  Launches {
    ground: stage.ground(),
    program,
  }
}

fn launched(stage: &Stage) -> Vec<Json> {
  stage.starter.calls.of("start")
}

fn words(launch: &Json) -> Vec<String> {
  launch["command"]
    .as_array()
    .unwrap()
    .iter()
    .map(|word| word.as_str().unwrap().to_owned())
    .collect()
}

#[tokio::test]
async fn the_memo_digest_that_came_due_is_launched_where_no_run_of_it_stands() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);

  launches.digest("memo").await.unwrap();
  launches.digest("memo").await.unwrap();

  let calls = launched(&stage);

  assert_eq!(calls.len(), 2);
  assert_eq!(
    calls[0],
    json!({
      "unit": "bmo-digest",
      "title": "All pending notes",
      "command": ["brain", "--config", "brain.toml", "digest", "--unit", "bmo-digest"],
    })
  );
  // The second launch found the first one standing, which the service manager answers as running.
  assert!(stage.starter.standing.lock().unwrap().contains("bmo-digest"));
  assert_eq!(launches.digest("weekly").await, Err("no digest is called weekly".to_owned()));
}

#[tokio::test]
async fn the_people_digest_launches_its_launcher_and_every_recollecting_agents_sort_root_first() {
  let stage = Stage::new().await;
  let program = program();

  *stage.agency.registrations.lock().unwrap() = vec![
    Registration {
      brain_folder: Some("agents/agent-b/".to_owned()),
      ..registration(OTHER_AGENT)
    },
    Registration {
      brain_folder: Some(String::new()),
      ..registration(AGENT)
    },
    Registration {
      awake: false,
      brain_folder: Some("agents/agent-c".to_owned()),
      ..registration("agent-c")
    },
  ];

  let launches = launches(&stage, &program);

  assert_eq!(launches.recollectors().await.unwrap(), [AGENT, OTHER_AGENT]);
  launches.digest("people").await.unwrap();

  let calls = launched(&stage);
  let units: Vec<&str> = calls.iter().map(|call| call["unit"].as_str().unwrap()).collect();

  assert_eq!(units, ["bmo-people-digest", "bmo-people-sort-agent-a", "bmo-people-sort-agent-b"]);
  assert_eq!(calls[0]["title"], json!("Rooms in turn"));
  assert_eq!(words(&calls[0])[3..], ["people-digest", "--unit", "bmo-people-digest"]);
  assert_eq!(calls[2]["title"], json!("Sorting the person book"));
  assert_eq!(
    words(&calls[2])[3..],
    [
      "people-digest",
      "--sort",
      "--all",
      "--agent",
      OTHER_AGENT,
      "--unit",
      "bmo-people-sort-agent-b"
    ]
  );
}

#[tokio::test]
async fn the_chat_agent_recollects_at_the_root_where_no_registration_holds_it() {
  let stage = Stage::new().await;
  let program = program();

  *stage.agency.registrations.lock().unwrap() = vec![Registration {
    brain_folder: Some("agents/agent-b".to_owned()),
    ..registration(OTHER_AGENT)
  }];

  assert_eq!(launches(&stage, &program).recollectors().await.unwrap(), [AGENT, OTHER_AGENT]);
}

#[tokio::test]
async fn a_rooms_digest_is_launched_once_titled_by_the_room_and_a_refusal_is_answered() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);

  *stage.receiver.titles.lock().unwrap() = vec![RoomTitle {
    chat_id: ROOM,
    title: Some("Family".to_owned()),
    senders: vec![],
  }];

  assert_eq!(
    launches.room_digest(ROOM).await.unwrap(),
    RoomDigest::Launched("bmo-digest--100".to_owned())
  );
  assert_eq!(
    launches.room_digest(ROOM).await.unwrap(),
    RoomDigest::Running("bmo-digest--100".to_owned())
  );
  assert_eq!(launched(&stage)[0]["title"], json!("Family"));
  assert_eq!(
    words(&launched(&stage)[0])[3..],
    ["digest", "--chat", "-100", "--unit", "bmo-digest--100"]
  );

  *stage.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Refused,
    why: Some("The unit is masked.".to_owned()),
  });
  assert_eq!(
    launches.room_digest(OWNER).await.unwrap(),
    RoomDigest::Refused("The unit is masked.".to_owned())
  );
}

#[tokio::test]
async fn a_run_is_titled_by_its_rooms_title_the_owners_private_chat_or_its_number() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);

  *stage.receiver.titles.lock().unwrap() = vec![RoomTitle {
    chat_id: ROOM,
    title: Some("Family".to_owned()),
    senders: vec![],
  }];
  stage.receiver.roads.lock().unwrap().insert(
    OWNER,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );

  assert_eq!(launches.room_name(ROOM).await.unwrap(), "Family");
  assert_eq!(launches.room_name(OWNER).await.unwrap(), "Private chat");
  assert_eq!(launches.room_name(-300).await.unwrap(), "Room -300");
}

#[tokio::test]
async fn a_room_about_to_fold_launches_the_agents_worker_only_where_its_book_is_written_from_it() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);

  // The agent does not answer the room: nothing starts.
  launches.people_before_fold(ROOM, AGENT).await;
  assert!(launched(&stage).is_empty());

  *stage.receiver.answerers.lock().unwrap() = vec![RoomAnswerers {
    chat_id: ROOM,
    agents: vec![AGENT.to_owned()],
  }];
  launches.people_before_fold(ROOM, AGENT).await;

  let calls = launched(&stage);

  assert_eq!(calls.len(), 1);
  assert_eq!(calls[0]["unit"], json!("bmo-people-digest-agent-a--100"));
  assert_eq!(calls[0]["title"], json!("Room -100"));
  assert_eq!(
    words(&calls[0])[3..],
    [
      "people-digest",
      "--chat",
      "-100",
      "--agent",
      AGENT,
      "--unit",
      "bmo-people-digest-agent-a--100"
    ]
  );

  // A one-to-one keeps no room part, so its book is not written from it.
  stage.receiver.roads.lock().unwrap().insert(
    ROOM,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );
  stage.starter.standing.lock().unwrap().clear();
  launches.people_before_fold(ROOM, AGENT).await;
  assert_eq!(launched(&stage).len(), 1);
}

#[tokio::test]
async fn a_worker_catches_up_on_asking_stands_once_and_answers_the_agencys_refusal() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);
  let unit = "bmo-people-digest-agent-a--100";

  launches.people_worker(unit, ROOM, AGENT, true).await.unwrap();
  assert_eq!(
    words(&launched(&stage)[0])[3..],
    ["people-digest", "--chat", "-100", "--agent", AGENT, "--all", "--unit", unit]
  );
  // The service manager holds what was started, and the agency answers it alive.
  stage.agency.alive.lock().unwrap().insert(unit.to_owned());
  assert!(launches.standing(unit).await);
  assert!(!launches.standing("bmo-people-digest-agent-a--200").await);

  *stage.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Refused,
    why: Some("No host takes it.".to_owned()),
  });
  assert_eq!(
    launches.people_worker(unit, ROOM, AGENT, false).await,
    Err("No host takes it.".to_owned())
  );
}

// A watch standing in a room, armed at the moment given where it was.
async fn standing_watch(stage: &Stage, kind: &str, unit: Option<&str>, armed_at: Option<i64>) -> i64 {
  stage
    .scratch
    .run(
      r#"INSERT INTO "watches" ("chat_id", "by_chat", "kind", "note", "dedupe_key", "interval_seconds", "deadline_at",
           "unit", "armed_at", "created_at", "agent")
         VALUES ($1, $1, $2, 'tell them', $3, 60, 2000000000, $4, $5, 1, $6)"#,
      vec![
        Value::from(OWNER),
        kind.into(),
        format!("key-{kind}-{unit:?}").into(),
        unit.into(),
        armed_at.into(),
        AGENT.into(),
      ],
    )
    .await;

  stage.scratch.count(r#"SELECT MAX("id") FROM "watches""#, vec![]).await
}

async fn watch_column(stage: &Stage, id: i64, column: &str) -> Option<String> {
  let row = sea_orm::ConnectionTrait::query_one(
    stage.scratch.db(),
    sea_orm::Statement::from_sql_and_values(
      sea_orm::DbBackend::Postgres,
      format!(r#"SELECT "{column}"::text AS "value" FROM "watches" WHERE "id" = $1"#),
      vec![id.into()],
    ),
  )
  .await
  .unwrap()
  .unwrap();

  row.try_get::<Option<String>>("", "value").unwrap()
}

#[tokio::test]
async fn the_watches_nobody_holds_are_launched_again_each_in_a_run_of_its_own() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);
  let now_ms = 1_700_000_000_000;
  let now = now_ms / 1000;

  stage.receiver.roads.lock().unwrap().insert(
    OWNER,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );

  let lost = standing_watch(&stage, "condition", Some("bmo-watch-old"), Some(now - 600)).await;
  let held = standing_watch(&stage, "condition", Some("bmo-watch-held"), Some(now - 600)).await;
  let fresh = standing_watch(&stage, "condition", Some("bmo-watch-fresh"), Some(now - 10)).await;
  let timer = standing_watch(&stage, "timer", None, None).await;

  stage.agency.alive.lock().unwrap().insert("bmo-watch-held".to_owned());
  launches.relaunch_watches(now_ms).await.unwrap();

  let calls = launched(&stage);
  let unit = watch_unit(lost, now_ms);

  assert_eq!(calls.len(), 1);
  assert_eq!(calls[0]["unit"], json!(unit));
  assert_eq!(calls[0]["title"], json!(format!("Private chat watch #{lost}")));
  assert_eq!(
    words(&calls[0])[3..],
    [
      "watch-runner".to_owned(),
      lost.to_string(),
      unit.clone(),
      "--unit".to_owned(),
      unit.clone()
    ]
  );
  assert_eq!(watch_column(&stage, lost, "unit").await, Some(unit));
  assert_eq!(watch_column(&stage, lost, "armed_at").await, Some(now.to_string()));
  assert_eq!(watch_column(&stage, held, "unit").await.as_deref(), Some("bmo-watch-held"));
  assert_eq!(watch_column(&stage, fresh, "armed_at").await, Some((now - 10).to_string()));
  assert!(watch_column(&stage, timer, "unit").await.is_none());

  // One just set is launched by its number; one that no longer stands is not.
  assert!(launches.launch_watch_of(fresh, now_ms + 1).await.unwrap());
  stage
    .scratch
    .run(r#"UPDATE "watches" SET "state" = 'fired' WHERE "id" = $1"#, vec![Value::from(held)])
    .await;
  assert!(!launches.launch_watch_of(held, now_ms + 2).await.unwrap());

  // A refused launch leaves the watch standing for a later pass.
  *stage.starter.answer.lock().unwrap() = Some(Launch {
    outcome: LaunchOutcome::Refused,
    why: Some("No host takes it.".to_owned()),
  });
  assert!(!launches.launch_watch_of(lost, now_ms + 3).await.unwrap());
  assert_eq!(watch_column(&stage, lost, "state").await.as_deref(), Some("active"));
}

#[tokio::test]
async fn where_nobody_can_say_which_runs_stand_no_watch_is_launched_again() {
  let stage = Stage::new().await;
  let program = program();
  let launches = launches(&stage, &program);
  let now_ms = 1_700_000_000_000;

  standing_watch(&stage, "condition", Some("bmo-watch-old"), Some(now_ms / 1000 - 600)).await;
  *stage.agency.unsure.lock().unwrap() = true;
  launches.relaunch_watches(now_ms).await.unwrap();

  assert!(launched(&stage).is_empty());
  assert!(
    launches.standing("bmo-watch-old").await,
    "a unit nobody can speak for is taken as standing"
  );
}
