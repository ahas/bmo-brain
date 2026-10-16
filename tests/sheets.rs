//! The person book's standing sheets: which pieces a sheet is written from, what keeps a sheet from standing,
//! how a large book is folded, and what the gate's sheet judgement is asked.

mod support;

use std::fs;

use bmo_brain::editors::book::{KnownIdentifiers, PersonPiece};
use bmo_brain::editors::sheets::{
  Layer, PieceRow, SheetFor, fold_sheet, sheet_groups, sheet_problem, standing_pieces, write_common_sheet,
  write_standing_sheet,
};
use serde_json::json;

use support::AGENT;
use support::gate::FakeGate;

fn piece(path: &str, text: &str) -> PersonPiece {
  PersonPiece {
    path: path.into(),
    text: text.into(),
  }
}

#[test]
fn a_done_matter_is_left_to_the_search() {
  let pieces = vec![piece("a", "x"), piece("b", "y")];
  let rows = vec![PieceRow {
    path: "a".into(),
    status: Some("done".into()),
  }];

  assert_eq!(standing_pieces(pieces, &rows), vec![piece("b", "y")]);
}

#[test]
fn a_sheet_stands_only_inside_its_own_part_and_on_its_pieces_days() {
  let pieces = vec![piece("a", "- [2026-01-02 · confirmed 2026-01-03] likes tea")];

  assert_eq!(sheet_problem(Layer::Common, "", &pieces).as_deref(), Some("nothing"));
  assert_eq!(
    sheet_problem(Layer::Common, "## Address\nhi", &pieces).as_deref(),
    Some("a section of the other part (Address)")
  );
  assert_eq!(
    sheet_problem(Layer::Room, "## Personality\n- 2026-01-04", &pieces).as_deref(),
    Some("a section of the other part (Personality)")
  );
  assert_eq!(
    sheet_problem(Layer::Common, "## Personality\n- since 2026-01-04", &pieces).as_deref(),
    Some("a day none of its pieces names")
  );
  assert_eq!(sheet_problem(Layer::Common, "## Personality\n- 2026-01-02 tea", &pieces), None);

  let long = "x".repeat(Layer::Room.ceiling() + 1);

  assert!(sheet_problem(Layer::Room, &long, &pieces).unwrap().ends_with("ceiling"));
}

#[test]
fn groups_take_whole_pieces_under_the_budget() {
  let pieces = vec![piece("a", "aaaa"), piece("b", "bbbb"), piece("c", &"c".repeat(20)), piece("d", "d")];
  let groups = sheet_groups(&pieces, 10);

  assert_eq!(groups.iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 1, 1]);
}

#[tokio::test]
async fn a_group_that_gives_nothing_twice_keeps_the_sheet_that_is_there() {
  let calls = std::cell::Cell::new(0);
  let none = fold_sheet(Layer::Room, 7, &[piece("a", "x")], async |_: &[PersonPiece]| {
    calls.set(calls.get() + 1);
    None
  })
  .await;

  assert_eq!(none, None);
  assert_eq!(calls.get(), 2);

  let one = fold_sheet(Layer::Room, 7, &[piece("a", "x")], async |_: &[PersonPiece]| {
    Some("## Address\nhi".into())
  })
  .await;

  assert_eq!(one.as_deref(), Some("## Address\nhi\n"));
}

#[tokio::test]
async fn a_room_sheet_is_asked_of_the_gate_with_its_room_and_a_common_one_with_none() {
  let gate = FakeGate::default();

  gate.sheets.lock().unwrap().push_back(Some("  ## Address\n- since 2026-01-02  ".into()));

  let room = SheetFor {
    gate: &gate,
    layer: Layer::Room,
    chat_id: -5,
    user_id: 7,
    agent: AGENT,
    registration: Some("road-a"),
  };
  let pieces = [piece("rooms/-5/people/chunks/p7-a.md", "[2026-01-02] x")];

  assert_eq!(
    write_standing_sheet(&room, &pieces).await.as_deref(),
    Some("## Address\n- since 2026-01-02\n")
  );

  let common = SheetFor {
    layer: Layer::Common,
    ..room
  };

  assert_eq!(write_standing_sheet(&common, &pieces).await, None);

  let asked = gate.calls.of("judge_sheet");

  assert_eq!(asked.len(), 3);
  assert_eq!(
    asked[0],
    json!({
      "agent": AGENT,
      "part": "room",
      "chatId": -5,
      "person": 7,
      "pieces": [{ "path": "rooms/-5/people/chunks/p7-a.md", "text": "[2026-01-02] x" }],
      "ceiling": Layer::Room.ceiling(),
      "registration": "road-a",
    })
  );
  assert_eq!(asked[1]["part"], "common");
  assert_eq!(asked[1]["chatId"], json!(null));
}

#[tokio::test]
async fn a_common_sheet_that_names_somebody_else_is_not_written() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  fs::create_dir_all(root.join("people/7/chunks")).unwrap();
  fs::write(root.join("people/7/chunks/c7-a.md"), "[2026-01-02] likes tea").unwrap();

  let gate = FakeGate::default();
  let known = KnownIdentifiers {
    user_ids: vec![7, 9],
    chat_ids: vec![],
    titles: vec![],
  };
  let ask = SheetFor {
    gate: &gate,
    layer: Layer::Common,
    chat_id: -5,
    user_id: 7,
    agent: AGENT,
    registration: None,
  };

  gate.sheets.lock().unwrap().push_back(Some("## Personality\n- talks with 9".into()));
  assert!(!write_common_sheet(&ask, root, &known, &[]).await);
  assert!(!root.join("people/7/sheet.md").exists());

  gate.sheets.lock().unwrap().push_back(Some("## Personality\n- 2026-01-02 tea".into()));
  assert!(write_common_sheet(&ask, root, &known, &[]).await);
  assert_eq!(
    fs::read_to_string(root.join("people/7/sheet.md")).unwrap(),
    "## Personality\n- 2026-01-02 tea\n"
  );
}
