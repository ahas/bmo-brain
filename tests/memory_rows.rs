//! The memory rows over the brain's own migrated tables: notes, rows and watches written, closed and
//! listed, each refusal under its code, with the facts of other units and the watch launcher stood in for.

mod support;

use futures_util::future::BoxFuture;
use memory_rows::Fault;
use memory_rows::memories::{self, AddAsked, Asked, Handle, Refused, Work};
use memory_rows::notes::{NoteAsked, note};
use memory_rows::people::{Known, KnownRoom, KnownRooms};
use memory_rows::watches::{LaunchWatch, WatchArguments, watch};

use support::scratch::Scratch;
use support::{AGENT, OTHER_AGENT, OWNER, ROOM};

const NOW: i64 = 1_800_000_000;

// The archive a test's rows are read against: one group room under a title, and whether every text waits on
// something happening.
struct Archive {
  waits: bool,
}

impl Known for Archive {
  fn rooms(&self) -> BoxFuture<'_, Result<KnownRooms, Fault>> {
    Box::pin(async {
      Ok(KnownRooms {
        rooms: vec![KnownRoom {
          chat_id: ROOM,
          title: Some("Garden Club".to_owned()),
        }],
        senders: vec![OWNER],
      })
    })
  }

  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<Option<String>>, Fault>> {
    Box::pin(async move { Ok(people.iter().map(|_| None).collect()) })
  }

  fn agent_names(&self) -> BoxFuture<'_, Result<Vec<String>, Fault>> {
    Box::pin(async { Ok(vec![]) })
  }

  fn waits_on_happening<'a>(&'a self, _text: &'a str) -> BoxFuture<'a, Result<bool, Fault>> {
    let waits = self.waits;

    Box::pin(async move { Ok(waits) })
  }
}

// A launcher whose runs come up or do not.
struct Launcher(bool);

impl LaunchWatch for Launcher {
  fn launch(&self, _id: i64) -> BoxFuture<'_, Result<bool, Fault>> {
    let up = self.0;

    Box::pin(async move { Ok(up) })
  }
}

const QUIET: Archive = Archive { waits: false };
const UP: Launcher = Launcher(true);

fn parses(_check: &str) -> bool {
  true
}

fn work<'a>(scratch: &'a Scratch, known: &'a Archive, launch: &'a Launcher) -> Work<'a> {
  Work {
    db: scratch.db(),
    owner_id: Some(OWNER),
    known,
    launch,
    parses: &parses,
  }
}

fn asked(kind: &str, action: &str) -> AddAsked {
  AddAsked {
    agent: AGENT.to_owned(),
    scope: Some(ROOM),
    kind: Some(kind.to_owned()),
    action: action.to_owned(),
    ..AddAsked::default()
  }
}

fn coded(refused: &Refused) -> (&'static str, &[(&'static str, String)]) {
  (refused.code, &refused.parameters)
}

#[tokio::test]
async fn a_note_is_kept_for_its_room_or_for_every_room() {
  let scratch = Scratch::new().await;
  let room = NoteAsked {
    agent: AGENT,
    chat_id: ROOM,
    text: " the plants want water on Fridays ",
    author: Some(OWNER),
    shared: false,
  };

  assert_eq!(
    note(scratch.db(), &room, NOW).await.unwrap().unwrap(),
    vec!["note 1 kept for chat -100"]
  );
  assert_eq!(
    note(
      scratch.db(),
      &NoteAsked {
        shared: true,
        ..room
      },
      NOW
    )
    .await
    .unwrap()
    .unwrap(),
    vec!["note 2 kept for every room, from chat -100"]
  );
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "notes" WHERE "agent" = $1 AND "text" = 'the plants want water on Fridays'"#,
        vec![AGENT.into()]
      )
      .await,
    2
  );

  let empty = note(scratch.db(), &NoteAsked { text: " ", ..room }, NOW).await.unwrap().unwrap_err();

  assert_eq!(coded(&empty), ("usage", &[("verb", "note_add".to_owned())][..]));
}

#[tokio::test]
async fn a_row_is_filed_named_and_listed() {
  let scratch = Scratch::new().await;
  let work = work(&scratch, &QUIET, &UP);

  assert_eq!(
    memories::add(&work, &asked("promise", "send the photos"), NOW).await.unwrap(),
    vec!["memory 1 written for chat -100 as 'memory-1'"]
  );

  let listed = memories::list(scratch.db(), AGENT, Some(ROOM), None, false).await.unwrap();

  assert_eq!(listed.len(), 1);
  assert!(listed[0].starts_with("[on ] #1 promise · memory-1 · chat -100 · set "), "{}", listed[0]);
  assert_eq!(
    memories::list(scratch.db(), OTHER_AGENT, Some(ROOM), None, false).await.unwrap(),
    vec!["chat -100 has no open memories"]
  );
}

#[tokio::test]
async fn an_errand_waiting_on_a_happening_needs_what_wakes_it() {
  let scratch = Scratch::new().await;
  let waiting = Archive { waits: true };
  let work = work(&scratch, &waiting, &UP);
  let refused = memories::add(&work, &asked("promise", "tell them once it is out"), NOW)
    .await
    .unwrap_err();

  assert_eq!(refused.code, "memory-refused");
  assert!(
    refused.reason.starts_with("this row waits for something to happen first"),
    "{}",
    refused.reason
  );

  let woken = AddAsked {
    on: Some("deployed:#773".to_owned()),
    ..asked("promise", "tell them once it is out")
  };

  assert_eq!(
    memories::add(&work, &woken, NOW).await.unwrap(),
    vec!["memory 1 written for chat -100 as 'memory-1'"]
  );
  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "memories" WHERE "event" = 'deployed:773'"#, vec![])
      .await,
    1
  );
}

#[tokio::test]
async fn a_situation_row_is_a_pair_and_is_never_done() {
  let scratch = Scratch::new().await;
  let work = work(&scratch, &QUIET, &UP);
  let unpaired = memories::add(&work, &asked("situation", "answer in English"), NOW).await.unwrap_err();

  assert_eq!(coded(&unpaired), ("situation-unpaired", &[][..]));

  let paired = AddAsked {
    slug: Some("english-answers".to_owned()),
    condition: Some("someone writes in English".to_owned()),
    ..asked("situation", "answer in English")
  };

  memories::add(&work, &paired, NOW).await.unwrap();

  let handle = Handle::Id(1);
  let unfinished = memories::done(
    scratch.db(),
    &Asked {
      agent: AGENT,
      scope: Some(ROOM),
      handle: &handle,
    },
    None,
    NOW,
  )
  .await
  .unwrap_err();

  assert_eq!(
    coded(&unfinished),
    (
      "situation-unfinished",
      &[("handle", "1".to_owned()), ("slug", "english-answers".to_owned())][..]
    )
  );
  assert!(
    unfinished.reason.contains("--retire english-answers --why '<reason>'"),
    "{}",
    unfinished.reason
  );
}

#[tokio::test]
async fn a_global_row_names_no_room_and_lists_under_its_reach() {
  let scratch = Scratch::new().await;
  let work = work(&scratch, &QUIET, &UP);
  let naming = AddAsked {
    scope: None,
    ..asked("followup", "ask the garden club about seeds")
  };
  let refused = memories::add(&work, &naming, NOW).await.unwrap_err();

  assert!(refused.reason.ends_with("this one names room title Garden Club"), "{}", refused.reason);

  let plain = AddAsked {
    scope: None,
    todo_id: Some(12),
    ..asked("followup", "keep answers short")
  };

  assert_eq!(
    memories::add(&work, &plain, NOW).await.unwrap(),
    vec!["memory 1 written globally, reaching owner rooms as 'memory-1'"]
  );

  let listed = memories::list(scratch.db(), AGENT, None, None, false).await.unwrap();

  assert!(listed[0].contains(" · global · owner · set "), "{}", listed[0]);
  assert!(listed[0].contains(" · open strand of item #12"), "{}", listed[0]);
}

#[tokio::test]
async fn a_row_with_until_stands_with_its_watch_or_not_at_all() {
  let scratch = Scratch::new().await;
  let until = AddAsked {
    until: Some("test -f /tmp/built".to_owned()),
    name: Some("the build".to_owned()),
    ..asked("promise", "tell the room the build is done")
  };
  let lines = memories::add(&work(&scratch, &QUIET, &UP), &until, NOW).await.unwrap();

  assert_eq!(lines[0], "memory 1 written for chat -100 as 'memory-1'");
  assert!(
    lines[1].starts_with("watch #1 (the build): checking every 60 seconds until "),
    "{}",
    lines[1]
  );
  assert!(lines[1].ends_with(", serving memory #1"), "{}", lines[1]);

  let handle = Handle::Id(1);
  let retiring = Asked {
    agent: AGENT,
    scope: Some(ROOM),
    handle: &handle,
  };
  let no_why = memories::retire(scratch.db(), &retiring, Some(" "), NOW).await.unwrap_err();

  assert_eq!(coded(&no_why), ("why-missing", &[][..]));
  assert_eq!(
    memories::retire(scratch.db(), &retiring, Some("no longer needed"), NOW).await.unwrap(),
    vec![
      "memory 'memory-1' retired; its row stands with what it was and why it came down",
      "watch #1 (the build) cancelled with memory #1",
    ]
  );

  let down = Launcher(false);
  let refused = memories::add(&work(&scratch, &QUIET, &down), &until, NOW).await.unwrap_err();

  assert_eq!(refused.lines, vec!["memory 2 written for chat -100 as 'memory-2'"]);
  assert_eq!(
    refused.reason,
    "the watch could not be set, so memory 2 was taken down again: #2 could not be launched, so nothing is watching it"
  );
  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "memories" WHERE "id" = 2 AND "active" = 0"#, vec![])
      .await,
    1
  );
}

#[tokio::test]
async fn a_state_moves_on_only_with_where_it_stands() {
  let scratch = Scratch::new().await;

  memories::add(&work(&scratch, &QUIET, &UP), &asked("state", "a game of chess, white to move"), NOW)
    .await
    .unwrap();

  let handle = Handle::Slug("memory-1".to_owned());
  let moving = Asked {
    agent: AGENT,
    scope: Some(ROOM),
    handle: &handle,
  };
  let missing = memories::update(scratch.db(), &moving, None, NOW).await.unwrap_err();

  assert_eq!(coded(&missing), ("state-missing", &[][..]));
  assert_eq!(
    memories::update(scratch.db(), &moving, Some("black to move"), NOW).await.unwrap(),
    vec!["memory 1 moved on for chat -100"]
  );
  assert_eq!(
    memories::done(scratch.db(), &moving, Some("white won"), NOW).await.unwrap(),
    vec!["memory 1 released for chat -100"]
  );
}

#[tokio::test]
async fn a_watch_set_alone_stands_or_is_refused_under_its_code() {
  let scratch = Scratch::new().await;
  let chat = ROOM.to_string();
  let by = format!("chat:{ROOM}");
  let arguments = WatchArguments {
    agent: AGENT,
    chat: Some(&chat),
    by: Some(&by),
    until: Some("test -f /tmp/built"),
    at: None,
    name: None,
    note: Some("tell the room the build is done"),
    every: None,
    within: None,
    item: None,
  };
  let lines = watch(scratch.db(), &arguments, &UP, &parses, NOW).await.unwrap().unwrap();

  assert!(lines[0].starts_with("watch #1 (tell the room the build is done): checking every 60 seconds"));

  let again = watch(scratch.db(), &arguments, &UP, &parses, NOW).await.unwrap().unwrap_err();

  assert_eq!(coded(&again), ("memory-refused", &[][..]));
  assert_eq!(again.reason, "already watching this as #1");

  let unread = WatchArguments {
    note: None,
    ..arguments
  };
  let refused = watch(scratch.db(), &unread, &UP, &parses, NOW).await.unwrap().unwrap_err();

  assert_eq!(coded(&refused), ("usage", &[("verb", "watch_set".to_owned())][..]));
  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "watches" WHERE "agent" = $1"#, vec![AGENT.into()])
      .await,
    1
  );
}
