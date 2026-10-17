//! The ports' HTTP clients against a stub of each unit: every operation goes to its path behind the unit's
//! token with its camelCase body and reads its answer, a refusal reads as an answer, and a call is offered
//! again only while nothing listens.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::http::StatusCode;
use bmo_brain::ports::agency::{
  Agency, EditorTurn, HttpAgency, Registration, Reload, Rest, Rulebook, RunClock, TurnEnd, TurnFault,
  TurnOutcome,
};
use bmo_brain::ports::gate::{Gate, HttpGate, Piece, Sanitised, SheetAsk, SheetPart};
use bmo_brain::ports::http::{Caller, Patience, Peer, PortFailure, Wait, WireError};
use bmo_brain::ports::job::{HttpJob, ItemState, Job};
use bmo_brain::ports::receiver::{
  HttpReceiver, OwnerNotice, OwnerSpoke, QuietInstruction, Receiver, RoadKind, RoomCount, RoomSince,
};
use futures_util::future::BoxFuture;
use serde_json::json;
use support::agency::FakeAgency;
use support::gate::FakeGate;
use support::receiver::{FakeReceiver, message};
use support::stub::{Stub, TOKEN};
use support::{AGENT, MESSAGE, OTHER_AGENT, OWNER, ROOM};
use tokio::sync::oneshot;

#[tokio::test]
async fn the_agency_client_calls_each_route_as_the_agency_documents_it() {
  let stub = Stub::start().await;
  stub
    .answer(
      "/agents",
      json!({ "agents": [
        { "name": AGENT, "kind": "companion", "label": null, "brainFolder": "agents/agent-a", "awake": true,
          "recollectorRegistration": "local" },
        { "name": OTHER_AGENT, "kind": "worker", "label": "Other" },
      ], "roles": [] }),
    )
    .answer(
      "/session/start",
      json!({ "id": "people-digest@editor", "agent": "people-digest", "scope": { "kind": "editor" } }),
    )
    .answer("/runs/stop", json!({ "ok": true }))
    .answer("/runs/alive", json!({ "answered": true, "alive": ["bmo-digest"] }))
    .answer("/rest", json!({ "resting": true, "until": 1_700_003_600, "kind": "limit" }))
    .answer(
      "/documents/reload",
      json!({ "paths": ["knowledge/a.md"], "now": false, "agent": AGENT, "rooms": [ROOM] }),
    )
    .answer("/runs/clock", json!({ "capEnds": 1_700_000_000_000_i64, "keptUntil": null }))
    .answer("/editors/cap", json!({ "capSeconds": 2_400 }))
    .answer(
      "/rulebooks",
      json!({ "rulebooks": [{ "name": "plain-writing", "text": "Write plainly." }] }),
    );
  let agency = HttpAgency::new(&stub.peer);

  let all = agency.registrations(None).await.unwrap();
  assert_eq!(all.len(), 2);
  assert_eq!(all[0].brain_folder.as_deref(), Some("agents/agent-a"));
  assert_eq!(all[0].recollector_registration.as_deref(), Some("local"));
  assert_eq!(
    all[1],
    Registration {
      name: OTHER_AGENT.to_owned(),
      label: Some("Other".to_owned()),
      brain_folder: None,
      awake: true,
      recollector_registration: None,
    },
    "an agent listed with no brain folder keeps none, and is taken as awake"
  );
  let named = agency.registrations(Some(OTHER_AGENT)).await.unwrap();
  assert_eq!(named.iter().map(|known| known.name.as_str()).collect::<Vec<_>>(), [OTHER_AGENT]);
  let read: Vec<_> = stub.taken().into_iter().filter(|taken| taken.path == "/agents").collect();
  assert_eq!(read.len(), 2);
  assert!(read.iter().all(|taken| taken.method == "GET" && taken.bearer.as_deref() == Some(TOKEN)));

  assert_eq!(agency.editor_session("people-digest").await.unwrap(), "people-digest@editor");
  assert_eq!(
    stub.body_at("/session/start"),
    json!({ "agent": "people-digest", "scope": { "kind": "editor" } })
  );

  assert!(agency.stop_run("turn:people-digest:1", "cut at its cap").await.unwrap());
  assert_eq!(
    stub.body_at("/runs/stop"),
    json!({ "run": "turn:people-digest:1", "reason": "cut at its cap" })
  );

  let units = vec!["bmo-digest".to_owned(), "bmo-people-digest".to_owned()];
  assert_eq!(agency.alive(&units).await.unwrap(), Some(vec!["bmo-digest".to_owned()]));
  assert_eq!(stub.body_at("/runs/alive"), json!({ "names": ["bmo-digest", "bmo-people-digest"] }));

  let rest = agency.rest().await.unwrap();
  assert_eq!(
    rest,
    Rest {
      resting: true,
      until: Some(1_700_003_600),
      kind: Some("limit".to_owned())
    }
  );
  assert!(rest.for_limit());
  assert_eq!(stub.taken().iter().find(|taken| taken.path == "/rest").unwrap().method, "GET");

  let reload = Reload {
    agent: AGENT.to_owned(),
    paths: vec!["knowledge/a.md".to_owned()],
    rooms: vec![ROOM],
  };
  assert!(agency.reload_documents(&reload).await.unwrap());
  assert_eq!(
    stub.body_at("/documents/reload"),
    json!({ "paths": ["knowledge/a.md"], "now": false, "agent": AGENT, "rooms": [ROOM] })
  );

  let query = |path: &str| stub.taken().into_iter().find(|taken| taken.path == path).unwrap().query;

  assert_eq!(
    agency.run_clock("bmo-people-digest").await.unwrap(),
    RunClock {
      cap_ends: Some(1_700_000_000_000),
      kept_until: None,
    }
  );
  assert_eq!(query("/runs/clock").as_deref(), Some("run=bmo-people-digest"));
  assert_eq!(agency.editor_cap("people-digest", AGENT).await.unwrap(), 2_400);
  assert_eq!(query("/editors/cap"), Some(format!("workKey=people-digest&agent={AGENT}")));
  assert_eq!(
    agency.rulebooks(&["plain-writing", "self-preservation"]).await.unwrap(),
    [Rulebook {
      name: "plain-writing".to_owned(),
      text: "Write plainly.".to_owned(),
    }]
  );
  assert_eq!(query("/rulebooks").as_deref(), Some("names=plain-writing%2Cself-preservation"));
}

#[tokio::test]
async fn nobody_saying_which_runs_are_alive_answers_none() {
  let stub = Stub::start().await;
  stub.answer("/runs/alive", json!({ "answered": false, "alive": null }));

  let alive = HttpAgency::new(&stub.peer).alive(&["bmo-digest".to_owned()]).await.unwrap();

  assert_eq!(alive, None);
}

fn editor_turn() -> EditorTurn {
  EditorTurn {
    session: "people-digest@editor".to_owned(),
    prompt: "Read the room.".to_owned(),
    run: "bmo-people-digest:people-digest:agent-a:-100:1".to_owned(),
    parent: Some("bmo-people-digest".to_owned()),
    agent: AGENT.to_owned(),
    chat_id: ROOM,
    step: Some(2),
    steps: Some(5),
    workdir: "agents/agent-a".to_owned(),
    desk: "/data/fragment-desks/a.json".to_owned(),
    registration: None,
  }
}

#[tokio::test]
async fn an_editor_turn_is_posted_and_its_end_read_off_the_event_stream() {
  let stub = Stub::start().await;
  let turn = editor_turn();
  let other = "bmo-people-digest:people-digest:agent-b:-200:1";
  stub.answer("/session/turn", json!({ "delivery": "taken" })).answer_events(
    "/events",
    "/session/turn",
    vec![
      (
        "turn-started",
        json!({ "session": "worker-x@worker", "run": "turn:worker-x:1", "at": 1 }),
      ),
      ("turn-started", json!({ "session": "people-digest@editor", "run": other, "at": 2 })),
      ("turn-started", json!({ "session": "people-digest@editor", "run": turn.run, "at": 3 })),
      (
        "turn-ended",
        json!({ "session": "people-digest@editor", "run": other, "outcome": "failed" }),
      ),
      (
        "turn-ended",
        json!({ "session": "people-digest@editor", "run": turn.run, "outcome": "answered",
          "answer": "All folded in. DONE" }),
      ),
    ],
  );
  let agency = HttpAgency::new(&stub.peer);
  let (started, heard) = oneshot::channel();

  let end = agency.editor_turn(&turn, started).await.unwrap();

  assert_eq!(heard.await.unwrap(), turn.run, "another turn on the same session is not this one");
  assert_eq!(
    end,
    TurnEnd {
      run: turn.run.clone(),
      outcome: TurnOutcome::Answered,
      answer: Some("All folded in. DONE".to_owned()),
      reason: None,
    }
  );
  let opened = stub.taken().into_iter().find(|taken| taken.path == "/events").unwrap();
  assert_eq!(opened.method, "GET");
  assert_eq!(opened.query.as_deref(), Some("kinds=turn-started%2Cturn-ended"));
  assert_eq!(
    stub.body_at("/session/turn"),
    json!({ "session": "people-digest@editor", "prompt": "Read the room.",
      "run": "bmo-people-digest:people-digest:agent-a:-100:1", "parent": "bmo-people-digest", "agent": AGENT,
      "chatId": ROOM, "step": 2, "steps": 5, "workdir": "agents/agent-a", "desk": "/data/fragment-desks/a.json",
      "registration": null })
  );
}

#[tokio::test]
async fn a_turn_the_agency_names_by_the_brains_run_is_read_by_that_name() {
  let stub = Stub::start().await;
  let turn = editor_turn();
  stub.answer("/session/turn", json!({ "delivery": "taken" })).answer_events(
    "/events",
    "/session/turn",
    vec![
      ("turn-started", json!({ "session": "people-digest@editor", "run": turn.run, "at": 2 })),
      (
        "turn-ended",
        json!({ "session": "people-digest@editor", "run": turn.run, "outcome": "failed",
          "reason": "no login answered" }),
      ),
    ],
  );
  let (started, _heard) = oneshot::channel();

  let end = HttpAgency::new(&stub.peer).editor_turn(&turn, started).await.unwrap();

  assert_eq!((end.run.as_str(), end.outcome), (turn.run.as_str(), TurnOutcome::Failed));
  assert_eq!(end.reason.as_deref(), Some("no login answered"));
}

#[tokio::test]
async fn a_session_carrying_another_turn_is_read_as_busy() {
  let stub = Stub::start().await;
  stub
    .answer_with(
      "/session/turn",
      StatusCode::CONFLICT,
      json!({ "code": "session-busy", "error": "people-digest@editor is in a turn" }),
    )
    .answer_events("/events", "/never", vec![]);
  let (started, _heard) = oneshot::channel();

  let fault = HttpAgency::new(&stub.peer).editor_turn(&editor_turn(), started).await.unwrap_err();

  assert_eq!(fault, TurnFault::Busy);
}

#[tokio::test]
async fn the_job_client_reads_item_states_and_no_route_as_none_finished() {
  let stub = Stub::start().await;
  stub.answer(
    "/todos/states",
    json!({ "items": [{ "id": 3, "done": true }, { "id": 4, "done": false }] }),
  );

  let states = HttpJob::new(&stub.peer).item_states(&[3, 4]).await.unwrap();

  assert_eq!(states, [ItemState { id: 3, done: true }, ItemState { id: 4, done: false }]);
  assert_eq!(stub.body_at("/todos/states"), json!({ "items": [3, 4] }));

  let unserved = Stub::start().await;
  let states = HttpJob::new(&unserved.peer).item_states(&[5]).await.unwrap();

  assert_eq!(states, [ItemState { id: 5, done: false }]);
  assert_eq!(unserved.taken().len(), 1);
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
    .answer("/instructions/quiet", json!({ "answered": true, "ok": true }))
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

  let quiet = QuietInstruction {
    agent: AGENT.to_owned(),
    chat_id: ROOM,
    text: "Watch #4 fired.".to_owned(),
    kind: Some("watch".to_owned()),
  };
  assert!(receiver.quiet_instruction(&quiet).await.unwrap());
  assert_eq!(
    stub.body_at("/instructions/quiet"),
    json!({ "agent": AGENT, "chatId": ROOM, "text": "Watch #4 fired.", "kind": "watch" })
  );

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
    "/runs/alive",
    StatusCode::CONFLICT,
    json!({ "code": "not-ready", "error": "The service is not ready." }),
  );

  let failure = HttpAgency::new(&stub.peer).alive(&["run-1".to_owned()]).await.unwrap_err();

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

  let failure = HttpAgency::new(&stub.peer).rest().await.unwrap_err();

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
