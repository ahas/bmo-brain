//! Sorting a person's room pieces into the two parts of their book: a group at a time, measured against what it was
//! given, cut first where the book is still one document, the oversized room sheets written again before anything
//! else, and a work that fails twice the same way skipped with one notice to the owner.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bmo_brain::editors::book::PersonPiece;
use bmo_brain::editors::documents::sorted_slugs;
use bmo_brain::editors::people::sort::{
  Ended, Work, needs_cut, reconfirmed_chars, refresh_over_ceiling_sheets, room_id, sort_groups,
  sort_notice, sort_person, sort_status, statement_chars,
};
use serde_json::json;

use support::sorting::{
  BOSS, PERSON, ROOM_A, ROOM_B, Room, Store, common_dir, read, room, room_dir, write,
};

fn piece(path: &str, text: &str) -> PersonPiece {
  PersonPiece {
    path: path.into(),
    text: text.into(),
  }
}

#[test]
fn a_room_directory_is_a_chat_id() {
  assert_eq!(room_id("-100"), Some(-100));
  assert_eq!(room_id("12"), Some(12));
  assert_eq!(room_id("x"), None);
  assert_eq!(room_id("-"), None);
}

#[test]
fn a_statement_counts_without_its_heading_list_mark_or_bracket() {
  let text = "# p1-a\n\n## Facts\n- [date 2026-01-02 · confirmed 2026-01-03] likes tea\n- plain\nprose";

  assert_eq!(statement_chars(text), "likes tea".len() + "plain".len() + "prose".len());
}

#[test]
fn a_group_holds_pieces_up_to_its_size() {
  let pieces = [piece("a", &"a".repeat(5)), piece("b", &"b".repeat(5)), piece("c", &"c".repeat(12))];

  assert_eq!(sort_groups(&pieces, 10).iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 1]);
}

#[test]
fn an_item_whose_text_stands_and_whose_bracket_moved_is_counted_and_nothing_else() {
  let before = [piece(
    "c1-speech.md",
    "## Facts\n- [date 2026-09-01 · confirmed 2026-09-01] Speaks short.\n- [date unknown · confirmed unknown] Laughs.",
  )];
  let after = [piece(
    "c1-speech.md",
    "## Facts\n- [date 2026-09-01 · confirmed 2026-09-09] Speaks short.\n- [date unknown · confirmed unknown] Laughs.\n- [date unknown · confirmed unknown] A new item.",
  )];

  assert_eq!(reconfirmed_chars(&before, &after), "Speaks short.".len());
}

#[test]
fn a_skipped_work_is_told_as_a_code_with_its_heading_person_room_and_cause() {
  let notice = sort_notice(Work::TrimStretch("Humor"), PERSON, None, None, "It summarised.");

  assert_eq!(notice.code, "people-sort-failed");
  assert_eq!(
    serde_json::to_value(&notice).unwrap(),
    json!({
      "code": "people-sort-failed",
      "work": "trim-stretch",
      "heading": "Humor",
      "person": PERSON,
      "chatId": null,
      "title": null,
      "cause": "It summarised.",
    })
  );
  assert_eq!(
    [Work::Cut, Work::Group, Work::Sheet, Work::TrimHeading("Humor")].map(Work::code),
    ["cut", "group", "sheet", "trim-heading"]
  );
  assert_eq!(Work::Group.heading(), None);
}

fn talk_slug() -> String {
  format!("p{PERSON}-01-talk")
}

fn speech_slug() -> String {
  format!("p{PERSON}-speech")
}

fn talk() -> String {
  [
    format!("# Tester ({PERSON}) — way of talking, weekend stories"),
    String::new(),
    "The tester speaks short and dry and leads with the conclusion. They put almost no feeling on it, so they sound blunt, but that is their way.".to_owned(),
    "Every weekend they bring up building keyboards and post photos of newly bought switches. They like being held on that story for a long time.".to_owned(),
    String::new(),
  ]
  .join("\n")
}

fn speech() -> String {
  [
    format!("# Tester ({PERSON}) — talk they dislike"),
    String::new(),
    "## Facts".to_owned(),
    "- [date 2026-09-01 · confirmed 2026-09-01] Said twice they dislike long, roundabout polite answers."
      .to_owned(),
    String::new(),
  ]
  .join("\n")
}

// One person in two rooms: an older prose piece in one, an older list piece in the other, and a sheet in each.
fn books() -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  write(
    root,
    &format!("{}/index.md", room_dir(ROOM_A)),
    &format!(
      "# Person book (room {ROOM_A})\n\n## {PERSON} — Tester (Bro)\n- {} — way of talking, keyboards · period unknown\n",
      talk_slug()
    ),
  );
  write(root, &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()), &talk());
  write(
    root,
    &format!("{}/{PERSON}.md", room_dir(ROOM_A)),
    "## Address\n- [fact · confirmed unknown] Called Bro.\n",
  );
  write(
    root,
    &format!("{}/index.md", room_dir(ROOM_B)),
    &format!(
      "# Person book (room {ROOM_B})\n\n## {PERSON} — Tester\n- {} — way of talking, politeness · 2026-09\n",
      speech_slug()
    ),
  );
  write(root, &format!("{}/chunks/{}.md", room_dir(ROOM_B), speech_slug()), &speech());
  write(
    root,
    &format!("{}/{PERSON}.md", room_dir(ROOM_B)),
    "## Address\n- [fact · confirmed unknown] Tester.\n",
  );

  dir
}

// What a faithful sorting run writes: the common statements into the common part, the statement that hangs on this
// room's own talk left in the room piece, and every index kept to its pieces.
fn faithful(root: &Path, chat_id: i64) {
  let common_piece = format!("{}/chunks/c{PERSON}-speech.md", common_dir());
  let had = fs::read_to_string(root.join(&common_piece))
    .unwrap_or_else(|_| format!("# Tester ({PERSON}) — way of talking\n\n## Facts\n"));

  if chat_id == ROOM_A {
    write(
      root,
      &common_piece,
      &format!(
        "{}\n- [date unknown · confirmed unknown] Speaks short and dry and leads with the conclusion.\n- [date unknown · confirmed unknown] Puts almost no feeling on it, so they sound blunt, but that is their way.\n",
        had.trim_end()
      ),
    );
    write(
      root,
      &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()),
      &format!(
        "# Tester (Bro, {PERSON}) — weekend stories, keyboards\n\n## Facts\n- [date unknown · confirmed unknown] Every weekend they bring up building keyboards.\n- [date unknown · confirmed unknown] They post photos of newly bought switches.\n\n## Views\n- [date unknown · confirmed unknown] Say they like being held on that story for a long time.\n"
      ),
    );
  } else {
    write(
      root,
      &common_piece,
      &format!(
        "{}\n- [date 2026-09-01 · confirmed 2026-09-01] Said twice they dislike long, roundabout polite answers.\n",
        had.trim_end()
      ),
    );
    fs::remove_file(root.join(format!("{}/chunks/{}.md", room_dir(ROOM_B), speech_slug()))).unwrap();
    write(
      root,
      &format!("{}/index.md", room_dir(ROOM_B)),
      &format!("# Person book (room {ROOM_B})\n\n## {PERSON} — Tester\n"),
    );
  }

  write(
    root,
    &format!("{}/index.md", common_dir()),
    &format!(
      "# Person book — common to the person\n\n## {PERSON} — Tester\n- c{PERSON}-speech — way of talking, politeness · 2026-09\n"
    ),
  );
}

async fn sort_all(room: &mut Room) -> Ended {
  sort_person(room, PERSON, &mut usize::MAX.clone()).await.ok().unwrap()
}

fn dropping(root: &Path, chat_id: i64) {
  if chat_id == ROOM_A {
    write(
      root,
      &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()),
      &format!(
        "# Tester ({PERSON}) — weekends\n\n## Facts\n- [date unknown · confirmed unknown] Keyboards.\n"
      ),
    );
  }
}

#[tokio::test]
async fn the_common_statements_move_into_the_common_part_and_the_rooms_own_stay_in_the_room() {
  let dir = books();
  let root = dir.path();
  let mut sorting = room(root, faithful, None, &Store::default(), None);

  assert_eq!(sort_all(&mut sorting).await, Ended::Done);

  let common = read(root, &format!("{}/chunks/c{PERSON}-speech.md", common_dir()));

  assert!(common.contains("Speaks short and dry"));
  assert!(common.contains("[date 2026-09-01 · confirmed 2026-09-01] Said twice"));
  assert!(!common.contains("keyboards"));
  assert!(
    read(root, &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug())).contains("building keyboards")
  );
  assert!(
    !read(root, &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug())).contains("short and dry")
  );

  // The prompt carries the group itself, and the room's sorted list now names what was sorted.
  let recorded = &sorting.recorded;

  assert_eq!(recorded.prompts.iter().map(|(chat, _)| *chat).collect::<Vec<_>>(), vec![ROOM_A, ROOM_B]);
  assert!(recorded.prompts[0].1.contains("Every weekend they bring up building keyboards"));
  assert!(recorded.prompts[0].1.contains("display name \"Tester\""));
  // The run reaches this room's part of the book and the person's common part, and nothing else.
  assert_eq!(recorded.desks[0].0, ROOM_A);
  assert_eq!(recorded.desks[0].1, vec![room_dir(ROOM_A), common_dir()]);
  assert_eq!(
    recorded.desks[0]
      .2
      .iter()
      .map(|filter| (filter.chat_id, filter.owners.clone(), filter.active))
      .collect::<Vec<_>>(),
    vec![
      (Some(Some(ROOM_A)), Some(vec![PERSON]), Some(true)),
      (Some(None), Some(vec![PERSON]), Some(true))
    ]
  );
  assert!(sorted_slugs(root, &room_dir(ROOM_A)).contains(&talk_slug()));
  assert_eq!(recorded.sheets, vec![ROOM_A, ROOM_B]);
  assert!(recorded.finished[1].contains(&format!("{}/chunks/c{PERSON}-speech.md", common_dir())));
}

#[tokio::test]
async fn a_room_sheet_is_taken_away_where_no_room_piece_is_left() {
  let dir = books();
  let root = dir.path();

  sort_all(&mut room(root, faithful, None, &Store::default(), None)).await;

  assert!(!root.join(format!("{}/{PERSON}.md", room_dir(ROOM_B))).exists());
  assert!(root.join(format!("{}/{PERSON}.md", room_dir(ROOM_A))).exists());
  assert!(sort_status(root, &mut Store::default(), &BTreeSet::new()).await.unwrap().1.is_empty());
}

#[tokio::test]
async fn a_sort_picks_up_where_it_stopped_and_does_not_sort_a_sorted_piece_again() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();
  let mut first = room(root, faithful, None, &store, None);

  assert_eq!(sort_person(&mut first, PERSON, &mut 1).await.ok().unwrap(), Ended::Stopped);
  assert_eq!(first.recorded.prompts.len(), 1);

  let mut second = room(root, faithful, None, &store, None);

  assert_eq!(sort_all(&mut second).await, Ended::Done);
  assert_eq!(second.recorded.prompts.iter().map(|(chat, _)| *chat).collect::<Vec<_>>(), vec![ROOM_B]);
}

#[tokio::test]
async fn a_run_that_drops_a_statement_fails_and_the_book_is_put_back() {
  let dir = books();
  let root = dir.path();
  let mut sorting = room(root, dropping, None, &Store::default(), None);

  assert_eq!(sort_all(&mut sorting).await, Ended::Failed);
  assert_eq!(read(root, &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug())), talk());
  assert!(sorted_slugs(root, &room_dir(ROOM_A)).is_empty());
  assert!(sorting.recorded.finished.is_empty());
  assert!(sorting.recorded.sheets.is_empty());
}

fn keeping(share: f64) -> impl Fn(&Path, i64) {
  move |root: &Path, chat_id: i64| {
    if chat_id == ROOM_A {
      let kept = (statement_chars(&talk()) as f64 * share).ceil() as usize;

      write(
        root,
        &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()),
        &format!(
          "# Tester ({PERSON}) — weekends\n\n## Facts\n- [date unknown · confirmed unknown] {}\n",
          "a".repeat(kept)
        ),
      );
    }
  }
}

fn keeping_82(root: &Path, chat_id: i64) {
  keeping(0.82)(root, chat_id);
}

fn keeping_70(root: &Path, chat_id: i64) {
  keeping(0.7)(root, chat_id);
}

#[tokio::test]
async fn a_sort_that_keeps_most_of_the_group_stands_and_one_that_keeps_much_less_does_not() {
  assert!(statement_chars(&talk()) + statement_chars(&speech()) > 0);

  let most = books();
  let less = books();

  assert_ne!(
    sort_person(&mut room(most.path(), keeping_82, None, &Store::default(), None), PERSON, &mut 1)
      .await
      .ok()
      .unwrap(),
    Ended::Failed
  );
  assert_eq!(
    sort_person(&mut room(less.path(), keeping_70, None, &Store::default(), None), PERSON, &mut 1)
      .await
      .ok()
      .unwrap(),
    Ended::Failed
  );
}

fn naming(root: &Path, chat_id: i64) {
  faithful(root, chat_id);
  write(
    root,
    &format!("{}/chunks/c{PERSON}-relations.md", common_dir()),
    &format!(
      "# Tester ({PERSON}) — relations\n\n## Facts\n- [date unknown · confirmed unknown] Close to the boss ({BOSS}).\n"
    ),
  );
}

#[tokio::test]
async fn a_run_that_names_another_member_in_the_common_part_fails() {
  let dir = books();
  let root = dir.path();

  assert_eq!(sort_all(&mut room(root, naming, None, &Store::default(), None)).await, Ended::Failed);
  assert!(!root.join(format!("{}/chunks/c{PERSON}-relations.md", common_dir())).exists());
  assert_eq!(read(root, &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug())), talk());
}

fn reaching(root: &Path, chat_id: i64) {
  faithful(root, chat_id);
  write(root, &format!("{}/chunks/p{PERSON}-02-work.md", room_dir(ROOM_A)), "# Tester — work\n");
}

#[tokio::test]
async fn a_run_that_rewrites_a_room_piece_outside_its_group_fails() {
  let dir = books();
  let root = dir.path();
  let other = format!("{}/chunks/p{PERSON}-02-work.md", room_dir(ROOM_A));

  write(root, &other, "# Tester — work\n\nWorks at night.\n");
  write(root, &format!("{}/sorted.txt", room_dir(ROOM_A)), &format!("p{PERSON}-02-work\n"));
  write(
    root,
    &format!("{}/index.md", room_dir(ROOM_A)),
    &format!(
      "{}- p{PERSON}-02-work — work · period unknown\n",
      read(root, &format!("{}/index.md", room_dir(ROOM_A)))
    ),
  );

  assert_eq!(sort_all(&mut room(root, reaching, None, &Store::default(), None)).await, Ended::Failed);
  assert_eq!(read(root, &other), "# Tester — work\n\nWorks at night.\n");
}

fn prompts_of(room: &Room) -> Vec<i64> {
  room.recorded.prompts.iter().map(|(chat, _)| *chat).collect()
}

#[tokio::test]
async fn a_failing_group_is_tried_again_after_a_first_failure_with_no_notice_yet() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();

  assert_eq!(sort_all(&mut room(root, dropping, None, &store, None)).await, Ended::Failed);
  assert!(store.notices.borrow().is_empty());

  let mut second = room(root, dropping, None, &store, None);

  sort_all(&mut second).await;
  assert!(prompts_of(&second).contains(&ROOM_A));
}

#[tokio::test]
async fn a_group_is_skipped_after_failing_twice_the_same_way_and_the_persons_other_rooms_go_on() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();

  sort_all(&mut room(root, dropping, None, &store, None)).await;

  let mut second = room(root, dropping, None, &store, None);

  assert_eq!(sort_all(&mut second).await, Ended::Done);
  assert_eq!(store.notices.borrow().len(), 1);

  let notice = store.notices.borrow()[0].clone();

  assert_eq!(notice.code, "people-sort-failed");
  assert_eq!(notice.params["work"], json!("group"));
  assert_eq!(notice.params["person"], json!(PERSON));
  assert_eq!(notice.params["chatId"], json!(ROOM_A));
  assert!(
    notice.params["cause"]
      .as_str()
      .unwrap()
      .contains("summarised them instead of sorting them")
  );
  // The skipped room is passed over and the other room of the same person is sorted.
  assert_eq!(prompts_of(&second), vec![ROOM_A, ROOM_B]);

  let mut third = room(root, dropping, None, &store, None);

  assert_eq!(sort_all(&mut third).await, Ended::Done);
  assert!(third.recorded.prompts.is_empty());
  assert_eq!(store.notices.borrow().len(), 1);

  let (lines, people) = sort_status(root, &mut store.clone(), &BTreeSet::new()).await.unwrap();

  assert!(
    lines
      .iter()
      .any(|line| line.starts_with(&format!("{PERSON} in {ROOM_A}:")) && line.contains("skipped"))
  );
  assert!(people.is_empty());
}

#[tokio::test]
async fn a_skipped_group_is_tried_again_once_its_pieces_change() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();

  sort_all(&mut room(root, dropping, None, &store, None)).await;
  sort_all(&mut room(root, dropping, None, &store, None)).await;
  write(
    root,
    &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()),
    &format!("{}\n", talk()),
  );

  let mut again = room(root, faithful, None, &store, None);

  assert_eq!(sort_all(&mut again).await, Ended::Done);
  assert!(prompts_of(&again).contains(&ROOM_A));
  assert_eq!(store.notices.borrow().len(), 1);
}

#[tokio::test]
async fn no_failure_is_left_behind_once_the_group_stands() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();

  sort_all(&mut room(root, dropping, None, &store, None)).await;
  assert_eq!(store.counts.borrow().len(), 1);

  sort_all(&mut room(root, faithful, None, &store, None)).await;
  assert!(store.counts.borrow().is_empty());
}

fn long() -> String {
  "One sentence ends here. ".repeat(900)
}

fn cutting(root: &Path, _: i64) {
  write(root, &format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()), &talk());
  write(
    root,
    &format!("{}/{PERSON}.md", room_dir(ROOM_A)),
    "## Address\n- [fact · confirmed unknown] Bro.\n",
  );
  fs::remove_file(root.join(format!("{}/chunks/p{PERSON}-01-first.md", room_dir(ROOM_A)))).unwrap();
}

fn cutting_nothing(_: &Path, _: i64) {}

#[tokio::test]
async fn a_book_that_must_be_cut_is_cut_first_a_stretch_a_run_and_then_sorted() {
  let dir = books();
  let root = dir.path();

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_A)), &long());
  fs::remove_file(root.join(format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()))).unwrap();
  write(root, &format!("{}/chunks/p{PERSON}-01-first.md", room_dir(ROOM_A)), "# First stretch\n");
  assert!(needs_cut(root, ROOM_A, PERSON));

  let mut sorting = room(root, faithful, Some(cutting), &Store::default(), None);

  assert_eq!(sort_all(&mut sorting).await, Ended::Done);
  assert_eq!(sorting.recorded.cuts, 1);
  assert_eq!(sorting.recorded.prompts[0].0, ROOM_A);
}

#[tokio::test]
async fn the_person_stops_when_a_cut_leaves_the_book_exactly_as_it_was() {
  let dir = books();
  let root = dir.path();

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_A)), &long());
  fs::remove_file(root.join(format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()))).unwrap();

  let mut sorting = room(root, faithful, Some(cutting_nothing), &Store::default(), None);

  assert_eq!(sort_all(&mut sorting).await, Ended::Failed);
  assert!(sorting.recorded.prompts.is_empty());
}

#[tokio::test]
async fn a_book_is_skipped_after_its_cut_fails_twice_with_one_notice() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_A)), &long());
  fs::remove_file(root.join(format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()))).unwrap();

  sort_all(&mut room(root, faithful, Some(cutting_nothing), &store, None)).await;
  sort_all(&mut room(root, faithful, Some(cutting_nothing), &store, None)).await;

  let mut third = room(root, faithful, Some(cutting_nothing), &store, None);

  sort_all(&mut third).await;

  assert_eq!(third.recorded.cuts, 0);
  assert_eq!(store.notices.borrow().len(), 1);
  assert_eq!(store.notices.borrow()[0].params["work"], json!("cut"));
}

fn over_ceiling() -> String {
  format!(
    "## Address\n{}",
    "- [fact · confirmed unknown] Called Tester, and now and then Bro as well.\n".repeat(160)
  )
}

const SHORT: &str = "## Address\n- [fact · confirmed unknown] Tester.\n";

// What the sheet judgement writes from the pieces: a sheet under the ceiling at the room sheet's place.
fn judgement(root: &Path, chat_id: i64) {
  write(root, &format!("{}/{PERSON}.md", room_dir(chat_id)), SHORT);
}

#[tokio::test]
async fn a_room_sheet_over_the_ceiling_with_pieces_under_it_is_written_again_before_any_group_runs() {
  let dir = books();
  let root = dir.path();

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_B)), &over_ceiling());
  assert!(brain_support::js::units(&over_ceiling()) > 6_000);
  assert!(
    sort_status(root, &mut Store::default(), &BTreeSet::new())
      .await
      .unwrap()
      .0
      .contains(&format!(
        "{PERSON} in {ROOM_B}: sheet over the ceiling; 1 of 1 room pieces unsorted; 0 common pieces"
      ))
  );

  let mut sorting = room(root, faithful, None, &Store::default(), Some(judgement));

  assert!(!refresh_over_ceiling_sheets(&mut sorting, &[PERSON]).await.ok().unwrap());
  assert_eq!(sorting.recorded.events, vec![format!("sheets {ROOM_B}")]);
  assert!(sorting.recorded.finished[0].contains(&format!("{}/{PERSON}.md", room_dir(ROOM_B))));
  assert_eq!(read(root, &format!("{}/{PERSON}.md", room_dir(ROOM_B))), SHORT);

  sort_all(&mut sorting).await;
  assert_eq!(sorting.recorded.events[..2], [format!("sheets {ROOM_B}"), format!("run {ROOM_A}")]);
}

#[tokio::test]
async fn a_room_sheet_over_the_ceiling_is_what_the_launcher_starts_a_sort_for() {
  let dir = books();
  let root = dir.path();

  sort_all(&mut room(root, faithful, None, &Store::default(), None)).await;
  assert!(sort_status(root, &mut Store::default(), &BTreeSet::new()).await.unwrap().1.is_empty());

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_A)), &over_ceiling());
  assert_eq!(
    sort_status(root, &mut Store::default(), &BTreeSet::new()).await.unwrap().1,
    vec![PERSON]
  );
}

#[tokio::test]
async fn a_room_sheet_of_a_book_still_being_cut_is_left_alone_for_the_cut() {
  let dir = books();
  let root = dir.path();
  let book = long();

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_A)), &book);
  fs::remove_file(root.join(format!("{}/chunks/{}.md", room_dir(ROOM_A), talk_slug()))).unwrap();
  write(root, &format!("{}/chunks/p{PERSON}-01-first.md", room_dir(ROOM_A)), "# First stretch\n");

  let mut sorting = room(root, faithful, None, &Store::default(), Some(judgement));

  assert!(!refresh_over_ceiling_sheets(&mut sorting, &[PERSON]).await.ok().unwrap());
  assert!(sorting.recorded.events.is_empty());
  assert_eq!(read(root, &format!("{}/{PERSON}.md", room_dir(ROOM_A))), book);

  let lines = sort_status(root, &mut Store::default(), &BTreeSet::new()).await.unwrap().0;
  let line = lines.iter().find(|line| line.starts_with(&format!("{PERSON} in {ROOM_A}:"))).unwrap();

  assert!(!line.contains("sheet over the ceiling"));
}

#[tokio::test]
async fn a_room_sheet_the_judgement_cannot_bring_under_the_ceiling_is_tried_once_more_then_left() {
  let dir = books();
  let root = dir.path();
  let store = Store::default();

  write(root, &format!("{}/{PERSON}.md", room_dir(ROOM_B)), &over_ceiling());

  for _ in 0..3 {
    refresh_over_ceiling_sheets(&mut room(root, faithful, None, &store, None), &[PERSON])
      .await
      .ok()
      .unwrap();
  }

  assert_eq!(store.notices.borrow().len(), 1);
  assert_eq!(store.notices.borrow()[0].params["work"], json!("sheet"));
  assert_eq!(store.notices.borrow()[0].params["chatId"], json!(ROOM_B));

  let mut last = room(root, faithful, None, &store, None);

  refresh_over_ceiling_sheets(&mut last, &[PERSON]).await.ok().unwrap();
  assert!(last.recorded.sheets.is_empty());
  assert!(sort_status(root, &mut store.clone(), &BTreeSet::new()).await.unwrap().0.contains(&format!(
    "{PERSON} in {ROOM_B}: sheet over the ceiling, not written again after two failures; 1 of 1 room pieces unsorted; 0 common pieces"
  )));
}

#[tokio::test]
async fn a_room_that_keeps_no_room_part_is_left_as_it_stands() {
  let dir = books();
  let root = dir.path();
  let speech_before = read(root, &format!("{}/chunks/{}.md", room_dir(ROOM_B), speech_slug()));
  let mut sorting = room(root, faithful, None, &Store::default(), None);

  sorting.unkept = BTreeSet::from([ROOM_B]);

  assert_eq!(sort_all(&mut sorting).await, Ended::Done);
  assert_eq!(sorting.recorded.prompts.iter().map(|(chat, _)| *chat).collect::<Vec<_>>(), vec![ROOM_A]);
  assert_eq!(read(root, &format!("{}/chunks/{}.md", room_dir(ROOM_B), speech_slug())), speech_before);

  let (lines, people) = sort_status(root, &mut Store::default(), &BTreeSet::from([ROOM_B])).await.unwrap();

  assert!(people.is_empty());
  assert!(lines.iter().all(|line| !line.contains(&ROOM_B.to_string())));
}
