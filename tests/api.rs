//! The routes the gate reads a turn's raw material by, served on a local port and called as another unit calls
//! them: a room's standing rows with a stood-still state released, the matcher's candidates withered once a span,
//! what the matcher handed back, the rows a turn's events wake, the fragment rows a room may be offered and what a
//! turn attached, a forgotten room, and the wire's own answers.

mod support;

use std::sync::Arc;

use bmo_brain::api::memories::Withering;
use bmo_brain::api::{Api, router};
use bmo_brain::editors::fragments::store::now_seconds;
use bmo_brain::ports::http::{Caller, Peer, PortFailure};
use bmo_brain::ports::receiver::{RoadKind, Roads};
use sea_orm::Value as Sql;
use serde_json::{Value, json};

use support::receiver::{FakeReceiver, message};
use support::scratch::Scratch;
use support::{AGENT, OTHER_AGENT, ROOM};

const TOKEN: &str = "brain-token";
const OTHER_ROOM: i64 = -200;
const DAY: i64 = 86_400;

struct Served {
  scratch: Scratch,
  receiver: Arc<FakeReceiver>,
  caller: Caller,
  base: String,
}

async fn served() -> Served {
  let scratch = Scratch::new().await;
  let receiver = Arc::new(FakeReceiver::default());
  let api = Arc::new(Api {
    db: scratch.db().clone(),
    receiver: receiver.clone(),
    token: TOKEN.to_owned(),
    withering: Withering::default(),
  });
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, router(api)).await.unwrap() });

  Served {
    caller: caller(&base, TOKEN),
    scratch,
    receiver,
    base,
  }
}

fn caller(base: &str, token: &str) -> Caller {
  Caller::new(&Peer {
    base: base.to_owned(),
    token: token.to_owned(),
  })
}

impl Served {
  async fn post(&self, path: &str, body: Value) -> Value {
    self.caller.post::<Value>(path, &body).await.unwrap()
  }

  // A memory row of an agent, open and up, set the given seconds ago.
  async fn memory(&self, agent: &str, slug: &str, chat_id: Option<i64>, kind: &str, more: Row<'_>) {
    let now = now_seconds();

    self
      .scratch
      .run(
        r#"INSERT INTO "memories" ("slug", "type", "chat_id", "reach", "condition", "action", "set_at", "active",
             "hits", "event", "event_met_at", "moved_at", "last_hit_at", "agent")
           VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 0, $8, $9, $10, $11, $12)"#,
        vec![
          slug.into(),
          kind.into(),
          chat_id.into(),
          more.reach.into(),
          more.condition.into(),
          format!("do {slug}").into(),
          (now - more.set_ago).into(),
          more.event.into(),
          more.met_ago.map(|ago| now - ago).into(),
          more.moved_ago.map(|ago| now - ago).into(),
          more.hit_ago.map(|ago| now - ago).into(),
          agent.into(),
        ],
      )
      .await;
  }

  async fn memory_column(&self, slug: &str, column: &str) -> Option<String> {
    let row = sea_orm::ConnectionTrait::query_one(
      self.scratch.db(),
      sea_orm::Statement::from_sql_and_values(
        sea_orm::DbBackend::Postgres,
        format!(r#"SELECT "{column}"::text AS "value" FROM "memories" WHERE "slug" = $1"#),
        vec![slug.into()],
      ),
    )
    .await
    .unwrap()
    .unwrap();

    row.try_get::<Option<String>>("", "value").unwrap()
  }

  // A fragment row, up unless said otherwise.
  async fn fragment(&self, kind: &str, row: Piece<'_>) {
    self
      .scratch
      .run(
        r#"INSERT INTO "fragments" ("kind", "chat_id", "owner", "slug", "title", "path", "position", "active", "hits",
             "set_at", "agent", "all_agents")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, 1, $9, $10)"#,
        vec![
          kind.into(),
          row.chat_id.into(),
          row.owner.into(),
          row.slug.into(),
          format!("about {}", row.slug).into(),
          format!("{}.md", row.slug).into(),
          row.position.into(),
          i64::from(row.active).into(),
          row.agent.into(),
          i64::from(row.all_agents).into(),
        ],
      )
      .await;
  }

  async fn hits_of(&self, slug: &str) -> i64 {
    self
      .scratch
      .count(r#"SELECT "hits" FROM "fragments" WHERE "slug" = $1"#, vec![Sql::from(slug)])
      .await
  }
}

struct Row<'a> {
  reach: &'a str,
  condition: &'a str,
  event: &'a str,
  set_ago: i64,
  met_ago: Option<i64>,
  moved_ago: Option<i64>,
  hit_ago: Option<i64>,
}

fn plain() -> Row<'static> {
  Row {
    reach: "all",
    condition: "",
    event: "",
    set_ago: 60,
    met_ago: None,
    moved_ago: None,
    hit_ago: None,
  }
}

struct Piece<'a> {
  chat_id: Option<i64>,
  owner: Option<i64>,
  slug: &'a str,
  position: i64,
  agent: &'a str,
  all_agents: bool,
  active: bool,
}

fn piece(slug: &str, chat_id: Option<i64>, position: i64) -> Piece<'_> {
  Piece {
    chat_id,
    owner: None,
    slug,
    position,
    agent: AGENT,
    all_agents: false,
    active: true,
  }
}

fn slugs(rows: &Value) -> Vec<String> {
  rows
    .as_array()
    .unwrap()
    .iter()
    .map(|row| row["slug"].as_str().unwrap().to_owned())
    .collect()
}

#[tokio::test]
async fn a_rooms_standing_rows_answer_whole_and_a_stood_still_state_is_released_first() {
  let api = served().await;

  api.memory(AGENT, "tea", Some(ROOM), "promise", plain()).await;
  api
    .memory(
      AGENT,
      "game",
      Some(ROOM),
      "state",
      Row {
        moved_ago: Some(13 * 3_600),
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "match",
      Some(ROOM),
      "promise",
      Row {
        moved_ago: Some(3_600),
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "if-rain",
      Some(ROOM),
      "situation",
      Row {
        condition: "when it rains",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "after-deploy",
      Some(ROOM),
      "check",
      Row {
        event: "deployed:773",
        met_ago: Some(60),
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "before-deploy",
      Some(ROOM),
      "check",
      Row {
        event: "deployed:774",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "on-deploy",
      Some(ROOM),
      "situation",
      Row {
        event: "deployed",
        met_ago: Some(60),
        ..plain()
      },
    )
    .await;
  api.memory(OTHER_AGENT, "theirs", Some(ROOM), "promise", plain()).await;
  api.memory(AGENT, "elsewhere", Some(OTHER_ROOM), "promise", plain()).await;

  let answer = api.post("/memory/standing", json!({ "agent": AGENT, "chatId": ROOM })).await;

  assert_eq!(slugs(&answer["rows"]), ["tea", "match", "after-deploy"]);
  assert_eq!(answer["rows"][0]["type"], json!("promise"));
  assert_eq!(answer["rows"][0]["action"], json!("do tea"));
  assert_eq!(answer["rows"][0]["chatId"], json!(ROOM));
  assert_eq!(answer["rows"][2]["event"], json!({ "kind": "deployed", "item": 773 }));
  assert_eq!(api.memory_column("game", "done_text").await.as_deref(), Some("stood-still hours=12"));
  assert!(api.memory_column("game", "done_at").await.is_some());
}

#[tokio::test]
async fn the_matchers_candidates_are_the_rooms_and_the_global_rows_reaching_it_withered_once_a_span() {
  let api = served().await;

  api
    .memory(
      AGENT,
      "late",
      Some(ROOM),
      "situation",
      Row {
        condition: "when late",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "stale",
      Some(ROOM),
      "situation",
      Row {
        condition: "when stale",
        set_ago: 61 * DAY,
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "kept-alive",
      Some(ROOM),
      "situation",
      Row {
        condition: "when kept",
        set_ago: 90 * DAY,
        hit_ago: Some(DAY),
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "everywhere",
      None,
      "situation",
      Row {
        condition: "when any",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "groups",
      None,
      "situation",
      Row {
        reach: "group",
        condition: "when grouped",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "owners",
      None,
      "situation",
      Row {
        reach: "owner",
        condition: "when owned",
        ..plain()
      },
    )
    .await;
  api.memory(AGENT, "plain", Some(ROOM), "promise", plain()).await;

  let asked = json!({ "agent": AGENT, "chatId": ROOM });
  let answer = api.post("/memory/conditions", asked.clone()).await;

  // The room's own rows oldest first, then the global ones.
  assert_eq!(slugs(&answer["rows"]), ["kept-alive", "late", "everywhere", "groups"]);
  assert_eq!(
    answer["rows"][1],
    json!({ "slug": "late", "condition": "when late", "action": "do late" })
  );
  assert_eq!(api.memory_column("stale", "down_reason").await.as_deref(), Some("withered"));

  // Within the span the room is not looked over again; once it is forgotten, it is.
  api
    .memory(
      AGENT,
      "older",
      Some(ROOM),
      "situation",
      Row {
        condition: "when older",
        set_ago: 70 * DAY,
        ..plain()
      },
    )
    .await;
  assert!(
    slugs(&api.post("/memory/conditions", asked.clone()).await["rows"]).contains(&"older".to_owned())
  );

  assert_eq!(api.post("/rooms/forgotten", json!({ "chatId": ROOM })).await, json!({ "ok": true }));
  assert!(
    !slugs(&api.post("/memory/conditions", asked.clone()).await["rows"]).contains(&"older".to_owned())
  );
  assert_eq!(api.memory_column("older", "down_reason").await.as_deref(), Some("withered"));

  // An owner's room is offered the rows reaching owners' rooms instead.
  api.receiver.roads.lock().unwrap().insert(
    OTHER_ROOM,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );
  assert_eq!(
    slugs(&api.post("/memory/conditions", json!({ "agent": AGENT, "chatId": OTHER_ROOM })).await["rows"]),
    ["everywhere", "owners"]
  );
}

#[tokio::test]
async fn what_the_matcher_hands_back_is_counted_and_a_spent_rule_taken_down_in_its_own_room_alone() {
  let api = served().await;

  api
    .memory(
      AGENT,
      "late",
      Some(ROOM),
      "situation",
      Row {
        condition: "when late",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "loud",
      Some(ROOM),
      "situation",
      Row {
        condition: "when loud",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "far",
      Some(OTHER_ROOM),
      "situation",
      Row {
        condition: "when far",
        ..plain()
      },
    )
    .await;

  let answer = api
    .post(
      "/memory/matched",
      json!({ "agent": AGENT, "chatId": ROOM, "matched": ["late"], "spent": ["loud", "far"] }),
    )
    .await;

  assert_eq!(answer, json!({ "ok": true }));
  assert_eq!(api.memory_column("late", "hits").await.as_deref(), Some("1"));
  assert!(api.memory_column("late", "last_hit_at").await.is_some());
  assert_eq!(api.memory_column("loud", "down_reason").await.as_deref(), Some("spent"));
  assert_eq!(api.memory_column("loud", "active").await.as_deref(), Some("0"));
  assert_eq!(api.memory_column("far", "active").await.as_deref(), Some("1"));
}

#[tokio::test]
async fn an_event_wakes_its_rows_by_itself_and_meets_every_errand_waiting_on_it() {
  let api = served().await;

  api
    .memory(
      AGENT,
      "on-deploy",
      Some(ROOM),
      "situation",
      Row {
        event: "deployed",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "after-773",
      Some(OTHER_ROOM),
      "check",
      Row {
        event: "deployed:773",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      OTHER_AGENT,
      "theirs-773",
      Some(ROOM),
      "check",
      Row {
        event: "deployed:773",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "any-deploy",
      None,
      "situation",
      Row {
        event: "deployed",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "owners-deploy",
      None,
      "situation",
      Row {
        reach: "owner",
        event: "deployed",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "after-774",
      Some(ROOM),
      "check",
      Row {
        event: "deployed:774",
        ..plain()
      },
    )
    .await;
  api
    .memory(
      AGENT,
      "on-queue",
      Some(ROOM),
      "situation",
      Row {
        event: "queue-changed",
        ..plain()
      },
    )
    .await;

  let answer = api
    .post(
      "/memory/events",
      json!({ "agent": AGENT, "chatId": ROOM, "events": [{ "kind": "deployed", "item": 773 }] }),
    )
    .await;

  assert_eq!(slugs(&answer["rows"]), ["on-deploy", "any-deploy"]);
  assert_eq!(api.memory_column("on-deploy", "hits").await.as_deref(), Some("1"));
  // Every errand the event wakes is met, wherever it waits and whoever's it is; a situation row never is.
  assert!(api.memory_column("after-773", "event_met_at").await.is_some());
  assert!(api.memory_column("theirs-773", "event_met_at").await.is_some());
  assert!(api.memory_column("on-deploy", "event_met_at").await.is_none());
  assert!(api.memory_column("after-774", "event_met_at").await.is_none());
  assert_eq!(
    api
      .post("/memory/events", json!({ "agent": AGENT, "chatId": ROOM, "events": [] }))
      .await,
    json!({ "rows": [] })
  );
}

#[tokio::test]
async fn a_room_is_offered_its_own_rows_the_shared_ones_and_the_books_of_those_who_spoke_there() {
  let api = served().await;

  *api.receiver.messages.lock().unwrap() = vec![message(ROOM, 1, 7, 0)];

  api.fragment("room-rule", piece("quiet", Some(ROOM), 0)).await;
  api.fragment("room-rule", piece("loud", Some(OTHER_ROOM), 0)).await;
  api.fragment("library-knowledge", piece("tea", Some(ROOM), 2)).await;
  api.fragment("library-knowledge", piece("deploys", None, 1)).await;
  api.fragment("library-knowledge", piece("far-off", Some(OTHER_ROOM), 0)).await;
  api
    .fragment(
      "library-knowledge",
      Piece {
        active: false,
        ..piece("down", None, 0)
      },
    )
    .await;
  api
    .fragment(
      "library-knowledge",
      Piece {
        agent: OTHER_AGENT,
        ..piece("theirs", None, 0)
      },
    )
    .await;
  api
    .fragment(
      "library-knowledge",
      Piece {
        agent: OTHER_AGENT,
        all_agents: true,
        ..piece("for-all", None, 3)
      },
    )
    .await;
  api
    .fragment(
      "person-book",
      Piece {
        owner: Some(7),
        ..piece("p7-address", Some(ROOM), 0)
      },
    )
    .await;
  api
    .fragment(
      "person-book",
      Piece {
        owner: Some(8),
        ..piece("c8-speech", None, 0)
      },
    )
    .await;
  api
    .fragment(
      "person-book",
      Piece {
        owner: Some(7),
        ..piece("c7-speech", None, 1)
      },
    )
    .await;
  api.fragment("system-document", piece("deploying", None, 0)).await;

  let kinds = json!(["room-rule", "library-knowledge", "person-book", "system-document"]);
  let answer = api
    .post("/fragments/offer", json!({ "agent": AGENT, "chatId": ROOM, "kinds": kinds }))
    .await;

  assert_eq!(
    slugs(&answer["fragments"]),
    ["quiet", "deploys", "tea", "for-all", "p7-address", "c7-speech", "deploying"]
  );
  assert_eq!(answer["fragments"][0]["kind"], json!("room-rule"));
  assert_eq!(answer["fragments"][0]["path"], json!("quiet.md"));
  assert_eq!(answer["fragments"][3]["allAgents"], json!(true));
  assert_eq!(answer["fragments"][5]["owner"], json!(7));

  // A one-to-one keeps no room part, and its book's room rows are offered to nobody.
  api.receiver.roads.lock().unwrap().insert(
    ROOM,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );

  let one_to_one = api
    .post("/fragments/offer", json!({ "agent": AGENT, "chatId": ROOM, "kinds": ["person-book"] }))
    .await;

  assert_eq!(slugs(&one_to_one["fragments"]), ["c7-speech"]);
}

#[tokio::test]
async fn what_a_turn_attached_is_counted_on_the_rows_its_agent_may_read() {
  let api = served().await;

  api.fragment("library-knowledge", piece("tea", Some(ROOM), 0)).await;
  api
    .fragment(
      "library-knowledge",
      Piece {
        agent: OTHER_AGENT,
        all_agents: true,
        ..piece("for-all", None, 0)
      },
    )
    .await;
  api
    .fragment(
      "library-knowledge",
      Piece {
        agent: OTHER_AGENT,
        ..piece("theirs", None, 0)
      },
    )
    .await;
  api
    .fragment(
      "library-knowledge",
      Piece {
        active: false,
        ..piece("down", None, 0)
      },
    )
    .await;

  let hit = |slug: &str, chat_id: Option<i64>| json!({ "kind": "library-knowledge", "chatId": chat_id, "owner": null, "slug": slug });
  let answer = api
    .post(
      "/fragments/hits",
      json!({
        "agent": AGENT,
        "chatId": ROOM,
        "attached": [hit("tea", Some(ROOM)), hit("for-all", None), hit("theirs", None), hit("down", None), hit("tea", None)],
      }),
    )
    .await;

  assert_eq!(answer, json!({ "ok": true }));
  assert_eq!(
    [
      api.hits_of("tea").await,
      api.hits_of("for-all").await,
      api.hits_of("theirs").await,
      api.hits_of("down").await
    ],
    [1, 1, 0, 0]
  );
}

fn refused(failure: PortFailure) -> (u16, String) {
  match failure {
    PortFailure::Answered {
      status,
      answer: Some(answer),
      ..
    } => (status, answer.code),
    other => panic!("not an error answer: {other:?}"),
  }
}

#[tokio::test]
async fn the_wire_refuses_a_wrong_token_a_body_of_another_shape_and_a_path_it_does_not_serve() {
  let api = served().await;
  let asked = json!({ "agent": AGENT, "chatId": ROOM });

  assert_eq!(
    refused(caller(&api.base, "wrong").post::<Value>("/memory/standing", &asked).await.unwrap_err()),
    (401, "unauthorized".to_owned())
  );
  assert_eq!(
    refused(
      api
        .caller
        .post::<Value>("/memory/standing", &json!({ "agent": AGENT }))
        .await
        .unwrap_err()
    ),
    (400, "bad-request".to_owned())
  );
  assert_eq!(
    refused(api.caller.post::<Value>("/memory/nothing", &asked).await.unwrap_err()),
    (404, "no-route".to_owned())
  );
}
