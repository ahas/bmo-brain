//! The fragment table's routes as a person's tool calls them (docs/fragments.md, 「Writing a row by hand」 and 「The
//! drift check」; docs/boundary-api.md, 「Fragments」), served on a local port over a brain and a database of the
//! test's own: the rows listed and narrowed, the brain's libraries reconciled with their index files drawn, the table
//! checked against what stands on disk, and a piece's row written and taken down by hand.

mod support;

use std::path::Path;

use bmo_brain::editors::fragments::store::{NewRow, write_row};
use serde_json::{Value, json};

use support::served::{Served, served};
use support::{AGENT, OWNER, ROOM};

fn put(path: &Path, text: &str) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, text).unwrap();
}

fn rule_path(slug: &str) -> String {
  format!("rooms/{ROOM}/rules/{slug}.md")
}

impl Served {
  // A brain holding one room rule, and its row as a digest's editor would have handed it in.
  async fn greeting(&self) {
    put(
      &self.root().join(rule_path("greeting")),
      "- [date 2026-09-01 · confirmed 2026-09-02] greet, as #758 says\n",
    );
    write_row(
      self.scratch.db(),
      &NewRow {
        kind: "room-rule".into(),
        chat_id: Some(ROOM),
        agent: AGENT.into(),
        slug: "greeting".into(),
        title: "how the room is greeted".into(),
        path: rule_path("greeting"),
        keywords: vec!["greet".into()],
        ..NewRow::default()
      },
      1,
    )
    .await
    .unwrap();
  }

  // The rows a list answers, by slug.
  async fn listed(&self, query: &str) -> Vec<Value> {
    let (status, answer) = self.get(&format!("/fragments{query}")).await;

    assert_eq!(status, 200, "{answer}");
    answer["fragments"].as_array().unwrap().clone()
  }

  async fn slugs(&self, query: &str) -> Vec<String> {
    self
      .listed(query)
      .await
      .iter()
      .map(|row| row["slug"].as_str().unwrap().to_owned())
      .collect()
  }

  async fn lines(&self, path: &str, body: Value) -> Vec<String> {
    let answer = self.post(path, body).await;

    assert_eq!(answer["answered"], json!(true), "{answer}");
    answer["lines"]
      .as_array()
      .unwrap()
      .iter()
      .map(|line| line.as_str().unwrap().to_owned())
      .collect()
  }

  async fn active(&self, slug: &str) -> i64 {
    self
      .scratch
      .count(
        r#"SELECT COALESCE(MAX("active"), -1) FROM "fragments" WHERE "slug" = $1"#,
        vec![slug.into()],
      )
      .await
  }
}

#[tokio::test]
async fn the_rows_are_listed_before_a_reconcile_and_the_reconcile_reads_the_brain_and_draws_its_indexes() {
  let api = served().await;

  api.greeting().await;
  assert_eq!(api.slugs("").await, ["greeting"]);

  let synced = api.lines("/fragments/sync", json!({})).await;

  assert!(
    synced.iter().any(|line| line
      .starts_with("room-rule: 1 entries · 0 added · 0 updated · 0 raised · 0 taken down · 1 bodies read")),
    "{synced:?}"
  );
  // A person running the reconcile holds the brain, so the written libraries' index files are drawn.
  assert!(synced.contains(&format!("drawn: rooms/{ROOM}/rules/index.md")), "{synced:?}");
  assert!(
    std::fs::read_to_string(api.root().join(format!("rooms/{ROOM}/rules/index.md")))
      .unwrap()
      .contains("- greeting — how the room is greeted")
  );

  let rows = api.listed("").await;

  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0]["kind"], json!("room-rule"));
  assert_eq!(rows[0]["chatId"], json!(ROOM));
  assert_eq!(rows[0]["path"], json!(rule_path("greeting")));
  assert_eq!(rows[0]["keywords"], json!(["greet"]));
  assert_eq!(rows[0]["items"], json!([758]));
  assert_eq!(rows[0]["status"], json!("open"));
  assert_eq!(rows[0]["hits"], json!(0));
}

#[tokio::test]
async fn a_row_a_person_set_says_so_and_the_reconcile_leaves_it_where_they_put_it() {
  let api = served().await;

  api.greeting().await;
  api.lines("/fragments/sync", json!({})).await;
  api
    .scratch
    .run(
      r#"UPDATE "fragments" SET "active" = 0, "hand" = 0, "hand_at" = 1700000000 WHERE "slug" = 'greeting'"#,
      vec![],
    )
    .await;

  // A down row is listed only when it is asked for.
  assert!(api.slugs("").await.is_empty());

  let held = api.listed("?down=true&kind=room-rule").await;

  assert_eq!(held[0]["active"], json!(false));
  assert_eq!(held[0]["hand"], json!(false));
  assert_eq!(held[0]["handAt"], json!(1_700_000_000));

  let synced = api.lines("/fragments/sync", json!({})).await;

  assert!(
    synced
      .iter()
      .any(|line| line.starts_with("room-rule: 0 entries · 0 added · 0 updated · 0 raised · 0 taken down")),
    "{synced:?}"
  );
  assert_eq!(api.active("greeting").await, 0);
  // Nor does the check name it: a person took it down with its file standing.
  assert_eq!(api.post("/fragments/check", json!({})).await["drift"], json!([]));
}

#[tokio::test]
async fn a_kind_a_room_an_item_a_word_and_a_person_narrow_the_list() {
  let api = served().await;

  api.greeting().await;
  api.lines("/fragments/sync", json!({})).await;

  assert_eq!(api.slugs("?kind=room-rule").await, ["greeting"]);
  assert!(api.slugs("?kind=person-book").await.is_empty());
  assert_eq!(api.slugs(&format!("?chat={ROOM}")).await, ["greeting"]);
  assert!(api.slugs("?chat=-999").await.is_empty());
  assert_eq!(api.slugs("?item=758").await, ["greeting"]);
  assert!(api.slugs("?item=759").await.is_empty());
  assert_eq!(api.slugs("?text=greet").await, ["greeting"]);
  assert!(api.slugs("?text=farewell").await.is_empty());
  assert!(api.slugs(&format!("?person={OWNER}")).await.is_empty());

  // A number that is none and a kind there is none of are the wire's 400.
  let (status, answer) = api.get("/fragments?item=x").await;

  assert_eq!((status, answer["code"].as_str()), (400, Some("bad-request")));

  let (status, answer) = api.get("/fragments?kind=rulebook").await;

  assert_eq!(status, 400);
  assert!(
    answer["error"].as_str().unwrap().contains("rulebook is no kind of fragment"),
    "{answer}"
  );

  let kinds = api.get("/fragments/kinds").await.1;

  assert!(
    kinds["kinds"].as_array().unwrap().iter().any(|kind| kind["kind"] == json!("room-rule")),
    "{kinds}"
  );
}

#[tokio::test]
async fn the_check_says_nothing_is_out_of_step_and_names_what_is_when_the_brain_moves() {
  let api = served().await;

  api.greeting().await;
  api.lines("/fragments/sync", json!({})).await;

  let agreed = api.post("/fragments/check", json!({})).await;

  assert_eq!(agreed["drift"], json!([]), "{agreed}");
  // A standing row is held to the title shape only when written again, so the check counts it.
  assert_eq!(
    agreed["counted"],
    json!([
      { "kind": "room-rule", "refused": 1, "of": 1 },
      { "kind": "library-knowledge", "refused": 0, "of": 0 },
      { "kind": "person-book", "refused": 0, "of": 0 },
    ])
  );
  assert_eq!(agreed["titles"][0]["slug"], json!("greeting"));
  assert!(
    agreed["titles"][0]["problem"]
      .as_str()
      .unwrap()
      .contains("is too short to say what the piece is and when it is needed"),
    "{agreed}"
  );
  assert_eq!(
    api.post("/fragments/check", json!({ "kind": "person-book" })).await["counted"],
    json!([{ "kind": "person-book", "refused": 0, "of": 0 }])
  );

  // A body changed since the table read it, and a piece with no row.
  put(
    &api.root().join(rule_path("greeting")),
    "- [date 2026-09-01 · confirmed 2026-09-05] greet warmly\n",
  );
  put(
    &api.root().join(rule_path("stray")),
    "- [date 2026-09-03 · confirmed 2026-09-03] a stray rule\n",
  );

  let drift = api.post("/fragments/check", json!({})).await["drift"].clone();
  let troubles: Vec<(&str, &str)> = drift
    .as_array()
    .unwrap()
    .iter()
    .map(|one| (one["trouble"].as_str().unwrap(), one["slug"].as_str().unwrap()))
    .collect();

  assert_eq!(troubles, [("moved", "greeting"), ("unlisted", "stray")], "{drift}");
  assert_eq!(drift[1]["chatId"], json!(ROOM));

  // A row whose file is gone is bodiless.
  std::fs::remove_file(api.root().join(rule_path("greeting"))).unwrap();

  let gone = api.post("/fragments/check", json!({})).await["drift"].clone();

  assert_eq!(gone[0]["trouble"], json!("bodiless"), "{gone}");
  assert_eq!(gone[0]["said"], json!(format!("no file at {}", rule_path("greeting"))));
}

#[tokio::test]
async fn a_row_whose_piece_is_gone_is_taken_down_by_the_reconcile_and_listed_only_when_asked_for() {
  let api = served().await;

  api.greeting().await;
  api.lines("/fragments/sync", json!({})).await;
  std::fs::remove_file(api.root().join(rule_path("greeting"))).unwrap();

  let synced = api.lines("/fragments/sync", json!({})).await;

  assert!(
    synced
      .iter()
      .any(|line| line.starts_with("room-rule: 0 entries · 0 added · 0 updated · 0 raised · 1 taken down")),
    "{synced:?}"
  );
  assert!(api.slugs("?kind=room-rule").await.is_empty());
  assert_eq!(api.slugs("?kind=room-rule&down=true").await, ["greeting"]);
}

#[tokio::test]
async fn a_piece_written_by_hand_gets_its_row_held_to_the_same_checks_and_its_index_is_drawn() {
  let api = served().await;
  let path = rule_path("farewell");

  put(&api.root().join(&path), "- [date 2026-09-03 · confirmed 2026-09-04] say bye\n");

  let short = api.post("/fragments/write", json!({ "path": path, "title": "farewell, bye" })).await;

  assert_eq!(short["refusal"]["code"], json!("row-refused"), "{short}");
  assert!(
    short["refusal"]["reasons"][0]
      .as_str()
      .unwrap()
      .contains("is too short to say what the piece is and when it is needed"),
    "{short}"
  );
  assert_eq!(api.active("farewell").await, -1);

  let title =
    "A rule this room agreed on for saying goodbye; needed when somebody leaves the room or asks how to";
  let written = api
    .lines(
      "/fragments/write",
      json!({ "path": path, "title": title, "keywords": ["parting", " goodbye ", ""], "items": [758] }),
    )
    .await;

  assert_eq!(written.len(), 1);
  assert!(written[0].contains(&format!("farewell — {title}")), "{written:?}");
  assert!(written[0].ends_with(&format!("[{path}]")), "{written:?}");

  let rows = api.listed("?kind=room-rule").await;

  assert_eq!(rows[0]["slug"], json!("farewell"));
  assert_eq!(rows[0]["chatId"], json!(ROOM));
  assert_eq!(rows[0]["keywords"], json!(["parting", "goodbye"]));
  assert_eq!(rows[0]["items"], json!([758]));
  assert_eq!(rows[0]["hand"], json!(null));
  assert!(
    std::fs::read_to_string(api.root().join(format!("rooms/{ROOM}/rules/index.md")))
      .unwrap()
      .contains("- farewell — A rule")
  );

  // A row is taken down by hand only once its file is gone.
  let stands = api.post("/fragments/remove", json!({ "path": path })).await;

  assert_eq!(stands["refusal"]["code"], json!("body-stands"), "{stands}");
  assert_eq!(stands["refusal"]["path"], json!(path));
  assert_eq!(api.active("farewell").await, 1);

  std::fs::remove_file(api.root().join(&path)).unwrap();
  assert_eq!(
    api.lines("/fragments/remove", json!({ "path": path })).await,
    [format!("taken down: {path}")]
  );
  assert_eq!(api.active("farewell").await, 0);

  // A write with no title is the wire's 400.
  let untitled = api
    .caller
    .post::<Value>("/fragments/write", &json!({ "path": path, "title": " " }))
    .await;

  assert!(
    matches!(untitled, Err(bmo_brain::ports::http::PortFailure::Answered { status: 400, .. })),
    "{untitled:?}"
  );
}
