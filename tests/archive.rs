//! What the editors read and mark: the notes on the brain's own table, the day a book starts reading on its
//! own settings, and the messages, marks, titles and roads through the receiver.

mod support;

use bmo_brain::editors::archive::{
  book_room, chat_title, chat_titles, delete_notes, forget_read, keeps_room_book, mark_read, own_number_in,
  pending_notes, people_counts, read_count, read_messages, reading_since, set_reading_since, spoke_between,
  unkept_rooms, unread_counts, unread_messages,
};
use bmo_brain::ports::receiver::{RoadKind, Roads, RoomAnswerers, RoomTitle};
use serde_json::json;

use support::receiver::{FakeReceiver, message};
use support::scratch::Scratch;
use support::{AGENT, OTHER_AGENT, OWNER, ROOM};

#[tokio::test]
async fn an_agents_unread_messages_are_its_own_and_start_from_its_books_day() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let room = -1_000_000_000_002;
  let (early, late) = (1_719_759_600, 1_786_287_600);
  let receiver = FakeReceiver::with_messages(vec![
    message(room, 1, 30_000_003, early),
    message(room, 2, 30_000_003, late),
  ]);

  assert_eq!(unread_counts(db, &receiver, AGENT).await.unwrap().get(&room), Some(&2));

  set_reading_since(db, AGENT, room, Some(late - 60)).await.unwrap();
  assert_eq!(reading_since(db, AGENT, room).await.unwrap(), Some(late - 60));
  assert_eq!(unread_counts(db, &receiver, AGENT).await.unwrap().get(&room), Some(&1));
  let unread = unread_messages(db, &receiver, AGENT, room, 10).await.unwrap();
  assert_eq!(unread.iter().map(|message| message.message_id).collect::<Vec<_>>(), [2]);
  assert_eq!(
    receiver.calls.of("unread_counts").last(),
    Some(&json!({ "agent": AGENT, "since": [{ "chatId": room, "since": late - 60 }] }))
  );

  set_reading_since(db, AGENT, room, None).await.unwrap();
  assert_eq!(reading_since(db, AGENT, room).await.unwrap(), None);
  mark_read(&receiver, AGENT, room, &[1]).await.unwrap();
  assert_eq!(unread_counts(db, &receiver, AGENT).await.unwrap().get(&room), Some(&1));
  assert_eq!(unread_counts(db, &receiver, OTHER_AGENT).await.unwrap().get(&room), Some(&2));
  assert_eq!(read_count(&receiver, AGENT, room).await.unwrap(), 1);
  let read = read_messages(&receiver, AGENT, room, 0, 10).await.unwrap();
  assert_eq!(read.iter().map(|message| message.message_id).collect::<Vec<_>>(), [1]);

  assert_eq!(forget_read(&receiver, AGENT, None).await.unwrap(), 1);
  assert_eq!(unread_counts(db, &receiver, AGENT).await.unwrap().get(&room), Some(&2));
}

#[tokio::test]
async fn a_person_spoke_in_a_room_only_where_their_message_stands_in_the_window() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let receiver = FakeReceiver::with_messages(vec![message(-5, 1, 7, 1000)]);
  scratch
    .run(
      r#"INSERT INTO "notes" ("chat_id", "author", "text", "created_at", "agent") VALUES ($1, $2, $3, $4, $5)"#,
      vec![(-5i64).into(), 7i64.into(), "a note".into(), 2000i64.into(), AGENT.into()],
    )
    .await;

  assert!(spoke_between(&receiver, -5, 7, 2000 - 3600, 2000).await.unwrap());
  assert!(!spoke_between(&receiver, -6, 7, 2000 - 3600, 2000).await.unwrap());
  assert!(!spoke_between(&receiver, -5, 8, 2000 - 3600, 2000).await.unwrap());

  let notes = pending_notes(db, Some(AGENT)).await.unwrap();
  assert_eq!(notes.len(), 1);
  assert_eq!((notes[0].chat_id, notes[0].author, notes[0].shared), (-5, Some(7), false));
  assert!(pending_notes(db, Some(OTHER_AGENT)).await.unwrap().is_empty());
  assert_eq!(pending_notes(db, None).await.unwrap().len(), 1);

  delete_notes(db, &[notes[0].id]).await.unwrap();
  assert!(pending_notes(db, None).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_one_to_one_with_the_chat_agent_keeps_no_room_part() {
  let receiver = FakeReceiver::with_messages(vec![message(7, 1, 7, 100), message(ROOM, 2, 7, 100)]);
  receiver.roads.lock().unwrap().insert(
    7,
    Roads {
      kind: RoadKind::Owner,
      one_to_one: true,
    },
  );
  *receiver.answerers.lock().unwrap() = [7, ROOM]
    .map(|chat_id| RoomAnswerers {
      chat_id,
      agents: vec![AGENT.to_owned()],
    })
    .to_vec();

  assert!(!keeps_room_book(&receiver, 7).await.unwrap());
  assert!(keeps_room_book(&receiver, ROOM).await.unwrap());
  assert!(!book_room(&receiver, AGENT, 7).await.unwrap());
  assert!(book_room(&receiver, AGENT, ROOM).await.unwrap());
  assert!(!book_room(&receiver, OTHER_AGENT, ROOM).await.unwrap());

  let brain = tempfile::tempdir().unwrap();
  for file in ["people/7/sheet.md", "rooms/7/people/7.md", "rooms/-100/people/7.md"] {
    let path = brain.path().join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "# 7\n").unwrap();
  }
  let unkept = unkept_rooms(&receiver, brain.path()).await.unwrap();
  assert_eq!(unkept.into_iter().collect::<Vec<_>>(), [7]);

  let scratch = Scratch::new().await;
  let counts = people_counts(scratch.db(), &receiver, AGENT).await.unwrap();
  assert_eq!(counts.into_iter().collect::<Vec<_>>(), [(ROOM, 1)]);
}

#[tokio::test]
async fn titles_and_the_agents_own_number_come_from_the_receiver() {
  let receiver = FakeReceiver::default();
  *receiver.titles.lock().unwrap() = vec![
    RoomTitle {
      chat_id: ROOM,
      title: Some("Garden Club".to_owned()),
      senders: vec![OWNER],
    },
    RoomTitle {
      chat_id: OWNER,
      title: None,
      senders: vec![OWNER],
    },
  ];
  receiver.own_numbers.lock().unwrap().insert((AGENT.to_owned(), ROOM), 900);

  assert_eq!(chat_title(&receiver, ROOM).await.unwrap().as_deref(), Some("Garden Club"));
  assert_eq!(chat_title(&receiver, OWNER).await.unwrap(), None);
  assert_eq!(chat_titles(&receiver).await.unwrap(), ["Garden Club"]);
  assert_eq!(own_number_in(&receiver, AGENT, ROOM).await.unwrap(), 900);
  assert_eq!(own_number_in(&receiver, OTHER_AGENT, ROOM).await.unwrap(), 0);
}
