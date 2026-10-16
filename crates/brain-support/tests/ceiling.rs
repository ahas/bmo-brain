//! The answer ceiling's cut of a list, held to the cases in `fixtures/answer-cap-cases.json`, and the
//! thousands mark every count carries.

use brain_support::ceiling::{MAX_ANSWER_CHARS, Said, cap_items, count_text};
use serde::Deserialize;

const CASES: &str = include_str!("fixtures/answer-cap-cases.json");

#[derive(Deserialize)]
#[serde(untagged)]
enum Piece {
  Text(String),
  Repeated { text: String, times: usize },
}

impl Piece {
  fn text(&self) -> String {
    match self {
      Piece::Text(text) => text.clone(),
      Piece::Repeated { text, times } => text.repeat(*times),
    }
  }
}

#[derive(Deserialize)]
struct ItemCase {
  name: String,
  items: Vec<Piece>,
  copies: Option<usize>,
  limit: Option<usize>,
  render: String,
  unit: String,
  how: String,
  kept: usize,
  left: usize,
  note: Option<String>,
}

#[derive(Deserialize)]
struct Cases {
  items: Vec<ItemCase>,
}

fn render(kind: &str) -> impl Fn(&[String]) -> String {
  let json = kind == "json";

  move |kept| if json { serde_json::to_string_pretty(kept).unwrap() } else { kept.join("\n") }
}

#[test]
fn a_list_is_cut_to_the_whole_items_under_the_ceiling() {
  let cases: Cases = serde_json::from_str(CASES).unwrap();

  for case in &cases.items {
    let pieces: Vec<String> = case.items.iter().map(Piece::text).collect();
    let items: Vec<String> =
      pieces.iter().cloned().cycle().take(pieces.len() * case.copies.unwrap_or(1)).collect();
    let said = Said {
      unit: &case.unit,
      how: &case.how,
      limit: case.limit.unwrap_or(MAX_ANSWER_CHARS),
    };
    let capped = cap_items(&items, render(&case.render), said);

    assert_eq!(
      (capped.kept.len(), capped.left, capped.note.clone()),
      (case.kept, case.left, case.note.clone()),
      "{}",
      case.name
    );
  }
}

#[test]
fn a_said_stands_at_the_answer_ceiling_unless_given_another() {
  let said = Said::new("rows", "give a LIMIT.");

  assert_eq!((said.unit, said.how, said.limit), ("rows", "give a LIMIT.", MAX_ANSWER_CHARS));
}

#[test]
fn counts_carry_the_thousands_mark() {
  assert_eq!(count_text(0), "0");
  assert_eq!(count_text(999), "999");
  assert_eq!(count_text(1_000), "1,000");
  assert_eq!(count_text(30_000), "30,000");
  assert_eq!(count_text(1_234_567), "1,234,567");
}
