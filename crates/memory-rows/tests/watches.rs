//! A watch asked for before anything is written: a check or a timer read off its arguments, and the
//! refusals of one that cannot stand; and the name a watch is shown by.

use memory_rows::watches::{
  Watch, WatchArguments, collapse, one_line, seoul_when, watch_name, watch_request,
};

const NOW: i64 = 1_800_000_000;

fn arguments<'a>() -> WatchArguments<'a> {
  WatchArguments {
    agent: "agent-a",
    chat: Some("-100"),
    by: Some(" chat:-100 "),
    until: Some("test -f  /tmp/done"),
    at: None,
    name: None,
    note: Some("tell the room the build is done"),
    every: None,
    within: None,
    item: None,
  }
}

#[test]
fn a_check_is_asked_for_with_its_defaults() {
  let asked = watch_request(&arguments(), NOW, &|_| true).unwrap();

  assert_eq!(asked.chat_id, -100);
  assert_eq!(asked.by_chat, -100);
  assert!(!asked.timer);
  assert_eq!(asked.dedupe_key, "test -f /tmp/done");
  assert_eq!(asked.interval_seconds, Some(60));
  assert_eq!(asked.deadline_at, NOW + 120 * 60);
}

#[test]
fn a_timer_is_read_on_the_seoul_clock() {
  let timer = WatchArguments {
    until: None,
    at: Some("2027-01-16 09:30"),
    ..arguments()
  };
  let asked = watch_request(&timer, NOW, &|_| true).unwrap();

  assert!(asked.timer);
  assert_eq!(asked.due_at, seoul_when("2027-01-16T09:30:00+09:00"));
  assert_eq!(asked.deadline_at, asked.due_at.unwrap());
  assert_eq!(asked.dedupe_key, format!("at:{}:tell the room the build is done", asked.deadline_at));
}

#[test]
fn a_watch_that_cannot_stand_is_refused_before_anything_exists() {
  let refused = |args: WatchArguments<'_>| watch_request(&args, NOW, &|_| true).unwrap_err();

  assert_eq!(
    refused(WatchArguments {
      by: Some("-100"),
      ..arguments()
    }),
    "a watch has to say whose it is: --by chat:<your own chatId>."
  );
  assert_eq!(
    refused(WatchArguments {
      every: Some("5"),
      ..arguments()
    }),
    "--every is whole seconds, from 15 to 1800."
  );
  assert_eq!(
    refused(WatchArguments {
      at: Some("2027-01-16 09:30"),
      ..arguments()
    }),
    "give exactly one of --until '<check>' (the watch) or --at '<time>' (the timer, only when no check can say it)."
  );
  assert_eq!(
    watch_request(&arguments(), NOW, &|_| false).unwrap_err(),
    "bash cannot parse the check given to --until."
  );
}

#[test]
fn a_watch_is_shown_by_its_name_or_the_head_of_its_note() {
  let watch = Watch {
    id: 1,
    chat_id: -100,
    by_chat: -100,
    name: None,
    note: "\n  first line\nsecond line".to_owned(),
    memory_id: None,
    created_at: NOW,
  };

  assert_eq!(watch_name(&watch), "first line");
  assert_eq!(
    watch_name(&Watch {
      name: Some(" short ".to_owned()),
      ..watch
    }),
    "short"
  );
  assert_eq!(one_line("a  b\nc", 3), "a …");
  assert_eq!(collapse("  a \t b  "), "a b");
}
