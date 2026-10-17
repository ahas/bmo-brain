//! The fragment tool's routes as a run's tool calls them (docs/boundary-api.md, 「Fragments」 and 「Fragment
//! refusals」): a desk opened as an editor batch opens one, over the served brain and its database, each call
//! reading it afresh, a write saving it whole and a refusal leaving it as it stood, and the table untouched.

mod support;

use std::path::{Path, PathBuf};

use bmo_brain::api::desk::desk_named;
use bmo_brain::editors::DocumentBackup;
use bmo_brain::editors::desk::{DeskScope, close_desk, desk_dir, open_desk};
use bmo_brain::editors::fragments::libraries::{AgentFolder, Brain};
use bmo_brain::editors::fragments::names;
use bmo_brain::editors::fragments::store::{NewRow, of_kind, write_row};
use bmo_brain::ports::http::PortFailure;
use fragment_desk::desk::Desk;
use serde_json::{Value, json};

use support::served::{Served, served};
use support::{AGENT, ROOM};

const STANDING: &str = "A note on the order of a deploy, read when a deploy has to be run again.";
const GREETING: &str = "- [date 2026-09-01 · confirmed 2026-09-02] hello\n";

fn put(root: &Path, path: &str, text: &str) {
  let at = root.join(path);

  std::fs::create_dir_all(at.parent().unwrap()).unwrap();
  std::fs::write(at, text).unwrap();
}

impl Served {
  // A brain with one knowledge chunk of the chat agent's in the table, a new chunk on disk with no row yet, and a
  // desk for a run of the chat agent's in the room, opened as the batch opens it.
  async fn desk(&self) -> PathBuf {
    put(self.root(), "knowledge/greeting.md", GREETING);
    put(
      self.root(),
      "knowledge/deploy.md",
      "- [date 2026-10-03 · confirmed 2026-10-05] #758 the order\n",
    );
    write_row(
      self.scratch.db(),
      &NewRow {
        kind: "library-knowledge".into(),
        chat_id: Some(ROOM),
        agent: AGENT.into(),
        slug: "greeting".into(),
        title: "How the room is greeted, read when somebody new arrives.".into(),
        path: "knowledge/greeting.md".into(),
        keywords: vec!["greeting".into(), "welcome".into()],
        items: vec![12],
        ..NewRow::default()
      },
      1,
    )
    .await
    .unwrap();

    let brain = Brain::new(
      self.root().to_path_buf(),
      AGENT,
      vec![AgentFolder {
        agent: AGENT.into(),
        folder: String::new(),
        label: Some("Helper".into()),
      }],
    );
    let before = DocumentBackup::from([("knowledge/greeting.md".to_owned(), GREETING.as_bytes().to_vec())]);
    let dir = desk_dir(self.data.path());

    std::fs::create_dir_all(&dir).unwrap();
    open_desk(
      &DeskScope {
        room: Some(ROOM),
        agent: Some(AGENT.into()),
        folder: String::new(),
        places: vec!["knowledge".into()],
        reach: vec![of_kind("library-knowledge")],
        ..DeskScope::default()
      },
      &dir,
      &self.scratch.address(),
      self.root(),
      &before,
      &names(&brain, AGENT, &["Owner".to_owned()]),
    )
    .unwrap()
  }

  async fn call(&self, path: &str, desk: &Path, mut body: Value) -> Value {
    body["desk"] = json!(desk.to_string_lossy());
    self.post(path, body).await
  }

  async fn status(&self, path: &str, desk: &Path, mut body: Value) -> u16 {
    body["desk"] = json!(desk.to_string_lossy());

    match self.caller.post::<Value>(path, &body).await {
      Err(PortFailure::Answered { status, .. }) => status,
      other => panic!("answered: {other:?}"),
    }
  }
}

fn lines(answer: &Value) -> Vec<String> {
  assert_eq!(answer["answered"], json!(true), "{answer}");
  answer["lines"]
    .as_array()
    .unwrap()
    .iter()
    .map(|line| line.as_str().unwrap().to_owned())
    .collect()
}

fn code(answer: &Value) -> &str {
  assert_eq!(answer["answered"], json!(false), "{answer}");
  answer["refusal"]["code"].as_str().unwrap()
}

fn write(path: &str, title: &str, keywords: &str) -> Value {
  json!({ "path": path, "title": title, "keywords": keywords, "items": [] })
}

#[tokio::test]
async fn a_run_finds_its_rows_writes_a_new_piece_and_owes_nothing_after() {
  let api = served().await;
  let desk = api.desk().await;
  let found = lines(&api.call("/fragments/desk/search", &desk, json!({})).await);

  assert_eq!(found.len(), 1, "{found:?}");
  assert!(found[0].starts_with("- greeting"), "{}", found[0]);
  assert!(
    found[0].contains("(path knowledge/greeting.md · keywords greeting, welcome · items #12)"),
    "{}",
    found[0]
  );
  assert_eq!(
    lines(&api.call("/fragments/desk/search", &desk, json!({ "text": "nothing-like-it" })).await),
    ["No row matches."]
  );
  assert_eq!(
    lines(&api.call("/fragments/desk/owed", &desk, json!({})).await),
    ["These pieces have no row written in this run yet:", "knowledge/deploy.md"]
  );

  let written = lines(
    &api
      .call("/fragments/desk/write", &desk, write("./knowledge/deploy.md", STANDING, "deploy-order"))
      .await,
  );

  assert_eq!(written.len(), 1);
  assert!(
    written[0].starts_with("- deploy") && written[0].ends_with("[written in this run]"),
    "{}",
    written[0]
  );
  assert_eq!(
    lines(&api.call("/fragments/desk/owed", &desk, json!({})).await),
    ["Nothing is owed: every piece this run touched has its row."]
  );

  // The write is on the desk, and nothing reached the table, which the batch writes once the run ends.
  let saved = Desk::read(&desk).unwrap();

  assert_eq!(saved.writes.len(), 1);
  assert_eq!(saved.writes[0].path, "knowledge/deploy.md");
  assert_eq!(saved.writes[0].keywords, ["deploy-order"]);
  assert_eq!(api.scratch.count(r#"SELECT COUNT(*) FROM "fragments""#, vec![]).await, 1);

  // A piece taken away goes from disk and from the desk's view of the rows.
  let removed = lines(
    &api
      .call("/fragments/desk/remove", &desk, json!({ "path": "knowledge/deploy.md" }))
      .await,
  );

  assert!(!removed.is_empty());
  assert!(!api.root().join("knowledge/deploy.md").exists());
  assert!(Desk::read(&desk).unwrap().writes.iter().any(|write| write.removed));
}

#[tokio::test]
async fn every_write_that_cannot_stand_is_refused_by_its_code_and_leaves_the_desk_as_it_was() {
  let api = served().await;
  let desk = api.desk().await;

  for (body, refused) in [
    (write("knowledge/deploy.md", "Too short.", "deploy-order"), "title-shape"),
    (write("rooms/-100/rules/x.md", STANDING, "deploy-order"), "out-of-place"),
    (write("../outside.md", STANDING, "deploy-order"), "out-of-place"),
    (write("knowledge/missing.md", STANDING, "deploy-order"), "no-body"),
    // The agent's label and the owner's names are common words, which a keyword cannot hang on.
    (write("knowledge/deploy.md", STANDING, "deploy-order, Helper"), "keywords"),
    (write("knowledge/deploy.md", STANDING, "deploy-order, owner"), "keywords"),
    (write("knowledge/deploy.md", STANDING, "greeting, welcome"), "repeats"),
  ] {
    let answer = api.call("/fragments/desk/write", &desk, body.clone()).await;

    assert_eq!(code(&answer), refused, "{body}: {answer}");
  }

  put(
    api.root(),
    "knowledge/loose.md",
    "- [2026-10-03 · confirmed 2026-10-05] no slot names\n",
  );
  assert_eq!(
    code(
      &api
        .call("/fragments/desk/write", &desk, write("knowledge/loose.md", STANDING, "loose-one"))
        .await
    ),
    "unnamed-slots"
  );
  assert_eq!(
    code(
      &api
        .call("/fragments/desk/remove", &desk, json!({ "path": "knowledge/nothing.md" }))
        .await
    ),
    "nothing-stands"
  );
  assert_eq!(
    code(
      &api
        .call(
          "/fragments/desk/split",
          &desk,
          json!({ "from": "knowledge/deploy.md", "pieces": [
            { "path": "knowledge/deploy.md", "title": STANDING, "keywords": "deploy-order", "items": [], "shared": null }
          ] }),
        )
        .await
    ),
    "bad-split"
  );
  assert!(Desk::read(&desk).unwrap().writes.is_empty());

  // A body of another shape is the wire's 400.
  for (route, body) in [
    (
      "/fragments/desk/write",
      json!({ "path": "knowledge/deploy.md", "title": STANDING, "keywords": "x", "items": [0] }),
    ),
    (
      "/fragments/desk/write",
      json!({ "path": "knowledge/deploy.md", "title": STANDING, "keywords": ["x"], "items": [] }),
    ),
    (
      "/fragments/desk/merge",
      json!({ "into": "knowledge/deploy.md", "from": [], "title": STANDING }),
    ),
    ("/fragments/desk/split", json!({ "from": "knowledge/deploy.md", "pieces": [] })),
    ("/fragments/desk/search", json!({ "status": "half" })),
  ] {
    assert_eq!(api.status(route, &desk, body.clone()).await, 400, "{route} {body}");
  }
}

#[tokio::test]
async fn a_desk_is_found_only_among_the_open_desks_under_the_data_root() {
  let api = served().await;
  let desk = api.desk().await;
  let elsewhere = api.data.path().join("passwd");

  std::fs::write(&elsewhere, "{}").unwrap();

  for named in [
    elsewhere.clone(),
    desk_dir(api.data.path()).join("0000.json"),
    desk_dir(api.data.path()).join(format!("{}.json", "a".repeat(32))),
    PathBuf::from(desk.file_name().unwrap()),
  ] {
    assert_eq!(
      code(&api.call("/fragments/desk/owed", &named, json!({})).await),
      "no-desk",
      "{}",
      named.display()
    );
    assert_eq!(desk_named(api.data.path(), &named.to_string_lossy()), None);
  }

  assert_eq!(desk_named(api.data.path(), &desk.to_string_lossy()), Some(desk.clone()));

  // Once the batch closes it, the desk is no desk.
  close_desk(&desk);
  assert_eq!(code(&api.call("/fragments/desk/search", &desk, json!({})).await), "no-desk");
}
