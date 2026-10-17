//! Trimming a fixed heading past its ceiling: set aside at once, walked a stretch a run into the pieces of its own
//! matters, each stretch measured as a sorted group is, and a failure remembered as the sort's are.

mod support;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use bmo_brain::editors::book::FIXED_CEILING_CHARS;
use bmo_brain::editors::documents::sorted_slugs;
use bmo_brain::editors::people::sort::Ended;
use bmo_brain::editors::people::trim::{
  Item, Listed, STRETCH_CHARS, Trim, item_states_text, listed, render, stretch_len, trim_person,
  trim_status, trims_of,
};
use brain_support::js;
use serde_json::json;

use support::sorting::{BOSS, PERSON, ROOM_A, Store, common_dir, piece_files, read, room, room_dir, write};

fn ongoing_slug() -> String {
  format!("p{PERSON}-ongoing")
}

fn ongoing_path() -> String {
  format!("{}/chunks/{}.md", room_dir(ROOM_A), ongoing_slug())
}

fn holding() -> String {
  format!("{}/trimming/{}.md", room_dir(ROOM_A), ongoing_slug())
}

fn humor_path() -> String {
  format!("{}/chunks/c{PERSON}-humor.md", common_dir())
}

// An item of the heading: every tenth one still open, the rest finished, each naming its own TODO item.
fn matter(at: usize) -> String {
  let day = at % 28 + 1;
  let state = if at.is_multiple_of(10) { "is still under way" } else { "was finished and closed" };

  format!(
    "- [date 2026-09-{day:02} · confirmed 2026-09-{day:02}] Matter {at} of item #{} {state}; {}",
    1000 + at,
    "the tester followed it closely and asked for the result to be reported in their private chat. "
      .repeat(3)
  )
}

// A heading past its ceiling as the writer leaves one that only grows: the Facts list written twice.
fn ongoing(count: usize) -> String {
  let half = count / 2;
  let first: Vec<String> = (0..half).map(matter).collect();
  let second: Vec<String> = (half..count).map(matter).collect();

  format!(
    "# Tester ({PERSON}) — Ongoing and promises\n\n## Facts\n{}\n\n## Views\n- [date 2026-09-01 · confirmed 2026-09-01] Expects every matter to be closed within the week.\n\n## Facts\n{}\n",
    first.join("\n"),
    second.join("\n")
  )
}

// A room book whose Ongoing and promises is past its ceiling and on the room's sorted list.
fn book(count: usize) -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  write(root, &ongoing_path(), &ongoing(count));
  write(
    root,
    &format!("{}/chunks/p{PERSON}-address.md", room_dir(ROOM_A)),
    &format!(
      "# Tester ({PERSON}) — Address\n\n## Facts\n- [date unknown · confirmed unknown] Called Bro.\n"
    ),
  );
  write(
    root,
    &format!("{}/sorted.txt", room_dir(ROOM_A)),
    &format!("{}\np{PERSON}-address\n", ongoing_slug()),
  );

  dir
}

// The heading a run is walking: the one set aside, or the first to trim where none is yet.
fn the_trim(root: &Path) -> Trim {
  let trims = trims_of(root, PERSON, &BTreeSet::new());

  trims
    .iter()
    .find(|trim| root.join(trim.holding_path()).exists())
    .unwrap_or(&trims[0])
    .clone()
}

// What a faithful trim writes for the stretch it is given: the open matters into the heading, every finished one
// into a piece of finished matters, a piece to about every eight.
fn faithful(root: &Path, _chat_id: i64) {
  let trim = the_trim(root);
  let items = listed(&read(root, &trim.holding_path())).items;
  let stretch = &items[..stretch_len(&items, STRETCH_CHARS)];
  let mut heading = listed(&read(root, &trim.piece_path()));

  for item in stretch {
    if item.text.contains("still under way") || item.section == "Views" {
      heading.items.push(item.clone());
      continue;
    }

    let number = item.text.split("Matter ").nth(1).and_then(|rest| rest.split(' ').next()).unwrap();
    let path = format!(
      "{}/chunks/p{PERSON}-finished-matters-{}.md",
      trim.dir(),
      number.parse::<usize>().unwrap() / 8
    );
    let mut piece = listed(&fs::read_to_string(root.join(&path)).unwrap_or_default());

    if piece.heading.is_empty() {
      piece.heading = format!("# Tester ({PERSON}) — finished matters");
    }

    piece.items.push(item.clone());
    write(root, &path, &render(&piece));
  }

  write(root, &trim.piece_path(), &render(&heading));
}

// A trim that keeps half of what it was given.
fn dropping(root: &Path, _chat_id: i64) {
  let trim = the_trim(root);
  let items = listed(&read(root, &trim.holding_path())).items;
  let stretch = &items[..stretch_len(&items, STRETCH_CHARS) / 2];

  write(
    root,
    &format!("{}/chunks/p{PERSON}-finished-matters.md", trim.dir()),
    &render(&Listed {
      heading: format!("# Tester ({PERSON}) — finished matters"),
      items: stretch.to_vec(),
    }),
  );
}

// A trim that moves its stretch faithfully and then writes into another person's room piece.
fn trespassing(root: &Path, chat_id: i64) {
  faithful(root, chat_id);
  write(
    root,
    &format!("{}/chunks/p{BOSS}-relations.md", room_dir(ROOM_A)),
    &format!(
      "# Boss ({BOSS}) — Relations\n\n## Facts\n- [date unknown · confirmed unknown] Knows the tester.\n"
    ),
  );
}

// A trim that moves its stretch faithfully and then edits the holding file it was told to leave alone.
fn touching_the_holding(root: &Path, chat_id: i64) {
  let trim = the_trim(root);

  faithful(root, chat_id);
  write(root, &trim.holding_path(), "# emptied\n");
}

// A trim that judges every matter still open.
fn keeping_everything(root: &Path, _chat_id: i64) {
  let trim = the_trim(root);
  let items = listed(&read(root, &trim.holding_path())).items;
  let mut heading = listed(&read(root, &trim.piece_path()));

  heading.items.extend(items[..stretch_len(&items, STRETCH_CHARS)].iter().cloned());
  write(root, &trim.piece_path(), &render(&heading));
}

fn all_text(root: &Path) -> String {
  let mut files: Vec<(String, String)> = piece_files(root).into_iter().collect();

  files.sort();
  files.into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_piece_reads_as_its_heading_and_items_and_writes_each_section_once() {
  let text = "# Tester (1) — Ongoing and promises\n\n## Facts\n- one\n  carried on\n\n## Judgements\n- three\n\n## Facts\n- two\n";
  let read = listed(text);

  assert_eq!(read.heading, "# Tester (1) — Ongoing and promises");
  assert_eq!(
    read
      .items
      .iter()
      .map(|item| (item.section.as_str(), item.text.as_str()))
      .collect::<Vec<_>>(),
    vec![("Facts", "- one\n  carried on"), ("Judgements", "- three"), ("Facts", "- two")]
  );
  assert_eq!(
    render(&read),
    "# Tester (1) — Ongoing and promises\n\n## Facts\n- one\n  carried on\n- two\n\n## Judgements\n- three\n"
  );
  assert_eq!(
    listed(&render(&read)),
    Listed {
      heading: read.heading.clone(),
      items: vec![
        Item {
          section: "Facts".into(),
          text: "- one\n  carried on".into()
        },
        Item {
          section: "Facts".into(),
          text: "- two".into()
        },
        Item {
          section: "Judgements".into(),
          text: "- three".into()
        },
      ],
    }
  );
}

#[test]
fn a_stretch_takes_the_items_that_fit_and_never_none() {
  let items: Vec<Item> = ["aaaa", "bbbb", "cccc"]
    .iter()
    .map(|text| Item {
      section: "Facts".into(),
      text: (*text).into(),
    })
    .collect();

  assert_eq!(stretch_len(&items, 9), 2);
  assert_eq!(stretch_len(&items, 2), 1);
  assert_eq!(stretch_len(&items, 100), 3);
}

#[test]
fn an_items_state_reads_as_finished_open_or_unknown() {
  let states = HashMap::from([(1, true), (2, false)]);

  assert_eq!(
    item_states_text(&states, &[1, 2, 3]),
    "- #1: finished\n- #2: not finished\n- #3: not in the TODO"
  );
  assert_eq!(item_states_text(&states, &[]), "(the stretch names no TODO item)");
}

#[test]
fn only_a_fixed_heading_past_its_ceiling_or_set_aside_is_to_trim() {
  let dir = book(60);
  let root = dir.path();
  let trims = trims_of(root, PERSON, &BTreeSet::new());

  assert_eq!(trims.len(), 1);
  assert_eq!(trims[0].piece_path(), ongoing_path());
  assert_eq!(trims[0].holding_path(), holding());
  assert_eq!(trims[0].chat_id, Some(ROOM_A));
  assert!(trims_of(root, BOSS, &BTreeSet::new()).is_empty());

  // A heading under its ceiling is left alone; one of the writer's own topics however long is not a heading.
  write(root, &ongoing_path(), &ongoing(3));
  write(root, &format!("{}/chunks/p{PERSON}-long-story.md", room_dir(ROOM_A)), &ongoing(60));
  assert!(trims_of(root, PERSON, &BTreeSet::new()).is_empty());
}

#[tokio::test]
async fn a_heading_past_its_ceiling_is_walked_and_keeps_only_what_is_open() {
  let dir = book(60);
  let root = dir.path();
  let before = all_text(root);
  let mut trimming = room(root, faithful, None, &Store::default(), None);

  assert_eq!(
    trim_person(&mut trimming, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Done
  );

  let heading = read(root, &ongoing_path());

  assert!(js::units(&heading) <= FIXED_CEILING_CHARS);
  assert!(heading.contains("Matter 10 of item #1010 is still under way"));
  assert!(heading.contains("Expects every matter to be closed within the week"));
  assert!(!heading.contains("was finished"));
  assert!(!root.join(holding()).exists());

  // Every item stands in exactly one piece, the finished ones in pieces of their own.
  let after = all_text(root);

  for at in 0..60 {
    let text = format!("Matter {at} of item #{} ", 1000 + at);

    assert_eq!(after.matches(&text).count(), 1, "{text}");
    assert_eq!(before.matches(&text).count(), 1, "{text}");
  }

  // Two stretches, each run told its stretch, how its items stand and where it may write.
  let recorded = &trimming.recorded;

  assert_eq!(recorded.prompts.len(), 2);
  assert!(recorded.prompts[0].1.contains("Matter 1 of item #1001"));
  assert!(recorded.prompts[0].1.contains("- #1001: not finished"));
  assert!(recorded.prompts[0].1.contains("- #1002: finished"));
  assert!(recorded.prompts[0].1.contains("The fixed heading: Ongoing and promises"));
  assert!(!recorded.prompts[0].1.contains("{{"));
  assert_eq!(recorded.desks[0].1, vec![room_dir(ROOM_A)]);
  assert_eq!(
    recorded.desks[0]
      .2
      .iter()
      .map(|filter| (filter.chat_id, filter.owners.clone()))
      .collect::<Vec<_>>(),
    vec![(Some(Some(ROOM_A)), Some(vec![PERSON]))]
  );

  // What the heading gave was sorted already, so the pieces it made are on the sorted list.
  let sorted = sorted_slugs(root, &room_dir(ROOM_A));

  assert!(sorted.contains(&format!("p{PERSON}-finished-matters-0")));
  assert!(recorded.messages[0].contains("is set aside to be trimmed"));
  assert!(recorded.messages.last().unwrap().contains("is trimmed"));
  assert!(trims_of(root, PERSON, &BTreeSet::new()).is_empty());
}

#[tokio::test]
async fn a_trim_stopped_by_its_budget_leaves_a_short_heading_and_picks_up_where_it_stopped() {
  let dir = book(60);
  let root = dir.path();
  let store = Store::default();
  let mut first = room(root, faithful, None, &store, None);

  assert_eq!(trim_person(&mut first, PERSON, &mut 1).await.ok().unwrap(), Ended::Stopped);
  assert!(js::units(&read(root, &ongoing_path())) <= FIXED_CEILING_CHARS);
  assert!(root.join(holding()).exists());

  let (line, work) = trim_status(root, &mut store.clone(), &the_trim(root)).await.unwrap();

  assert!(line.contains("being trimmed"), "{line}");
  assert!(work);

  let mut second = room(root, faithful, None, &store, None);

  assert_eq!(
    trim_person(&mut second, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Done
  );
  assert_eq!(second.recorded.prompts.len(), 1);
  assert!(!second.recorded.prompts[0].1.contains("Matter 1 of item"));
  assert!(!root.join(holding()).exists());
}

#[tokio::test]
async fn a_stretch_that_drops_items_is_put_back_and_the_holding_file_stands() {
  let dir = book(60);
  let root = dir.path();
  let mut trimming = room(root, dropping, None, &Store::default(), None);

  assert_eq!(
    trim_person(&mut trimming, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Failed
  );
  assert!(!root.join(format!("{}/chunks/p{PERSON}-finished-matters.md", room_dir(ROOM_A))).exists());
  assert_eq!(listed(&read(root, &holding())).items.len(), 61);
  assert_eq!(trimming.recorded.prompts.len(), 1);
}

#[tokio::test]
async fn a_stretch_that_writes_outside_this_persons_pieces_fails() {
  for editor in [trespassing as fn(&Path, i64), touching_the_holding] {
    let dir = book(60);
    let root = dir.path();
    let mut trimming = room(root, editor, None, &Store::default(), None);

    assert_eq!(trim_person(&mut trimming, PERSON, &mut 1).await.ok().unwrap(), Ended::Failed);
    assert!(!root.join(format!("{}/chunks/p{BOSS}-relations.md", room_dir(ROOM_A))).exists());
    assert_eq!(listed(&read(root, &holding())).items.len(), 61);
    assert_eq!(listed(&read(root, &ongoing_path())).items.len(), 0);
  }
}

#[tokio::test]
async fn a_stretch_that_fails_twice_goes_back_into_the_heading_and_the_walk_goes_on() {
  let dir = book(60);
  let root = dir.path();
  let store = Store::default();

  for _ in 0..2 {
    let mut failing = room(root, dropping, None, &store, None);

    assert_eq!(
      trim_person(&mut failing, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
      Ended::Failed
    );
  }

  let notices = store.notices.borrow().clone();

  assert_eq!(notices.len(), 1);
  assert_eq!(notices[0].params["work"], json!("trim-stretch"));
  assert_eq!(notices[0].params["heading"], json!("Ongoing and promises"));
  assert_eq!(notices[0].params["chatId"], json!(ROOM_A));

  // The next run puts the first stretch back unwalked and walks the rest.
  let mut walking = room(root, faithful, None, &store, None);

  trim_person(&mut walking, PERSON, &mut usize::MAX.clone()).await.ok().unwrap();

  let heading = read(root, &ongoing_path());

  assert!(heading.contains("Matter 1 of item #1001 was finished"));
  assert!(!heading.contains("Matter 59 of item #1059"));
  assert!(!root.join(holding()).exists());
  assert_eq!(walking.recorded.prompts.len(), 1);
}

#[tokio::test]
async fn a_heading_left_with_nothing_open_is_taken_away_with_its_row() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  let finished: Vec<String> = (1..30_usize).filter(|at| !at.is_multiple_of(10)).map(matter).collect();

  write(
    root,
    &ongoing_path(),
    &format!("# Tester ({PERSON}) — Ongoing and promises\n\n## Facts\n{}\n", finished.join("\n")),
  );

  let mut trimming = room(root, faithful, None, &Store::default(), None);

  assert_eq!(
    trim_person(&mut trimming, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Done
  );
  assert!(!root.join(ongoing_path()).exists());
  assert_eq!(trimming.recorded.removed, vec![ongoing_path()]);
}

#[tokio::test]
async fn a_heading_still_past_its_ceiling_after_two_walks_is_left_as_it_stands() {
  let dir = book(60);
  let root = dir.path();
  let store = Store::default();

  for _ in 0..2 {
    let mut keeping = room(root, keeping_everything, None, &store, None);

    trim_person(&mut keeping, PERSON, &mut usize::MAX.clone()).await.ok().unwrap();
  }

  assert!(!root.join(holding()).exists());
  assert_eq!(store.notices.borrow().len(), 1);
  assert_eq!(store.notices.borrow()[0].params["work"], json!("trim-heading"));

  let (line, work) = trim_status(root, &mut store.clone(), &the_trim(root)).await.unwrap();

  assert!(line.contains("left as it stands after two trims"), "{line}");
  assert!(!work);

  let mut idle = room(root, keeping_everything, None, &store, None);

  assert_eq!(trim_person(&mut idle, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(), Ended::Done);
  assert!(idle.recorded.prompts.is_empty());
}

// A common heading past its ceiling, the person's room book standing beside it.
fn common_book() -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  let jokes: Vec<String> = (0..40)
    .map(|at| {
      format!(
        "- [date 2026-09-{:02} · confirmed 2026-09-{:02}] Joke {at} landed with the tester; {}",
        at % 28 + 1,
        at % 28 + 1,
        "they laughed and answered with a pun of their own. ".repeat(3)
      )
    })
    .collect();

  write(
    root,
    &humor_path(),
    &format!("# Tester ({PERSON}) — Humor\n\n## Facts\n{}\n", jokes.join("\n")),
  );
  write(
    root,
    &format!("{}/chunks/p{PERSON}-address.md", room_dir(ROOM_A)),
    &format!(
      "# Tester ({PERSON}) — Address\n\n## Facts\n- [date unknown · confirmed unknown] Called Bro.\n"
    ),
  );

  dir
}

// A common trim keeping the first three jokes as the pattern and moving the rest into a piece of the jokes.
fn common_faithful(root: &Path, _chat_id: i64) {
  let trim = the_trim(root);
  let items = listed(&read(root, &trim.holding_path())).items;
  let stretch = &items[..stretch_len(&items, STRETCH_CHARS)];
  let mut heading = listed(&read(root, &trim.piece_path()));
  let path = format!("{}/chunks/c{PERSON}-jokes-that-landed.md", common_dir());
  let mut jokes = listed(&fs::read_to_string(root.join(&path)).unwrap_or_default());

  jokes.heading = format!("# Tester ({PERSON}) — jokes that landed");

  for item in stretch {
    if heading.items.len() < 3 {
      heading.items.push(item.clone());
    } else {
      jokes.items.push(item.clone());
    }
  }

  write(root, &trim.piece_path(), &render(&heading));
  write(root, &path, &render(&jokes));
}

#[tokio::test]
async fn a_common_heading_is_trimmed_inside_the_common_part() {
  let dir = common_book();
  let root = dir.path();
  let trim = the_trim(root);

  assert_eq!(trim.chat_id, None);
  assert_eq!(trim.run_room, ROOM_A);
  assert_eq!(trim.holding_path(), format!("{}/trimming/c{PERSON}-humor.md", common_dir()));

  let mut trimming = room(root, common_faithful, None, &Store::default(), None);

  assert_eq!(
    trim_person(&mut trimming, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Done
  );
  assert!(js::units(&read(root, &humor_path())) <= FIXED_CEILING_CHARS);
  assert!(
    read(root, &format!("{}/chunks/c{PERSON}-jokes-that-landed.md", common_dir())).contains("Joke 39")
  );

  let prompt = &trimming.recorded.prompts[0].1;

  assert!(prompt.contains("The person's common book"));
  assert!(prompt.contains("\"Alpha Room\""));
  assert_eq!(trimming.recorded.desks[0].1, vec![common_dir()]);
  assert_eq!(trimming.recorded.desks[0].2[0].chat_id, Some(None));
}

// A trim that keeps what is open and drops every finished matter as the work state it is, listing each word for word.
fn dropping_work(root: &Path, _chat_id: i64) {
  let trim = the_trim(root);
  let items = listed(&read(root, &trim.holding_path())).items;
  let stretch = &items[..stretch_len(&items, STRETCH_CHARS)];
  let mut heading = listed(&read(root, &trim.piece_path()));
  let mut dropped = vec![];

  for item in stretch {
    if item.text.contains("was finished") {
      dropped.push(item.text.clone());
    } else {
      heading.items.push(item.clone());
    }
  }

  write(root, &trim.piece_path(), &render(&heading));
  write(root, &trim.dropped_path(), &format!("{}\n", dropped.join("\n")));
}

// A trim whose dropped list says less than the items it drops.
fn dropping_summarised(root: &Path, chat_id: i64) {
  let trim = the_trim(root);

  dropping_work(root, chat_id);
  write(
    root,
    &trim.dropped_path(),
    "- [date unknown · confirmed unknown] Many matters were finished.\n",
  );
}

#[tokio::test]
async fn work_states_are_dropped_through_their_list_and_nothing_of_them_stays() {
  let dir = book(60);
  let root = dir.path();
  let mut trimming = room(root, dropping_work, None, &Store::default(), None);

  assert_eq!(
    trim_person(&mut trimming, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Done
  );

  let after = all_text(root);

  assert!(!after.contains("was finished"));
  assert!(after.contains("Matter 10 of item #1010 is still under way"));
  assert!(!root.join(format!("{}/trimming", room_dir(ROOM_A))).exists());
  assert!(
    trimming.recorded.prompts[0]
      .1
      .contains(&format!("trimming/{}.dropped.md", ongoing_slug()))
  );
  // The lists are no documents anything reads, so nothing announces them.
  assert!(trimming.recorded.finished.iter().flatten().all(|path| !path.contains("/trimming/")));
}

#[tokio::test]
async fn a_dropped_list_that_is_not_the_stretchs_own_items_fails() {
  let dir = book(60);
  let root = dir.path();
  let mut trimming = room(root, dropping_summarised, None, &Store::default(), None);

  assert_eq!(trim_person(&mut trimming, PERSON, &mut 1).await.ok().unwrap(), Ended::Failed);
  assert_eq!(listed(&read(root, &holding())).items.len(), 61);
  assert!(
    !root
      .join(format!("{}/trimming/{}.dropped.md", room_dir(ROOM_A), ongoing_slug()))
      .exists()
  );
}

#[test]
fn a_room_that_keeps_no_room_part_has_nothing_to_trim() {
  let dir = book(60);
  let root = dir.path();

  assert!(trims_of(root, PERSON, &BTreeSet::from([ROOM_A])).is_empty());
}

#[tokio::test]
async fn a_failing_stretch_leaves_no_second_heading_set_aside() {
  let dir = book(60);
  let root = dir.path();

  write(root, &humor_path(), &ongoing(30).replace("Ongoing and promises", "Humor"));

  let mut failing = room(root, dropping, None, &Store::default(), None);

  assert_eq!(
    trim_person(&mut failing, PERSON, &mut usize::MAX.clone()).await.ok().unwrap(),
    Ended::Failed
  );

  let aside: Vec<Trim> = trims_of(root, PERSON, &BTreeSet::new())
    .into_iter()
    .filter(|trim| root.join(trim.holding_path()).exists())
    .collect();

  assert_eq!(aside.len(), 1);
  assert_eq!(failing.recorded.prompts.len(), 1);
}

#[tokio::test]
async fn a_heading_already_set_aside_is_walked_first() {
  let dir = book(60);
  let root = dir.path();
  let store = Store::default();

  // The room heading is set aside by a run that failed on it.
  trim_person(&mut room(root, dropping, None, &store, None), PERSON, &mut usize::MAX.clone())
    .await
    .ok();
  write(root, &humor_path(), &ongoing(30).replace("Ongoing and promises", "Humor"));

  let mut walking = room(root, faithful, None, &store, None);

  trim_person(&mut walking, PERSON, &mut 1).await.ok();

  assert!(walking.recorded.prompts[0].1.contains("The fixed heading: Ongoing and promises"));
  assert!(!root.join(format!("{}/trimming/c{PERSON}-humor.md", common_dir())).exists());
}
