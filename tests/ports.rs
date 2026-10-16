//! The ports' HTTP clients against a stub of each unit: every operation goes to its path behind the unit's
//! token with its camelCase body and reads its answer, a refusal reads as an answer, and a call is offered
//! again only while nothing listens.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::http::StatusCode;
use bmo_brain::ports::agency::{
  Agency, HttpAgency, ItemState, LaunchOutcome, Outcome, Overrun, QuietInstruction, Reload, SessionStart,
};
use bmo_brain::ports::gate::{Gate, HttpGate, Piece, Sanitised, SheetAsk, SheetPart};
use bmo_brain::ports::http::{Caller, Patience, Peer, PortFailure, Wait, WireError};
use bmo_brain::ports::receiver::{
  HttpReceiver, OwnerNotice, OwnerSpoke, Receiver, Rest, RoadKind, RoomCount, RoomSince,
};
use futures_util::future::BoxFuture;
use serde_json::json;
use support::agency::FakeAgency;
use support::gate::FakeGate;
use support::receiver::{FakeReceiver, message};
use support::stub::Stub;
use support::{AGENT, MESSAGE, OTHER_AGENT, OWNER, ROOM};

#[tokio::test]
async fn the_agency_client_posts_each_operation_and_reads_its_answer() {
  let stub = Stub::start().await;
  stub
    .answer(
      "/registrations/lookup",
      json!({ "agents": [{ "name": AGENT, "label": null, "key": "key-a", "brainFolder": "agents/agent-a",
        "awake": true, "recollectorRegistration": "local", "reasoning": "high" }] }),
    )
    .answer("/runs/start", json!({ "run": "run-7" }))
    .answer(
      "/runs/wait",
      json!({ "outcome": "limit", "answer": null, "filesWritten": ["people/1.md"], "cost": 0.5, "size": 900 }),
    )
    .answer("/runs/stop", json!({ "ok": true }))
    .answer("/runs/overrun", json!({ "keep": true, "until": 1_700_000_600 }))
    .answer("/runs/open", json!({ "open": false }))
    .answer("/work-keys/read", json!({ "capSeconds": 1200, "reasoning": null }))
    .answer("/items/states", json!({ "items": [{ "item": 3, "finished": true }, { "item": 4, "finished": false }] }))
    .answer("/units/launch", json!({ "outcome": "refused", "why": "The unit is masked." }))
    .answer("/instructions/quiet", json!({ "ok": true }))
    .answer("/documents/reload", json!({ "ok": true }));
  let agency = HttpAgency::new(&stub.peer);

  let known = agency.registrations(Some(AGENT)).await.unwrap();
  assert_eq!(stub.body_at("/registrations/lookup"), json!({ "agent": AGENT }));
  assert_eq!(known.len(), 1);
  assert_eq!(known[0].brain_folder, "agents/agent-a");
  assert_eq!(known[0].recollector_registration.as_deref(), Some("local"));

  let start = SessionStart {
    work_key: "people-digest".to_owned(),
    agent: AGENT.to_owned(),
    unit: "brain-run".to_owned(),
    prompt: "Read the room.".to_owned(),
    tools: vec!["fragment".to_owned()],
    workdir: "agents/agent-a".to_owned(),
    desk: "desk-1".to_owned(),
    registration: None,
  };
  assert_eq!(agency.start_session(&start).await.unwrap(), "run-7");
  assert_eq!(
    stub.body_at("/runs/start"),
    json!({ "workKey": "people-digest", "agent": AGENT, "unit": "brain-run", "prompt": "Read the room.",
      "tools": ["fragment"], "workdir": "agents/agent-a", "desk": "desk-1", "registration": null })
  );

  let end = agency.wait_session("run-7").await.unwrap();
  assert_eq!(stub.body_at("/runs/wait"), json!({ "run": "run-7" }));
  assert_eq!(end.outcome, Outcome::Limit);
  assert_eq!(end.files_written, ["people/1.md"]);
  assert_eq!((end.cost, end.size), (Some(0.5), Some(900)));

  assert!(agency.stop_session("run-7").await.unwrap());
  assert_eq!(stub.body_at("/runs/stop"), json!({ "run": "run-7" }));

  let overrun = agency.ask_overrun("run-7", "brain-run", 600).await.unwrap();
  assert_eq!(
    overrun,
    Overrun {
      keep: true,
      until: Some(1_700_000_600)
    }
  );
  assert_eq!(
    stub.body_at("/runs/overrun"),
    json!({ "run": "run-7", "unit": "brain-run", "capSeconds": 600 })
  );

  assert!(!agency.run_open("run-7").await.unwrap());
  assert_eq!(stub.body_at("/runs/open"), json!({ "run": "run-7" }));

  let key = agency.work_key("people-digest", AGENT).await.unwrap();
  assert_eq!((key.cap_seconds, key.reasoning), (1200, None));
  assert_eq!(stub.body_at("/work-keys/read"), json!({ "workKey": "people-digest", "agent": AGENT }));

  let states = agency.item_states(&[3, 4]).await.unwrap();
  assert_eq!(
    states,
    [
      ItemState {
        item: 3,
        finished: true
      },
      ItemState {
        item: 4,
        finished: false
      }
    ]
  );
  assert_eq!(stub.body_at("/items/states"), json!({ "items": [3, 4] }));

  let command = vec!["brain-run".to_owned(), "--once".to_owned()];
  let launch = agency.launch_unit("brain-run", &command).await.unwrap();
  assert_eq!(launch.outcome, LaunchOutcome::Refused);
  assert_eq!(launch.why.as_deref(), Some("The unit is masked."));
  assert_eq!(
    stub.body_at("/units/launch"),
    json!({ "unit": "brain-run", "command": ["brain-run", "--once"] })
  );

  let quiet = QuietInstruction {
    agent: AGENT.to_owned(),
    chat_id: ROOM,
    text: "The book was read again.".to_owned(),
    kind: None,
  };
  assert!(agency.quiet_instruction(&quiet).await.unwrap());
  assert_eq!(
    stub.body_at("/instructions/quiet"),
    json!({ "agent": AGENT, "chatId": ROOM, "text": "The book was read again.", "kind": null })
  );

  let reload = Reload {
    agent: AGENT.to_owned(),
    paths: vec!["knowledge/a.md".to_owned()],
    rooms: vec![ROOM],
  };
  assert!(agency.reload_documents(&reload).await.unwrap());
  assert_eq!(
    stub.body_at("/documents/reload"),
    json!({ "agent": AGENT, "paths": ["knowledge/a.md"], "rooms": [ROOM] })
  );
}

#[tokio::test]
async fn the_receiver_client_posts_each_operation_and_reads_its_answer() {
  let stub = Stub::start().await;
  let said = json!({ "chatId": ROOM, "messageId": MESSAGE, "senderId": OWNER, "senderName": "person",
    "sentAt": 1_700_000_000, "text": "hello", "replyToMessageId": null });
  stub
    .answer("/messages/unread", json!({ "messages": [said] }))
    .answer("/messages/unread-counts", json!({ "rooms": [{ "chatId": ROOM, "count": 3 }] }))
    .answer("/messages/read", json!({ "messages": [] }))
    .answer("/read-marks/count", json!({ "count": 2 }))
    .answer("/rooms/spoke-between", json!({ "spoke": true }))
    .answer("/rooms/own-number", json!({ "number": null }))
    .answer("/read-marks/keep", json!({ "ok": true }))
    .answer("/read-marks/forget", json!({ "forgotten": 4 }))
    .answer(
      "/rooms/titles",
      json!({ "rooms": [{ "chatId": ROOM, "title": null, "senders": [OWNER] }] }),
    )
    .answer(
      "/rooms/answerers",
      json!({ "rooms": [{ "chatId": ROOM, "agents": [AGENT, OTHER_AGENT] }] }),
    )
    .answer("/rooms/spoken-in", json!({ "people": [OWNER] }))
    .answer(
      "/people/names",
      json!({ "names": [{ "person": OWNER, "name": "person" }, { "person": 7, "name": null }] }),
    )
    .answer("/promises/waits", json!({ "waits": true }))
    .answer("/rooms/roads", json!({ "kind": "owner", "oneToOne": true }))
    .answer("/messages/sender-is-owner", json!({ "owner": true }))
    .answer("/rooms/owner-spoke", json!({ "owner": false, "spoke": true }))
    .answer("/rest", json!({ "resting": true, "until": 1_700_003_600 }))
    .answer("/owner/notice", json!({ "ok": true }));
  let receiver = HttpReceiver::new(&stub.peer);

  let unread = receiver.unread_messages(AGENT, ROOM, 50, Some(1_699_000_000)).await.unwrap();
  assert_eq!(serde_json::to_value(&unread).unwrap(), json!([said]));
  assert_eq!(
    stub.body_at("/messages/unread"),
    json!({ "agent": AGENT, "chatId": ROOM, "limit": 50, "since": 1_699_000_000 })
  );

  let since = [RoomSince {
    chat_id: ROOM,
    since: 1_699_000_000,
  }];
  let counts = receiver.unread_counts(AGENT, &since).await.unwrap();
  assert_eq!(
    counts,
    [RoomCount {
      chat_id: ROOM,
      count: 3
    }]
  );
  assert_eq!(
    stub.body_at("/messages/unread-counts"),
    json!({ "agent": AGENT, "since": [{ "chatId": ROOM, "since": 1_699_000_000 }] })
  );

  assert!(receiver.read_messages(AGENT, ROOM, MESSAGE, 20).await.unwrap().is_empty());
  assert_eq!(
    stub.body_at("/messages/read"),
    json!({ "agent": AGENT, "chatId": ROOM, "from": MESSAGE, "limit": 20 })
  );

  assert_eq!(receiver.read_count(AGENT, ROOM).await.unwrap(), 2);
  assert_eq!(stub.body_at("/read-marks/count"), json!({ "agent": AGENT, "chatId": ROOM }));

  assert!(receiver.spoke_between(ROOM, OWNER, 1_699_000_000, 1_700_000_000).await.unwrap());
  assert_eq!(
    stub.body_at("/rooms/spoke-between"),
    json!({ "chatId": ROOM, "person": OWNER, "from": 1_699_000_000, "to": 1_700_000_000 })
  );

  assert_eq!(receiver.own_number(AGENT, ROOM).await.unwrap(), None);
  assert_eq!(stub.body_at("/rooms/own-number"), json!({ "agent": AGENT, "chatId": ROOM }));

  assert!(receiver.keep_read_marks(AGENT, ROOM, &[MESSAGE]).await.unwrap());
  assert_eq!(
    stub.body_at("/read-marks/keep"),
    json!({ "agent": AGENT, "chatId": ROOM, "messageIds": [MESSAGE] })
  );

  assert_eq!(receiver.forget_read_marks(AGENT, None).await.unwrap(), 4);
  assert_eq!(stub.body_at("/read-marks/forget"), json!({ "agent": AGENT, "chatId": null }));

  let titles = receiver.room_titles().await.unwrap();
  assert_eq!((titles[0].chat_id, titles[0].title.as_deref()), (ROOM, None));
  assert_eq!(titles[0].senders, [OWNER]);
  assert_eq!(stub.body_at("/rooms/titles"), json!({}));

  let answerers = receiver.room_answerers(Some(ROOM)).await.unwrap();
  assert_eq!(answerers[0].agents, [AGENT, OTHER_AGENT]);
  assert_eq!(stub.body_at("/rooms/answerers"), json!({ "chatId": ROOM }));

  assert_eq!(receiver.spoken_in(ROOM, &[OWNER, 7]).await.unwrap(), [OWNER]);
  assert_eq!(stub.body_at("/rooms/spoken-in"), json!({ "chatId": ROOM, "people": [OWNER, 7] }));

  let names = receiver.display_names(&[OWNER, 7]).await.unwrap();
  assert_eq!(names[0].name.as_deref(), Some("person"));
  assert_eq!((names[1].person, names[1].name.as_deref()), (7, None));
  assert_eq!(stub.body_at("/people/names"), json!({ "people": [OWNER, 7] }));

  assert!(receiver.waits_on_happening("after the parcel comes").await.unwrap());
  assert_eq!(stub.body_at("/promises/waits"), json!({ "text": "after the parcel comes" }));

  let roads = receiver.roads(OWNER).await.unwrap();
  assert_eq!((roads.kind, roads.one_to_one), (RoadKind::Owner, true));
  assert_eq!(stub.body_at("/rooms/roads"), json!({ "chatId": OWNER }));

  assert!(receiver.sender_is_owner(ROOM, MESSAGE).await.unwrap());
  assert_eq!(
    stub.body_at("/messages/sender-is-owner"),
    json!({ "chatId": ROOM, "messageId": MESSAGE })
  );

  let spoke = receiver.owner_spoke(ROOM, 7, 1_700_000_000, 3600).await.unwrap();
  assert_eq!(
    spoke,
    OwnerSpoke {
      owner: false,
      spoke: true
    }
  );
  assert_eq!(
    stub.body_at("/rooms/owner-spoke"),
    json!({ "chatId": ROOM, "person": 7, "before": 1_700_000_000, "seconds": 3600 })
  );

  assert_eq!(
    receiver.rest_state().await.unwrap(),
    Rest {
      resting: true,
      until: Some(1_700_003_600)
    }
  );
  assert_eq!(stub.body_at("/rest"), json!({}));

  let mut params = serde_json::Map::new();
  params.insert("agent".to_owned(), json!(AGENT));
  let notice = OwnerNotice {
    code: "digest-paused".to_owned(),
    params,
  };
  assert!(receiver.notify_owner(&notice).await.unwrap());
  assert_eq!(stub.body_at("/owner/notice"), json!({ "code": "digest-paused", "agent": AGENT }));
}

#[tokio::test]
async fn the_gate_client_posts_each_operation_and_reads_a_refusal_as_an_answer() {
  let stub = Stub::start().await;
  stub
    .answer("/sanitiser/documents", json!({ "answered": true, "text": "kept" }))
    .answer("/sheets/judge", json!({ "sheet": null }));
  let gate = HttpGate::new(&stub.peer);

  assert_eq!(
    gate.sanitise("knowledge/a.md", "raw").await.unwrap(),
    Sanitised::Text("kept".to_owned())
  );
  assert_eq!(stub.body_at("/sanitiser/documents"), json!({ "path": "knowledge/a.md", "text": "raw" }));

  let ask = SheetAsk {
    agent: AGENT.to_owned(),
    part: SheetPart::Room,
    chat_id: Some(ROOM),
    person: OWNER,
    pieces: vec![Piece {
      path: "people/42/a.md".to_owned(),
      text: "piece".to_owned(),
    }],
    ceiling: 6000,
    registration: None,
  };
  assert_eq!(gate.judge_sheet(&ask).await.unwrap(), None);
  assert_eq!(
    stub.body_at("/sheets/judge"),
    json!({ "agent": AGENT, "part": "room", "chatId": ROOM, "person": OWNER,
      "pieces": [{ "path": "people/42/a.md", "text": "piece" }], "ceiling": 6000, "registration": null })
  );

  let refused = Stub::start().await;
  refused.answer(
    "/sanitiser/documents",
    json!({ "answered": false, "refusal": { "code": "too-long", "message": "The text is too long.", "limit": 9 } }),
  );
  let Sanitised::Refused(refusal) = HttpGate::new(&refused.peer).sanitise("a.md", "raw").await.unwrap()
  else {
    panic!("a refusal reads as a refusal");
  };
  assert_eq!((refusal.code.as_str(), refusal.message.as_str()), ("too-long", "The text is too long."));
  assert_eq!(refusal.params.get("limit"), Some(&json!(9)));
}

#[tokio::test]
async fn an_error_answer_is_read_and_never_offered_again() {
  let stub = Stub::start().await;
  stub.answer_with(
    "/runs/open",
    StatusCode::CONFLICT,
    json!({ "code": "not-ready", "error": "The service is not ready." }),
  );

  let failure = HttpAgency::new(&stub.peer).run_open("run-1").await.unwrap_err();

  let PortFailure::Answered { status, answer, .. } = failure else { panic!("{failure}") };
  assert_eq!(status, 409);
  assert_eq!(
    answer,
    Some(WireError {
      code: "not-ready".to_owned(),
      error: "The service is not ready.".to_owned(),
    })
  );
  assert_eq!(stub.taken().len(), 1);
}

#[tokio::test]
async fn an_answer_of_another_shape_is_undecodable() {
  let stub = Stub::start().await;
  stub.answer("/rest", json!({ "sleeping": true }));

  let failure = HttpReceiver::new(&stub.peer).rest_state().await.unwrap_err();

  assert!(matches!(failure, PortFailure::Undecodable { .. }), "{failure}");
}

// Counts the waits it is asked for and returns at once.
#[derive(Default)]
struct Counted(AtomicU32);

impl Wait for Counted {
  fn wait(&self, _interval: Duration) -> BoxFuture<'_, ()> {
    self.0.fetch_add(1, Ordering::SeqCst);
    Box::pin(async {})
  }
}

#[tokio::test]
async fn a_refused_connection_is_offered_again_for_the_whole_window() {
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let address = listener.local_addr().unwrap();
  drop(listener);
  let waits = Arc::new(Counted::default());
  let patience = Patience {
    window: Duration::from_secs(10),
    every: Duration::from_secs(2),
    waiter: waits.clone(),
  };
  let peer = Peer {
    base: format!("http://{address}"),
    token: "token".to_owned(),
  };
  let gate = HttpGate::with_caller(Caller::new(&peer).with_patience(patience));

  let failure = gate.sanitise("a.md", "raw").await.unwrap_err();

  let PortFailure::Unreachable { waited, .. } = failure else { panic!("{failure}") };
  assert_eq!(waited, Duration::from_secs(10));
  assert_eq!(waits.0.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn the_fakes_answer_as_their_units_would() {
  let agency = FakeAgency::with_agents(&[AGENT, OTHER_AGENT]);
  let receiver = FakeReceiver::with_messages(vec![
    message(ROOM, 1, OWNER, 100),
    message(ROOM, 2, 7, 200),
    message(OWNER, 3, OWNER, 300),
  ]);
  let gate = FakeGate::default();
  let (agency, receiver, gate): (&dyn Agency, &dyn Receiver, &dyn Gate) = (&agency, &receiver, &gate);

  assert_eq!(agency.registrations(Some(OTHER_AGENT)).await.unwrap()[0].name, OTHER_AGENT);
  assert_eq!(agency.registrations(None).await.unwrap().len(), 2);

  assert!(receiver.keep_read_marks(AGENT, ROOM, &[1]).await.unwrap());
  let unread = receiver.unread_messages(AGENT, ROOM, 10, None).await.unwrap();
  assert_eq!(unread.iter().map(|message| message.message_id).collect::<Vec<_>>(), [2]);
  assert_eq!(receiver.unread_messages(OTHER_AGENT, ROOM, 10, None).await.unwrap().len(), 2);
  assert_eq!(receiver.read_messages(AGENT, ROOM, 0, 10).await.unwrap()[0].message_id, 1);
  assert!(receiver.read_messages(AGENT, ROOM, 1, 10).await.unwrap().is_empty());
  assert_eq!(receiver.read_count(AGENT, ROOM).await.unwrap(), 1);
  let counts = receiver
    .unread_counts(
      AGENT,
      &[RoomSince {
        chat_id: ROOM,
        since: 300,
      }],
    )
    .await
    .unwrap();
  assert_eq!(
    counts,
    [RoomCount {
      chat_id: OWNER,
      count: 1
    }]
  );
  assert!(receiver.spoke_between(ROOM, 7, 200, 200).await.unwrap());
  assert!(!receiver.spoke_between(ROOM, 7, 201, 300).await.unwrap());
  assert_eq!(receiver.own_number(AGENT, ROOM).await.unwrap(), None);
  assert!(receiver.sender_is_owner(ROOM, 1).await.unwrap());
  assert_eq!(receiver.forget_read_marks(AGENT, None).await.unwrap(), 1);

  assert_eq!(gate.sanitise("a.md", "raw").await.unwrap(), Sanitised::Text("raw".to_owned()));
}
