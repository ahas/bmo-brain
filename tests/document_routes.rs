//! The brain's documents as a turn's gate reads them, and what a session holds of them (docs/boundary-api.md,
//! 「Brain documents」 and 「Session document loads」), served on a local port: the knowledge chunks and person-book
//! pieces a room may be offered read whole and versioned, any other refused, the standing sheets of the turn's
//! senders under their ceilings, and a session's loads kept, read, swept and forgotten.

mod support;

use bmo_brain::api::documents::version_of;
use bmo_brain::editors::fragments::store::{NewRow, now_seconds, write_row};
use bmo_brain::policy::BRAIN;
use serde_json::{Value, json};

use support::receiver::message;
use support::served::{Served, served};
use support::{AGENT, ROOM};

const OTHER_ROOM: i64 = -200;

impl Served {
  async fn row(
    &self,
    kind: &str,
    chat_id: Option<i64>,
    owner: Option<i64>,
    slug: &str,
    path: &str,
    body: Option<&str>,
  ) {
    write_row(
      self.scratch.db(),
      &NewRow {
        kind: kind.into(),
        chat_id,
        owner,
        agent: AGENT.into(),
        slug: slug.into(),
        title: format!("about {slug}"),
        path: path.into(),
        ..NewRow::default()
      },
      1,
    )
    .await
    .unwrap();

    if let Some(body) = body {
      let at = self.root().join(path);

      std::fs::create_dir_all(at.parent().unwrap()).unwrap();
      std::fs::write(at, body).unwrap();
    }
  }

  fn put(&self, path: &str, text: &str) {
    let at = self.root().join(path);

    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, text).unwrap();
  }
}

fn refused(answer: &Value) -> (&str, Value) {
  assert_eq!(answer["answered"], json!(false), "{answer}");
  (answer["refusal"]["code"].as_str().unwrap(), answer["refusal"]["slugs"].clone())
}

#[tokio::test]
async fn the_knowledge_a_room_may_be_offered_is_read_whole_and_any_other_refused() {
  let api = served().await;

  api
    .row(
      "library-knowledge",
      Some(ROOM),
      None,
      "deploy",
      "knowledge/deploy.md",
      Some("\n the order of a deploy \n"),
    )
    .await;
  api
    .row(
      "library-knowledge",
      None,
      None,
      "greeting",
      "knowledge/greeting.md",
      Some("how to greet"),
    )
    .await;
  api
    .row(
      "library-knowledge",
      Some(OTHER_ROOM),
      None,
      "elsewhere",
      "knowledge/elsewhere.md",
      Some("not here"),
    )
    .await;
  api.row("library-knowledge", Some(ROOM), None, "gone", "knowledge/gone.md", None).await;

  let read = api
    .post(
      "/knowledge/read",
      json!({ "agent": AGENT, "chatId": ROOM, "slugs": ["greeting", "deploy", "gone"] }),
    )
    .await;

  assert_eq!(
    read,
    json!({ "documents": [
      { "slug": "greeting", "path": "knowledge/greeting.md", "text": "how to greet", "version": version_of("how to greet") },
      { "slug": "deploy", "path": "knowledge/deploy.md", "text": "the order of a deploy", "version": version_of("the order of a deploy") },
    ] })
  );

  let outside = api
    .post(
      "/knowledge/read",
      json!({ "agent": AGENT, "chatId": ROOM, "slugs": ["deploy", "elsewhere", "nothing"] }),
    )
    .await;

  assert_eq!(refused(&outside), ("out-of-room", json!(["elsewhere", "nothing"])));
  // Another agent is offered nothing of this agent's own.
  assert_eq!(
    refused(
      &api
        .post("/knowledge/read", json!({ "agent": "agent-b", "chatId": ROOM, "slugs": ["deploy"] }))
        .await
    )
    .0,
    "out-of-room"
  );
}

#[tokio::test]
async fn the_senders_sheets_and_the_pieces_a_room_may_be_offered_are_read() {
  let api = served().await;
  let over = "x".repeat(BRAIN.sheets.common_ceiling + 1);

  api.put("people/7/sheet.md", "  seven, in short  ");
  api.put(&format!("rooms/{ROOM}/people/7.md"), "seven in this room");
  api.put("people/8/sheet.md", &over);
  api
    .row(
      "person-book",
      Some(ROOM),
      None,
      "p7-habits",
      &format!("rooms/{ROOM}/people/chunks/p7-habits.md"),
      Some("habits here"),
    )
    .await;
  api
    .row(
      "person-book",
      None,
      Some(7),
      "c7-work",
      "people/7/chunks/c7-work.md",
      Some("what seven does"),
    )
    .await;
  api
    .row(
      "person-book",
      None,
      Some(9),
      "c9-work",
      "people/9/chunks/c9-work.md",
      Some("what nine does"),
    )
    .await;
  // Seven spoke in the room; nine never did.
  api.receiver.messages.lock().unwrap().push(message(ROOM, 1, 7, 1));

  let read = api
    .post(
      "/people/read",
      json!({ "agent": AGENT, "chatId": ROOM, "senders": [7, 7, 8], "slugs": ["c7-work", "p7-habits"] }),
    )
    .await;

  assert_eq!(
    read["sheets"],
    json!([
      { "person": 7, "part": "common", "path": "people/7/sheet.md", "text": "seven, in short", "version": version_of("seven, in short") },
      { "person": 7, "part": "room", "path": format!("rooms/{ROOM}/people/7.md"), "text": "seven in this room", "version": version_of("seven in this room") },
    ])
  );
  assert_eq!(read["pieces"][0]["slug"], json!("c7-work"));
  assert_eq!(read["pieces"][0]["owner"], json!(7));
  assert_eq!(read["pieces"][1]["text"], json!("habits here"));
  assert_eq!(
    refused(
      &api
        .post(
          "/people/read",
          json!({ "agent": AGENT, "chatId": ROOM, "senders": [], "slugs": ["c9-work"] })
        )
        .await
    ),
    ("out-of-room", json!(["c9-work"]))
  );

  // An agent keeping no long-term memory is given no sheet.
  let none = api
    .post(
      "/people/read",
      json!({ "agent": "agent-z", "chatId": ROOM, "senders": [7], "slugs": [] }),
    )
    .await;

  assert_eq!(none, json!({ "sheets": [], "pieces": [] }));
}

#[tokio::test]
async fn a_one_to_one_reads_no_room_sheet() {
  let api = served().await;

  api.put("people/7/sheet.md", "seven");
  api.put(&format!("rooms/{ROOM}/people/7.md"), "seven in this room");
  api.receiver.roads.lock().unwrap().insert(
    ROOM,
    bmo_brain::ports::receiver::Roads {
      kind: bmo_brain::ports::receiver::RoadKind::Owner,
      one_to_one: true,
    },
  );

  let read = api
    .post("/people/read", json!({ "agent": AGENT, "chatId": ROOM, "senders": [7] }))
    .await;

  assert_eq!(read["sheets"].as_array().unwrap().len(), 1);
  assert_eq!(read["sheets"][0]["part"], json!("common"));
}

#[tokio::test]
async fn a_sessions_loads_are_kept_read_swept_after_a_week_and_forgotten() {
  let api = served().await;
  let now = now_seconds();
  let keep = |session: &str, loads: Value, at: i64| json!({ "session": session, "loads": loads, "at": at });

  assert_eq!(
    api
      .post(
        "/sessions/loads/keep",
        keep("old", json!([{ "path": "knowledge/a.md", "version": "1" }]), now - 8 * 86_400)
      )
      .await,
    json!({ "ok": true })
  );
  assert_eq!(
    api.post("/sessions/loads", json!({ "session": "old" })).await["loads"]
      .as_array()
      .unwrap()
      .len(),
    1
  );

  api
    .post(
      "/sessions/loads/keep",
      keep("s1", json!([{ "path": "people/7/sheet.md", "version": "2" }, { "path": "knowledge/a.md", "version": "1" }]), now),
    )
    .await;
  api
    .post(
      "/sessions/loads/keep",
      keep("s1", json!([{ "path": "knowledge/a.md", "version": "3" }]), now),
    )
    .await;

  assert_eq!(
    api.post("/sessions/loads", json!({ "session": "s1" })).await,
    json!({ "loads": [{ "path": "knowledge/a.md", "version": "3" }, { "path": "people/7/sheet.md", "version": "2" }] })
  );
  // The session untouched for a week went at the same stroke.
  assert_eq!(api.post("/sessions/loads", json!({ "session": "old" })).await, json!({ "loads": [] }));
  assert_eq!(
    api.post("/sessions/loads/forget", json!({ "session": "s1" })).await,
    json!({ "ok": true })
  );
  assert_eq!(api.post("/sessions/loads", json!({ "session": "s1" })).await, json!({ "loads": [] }));
}
