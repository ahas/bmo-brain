//! What the memory operations read and say without the database: an event asked for and its coded
//! refusals, a handle, a listing line's marks, and the names a global row is checked against.

use futures_util::future::BoxFuture;
use memory_rows::Fault;
use memory_rows::events::Event;
use memory_rows::memories::{
  GlobalNames, Handle, Memory, event_asked, global_names, global_violations, handle_of, memory_line,
};
use memory_rows::people::{Known, KnownRoom, KnownRooms};

const OWNER: i64 = 42;
const PERSON: i64 = 7;
const ROOM: i64 = -100;

// What a test's archive knows: a group room, the owner's room under the agent's label, and two senders.
struct Archive;

impl Known for Archive {
  fn rooms(&self) -> BoxFuture<'_, Result<KnownRooms, Fault>> {
    Box::pin(async {
      Ok(KnownRooms {
        rooms: vec![
          KnownRoom {
            chat_id: ROOM,
            title: Some("Garden Club".to_owned()),
          },
          KnownRoom {
            chat_id: OWNER,
            title: Some(" Agent A ".to_owned()),
          },
          KnownRoom {
            chat_id: -200,
            title: None,
          },
        ],
        senders: vec![PERSON, OWNER, PERSON],
      })
    })
  }

  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<Option<String>>, Fault>> {
    Box::pin(async move {
      Ok(
        people
          .iter()
          .map(|person| match *person {
            PERSON => Some("Robin".to_owned()),
            OWNER => Some("The Owner".to_owned()),
            _ => None,
          })
          .collect(),
      )
    })
  }

  fn agent_names(&self) -> BoxFuture<'_, Result<Vec<String>, Fault>> {
    Box::pin(async { Ok(vec!["agent a".to_owned()]) })
  }

  fn waits_on_happening<'a>(&'a self, _text: &'a str) -> BoxFuture<'a, Result<bool, Fault>> {
    Box::pin(async { Ok(false) })
  }
}

fn row() -> Memory {
  Memory {
    id: 3,
    agent: "agent-a".to_owned(),
    slug: "water-the-plants".to_owned(),
    kind: "followup".to_owned(),
    chat_id: None,
    reach: "group".to_owned(),
    condition: String::new(),
    action: "water the plants".to_owned(),
    for_user: None,
    todo_id: Some(12),
    set_at: 0,
    active: true,
    last_hit_at: None,
    hits: 0,
    done_at: None,
    done_text: None,
    down_reason: None,
    down_at: None,
    candidate_why: None,
    event: None,
    event_met_at: None,
    moved_at: None,
  }
}

#[test]
fn an_event_is_asked_for_with_its_item() {
  assert_eq!(
    event_asked(" deployed:#773 ").unwrap(),
    Event {
      kind: "deployed".to_owned(),
      item: Some(773),
    }
  );
  assert_eq!(event_asked("queue-changed").unwrap().item, None);
}

#[test]
fn an_event_outside_the_vocabulary_is_refused_with_the_kinds() {
  let refused = event_asked("elsewhere").unwrap_err();

  assert_eq!(refused.code, "event-unknown");
  assert_eq!(
    refused.parameters,
    vec![
      ("given", "elsewhere".to_owned()),
      ("kinds", "deployed[:<item>], job-ended[:<item>], queue-changed".to_owned()),
    ]
  );
  assert_eq!(
    refused.reason,
    "unknown event: elsewhere. Use one of deployed[:<item>], job-ended[:<item>], queue-changed."
  );
  assert!(event_asked("").unwrap_err().reason.starts_with("unknown event: (empty)."));
  assert_eq!(event_asked("deployed:1:2").unwrap_err().code, "event-unknown");
}

#[test]
fn an_item_on_the_wrong_event_or_a_bad_item_is_refused_by_its_code() {
  let no_item = event_asked("queue-changed:5").unwrap_err();

  assert_eq!(no_item.code, "event-takes-no-item");
  assert_eq!(
    no_item.parameters,
    vec![("kind", "queue-changed".to_owned()), ("given", "queue-changed:5".to_owned())]
  );

  let bad = event_asked("deployed:soon").unwrap_err();

  assert_eq!(bad.code, "event-bad-item");
  assert_eq!(bad.parameters, vec![("given", "deployed:soon".to_owned())]);
  assert_eq!(event_asked("job-ended:0").unwrap_err().code, "event-bad-item");
}

#[test]
fn a_handle_is_a_number_or_a_slug() {
  assert_eq!(handle_of(Some(" 12 "), "--done").unwrap(), Handle::Id(12));
  assert_eq!(handle_of(Some("a-slug"), "--done").unwrap(), Handle::Slug("a-slug".to_owned()));

  let refused = handle_of(None, "--done").unwrap_err();

  assert_eq!(refused.code, "memory-refused");
  assert_eq!(refused.reason, "--done takes the row's id or its slug");
}

#[test]
fn a_listing_line_marks_a_global_row_and_a_handed_over_strand_in_english() {
  let line = memory_line(&row());

  assert!(
    line.starts_with(
      "[on ] #3 followup · water-the-plants · global · group · set 1970-01-01 09:00 KST · open strand of item #12"
    ),
    "{line}"
  );
  assert!(line.ends_with("      then: water the plants"), "{line}");

  let room_row = Memory {
    chat_id: Some(ROOM),
    todo_id: None,
    ..row()
  };

  assert!(memory_line(&room_row).contains(" · chat -100 · "));
}

#[tokio::test]
async fn the_global_names_leave_out_the_owner_and_the_titles_agents_go_by() {
  let names = global_names(&Archive, Some(OWNER)).await.unwrap();

  assert_eq!(names.user_ids, vec![PERSON, OWNER]);
  assert_eq!(names.chat_ids, vec![ROOM, OWNER, -200]);
  assert_eq!(names.titles, vec!["Garden Club".to_owned()]);
  assert_eq!(names.people, vec!["Robin".to_owned()]);
}

#[test]
fn a_global_row_may_name_no_room_person_or_id() {
  let names = GlobalNames {
    user_ids: vec![PERSON],
    chat_ids: vec![ROOM],
    titles: vec!["Garden Club".to_owned()],
    people: vec!["Robin".to_owned()],
  };

  assert_eq!(
    global_violations("ask robin about the garden club in -100, see id 7", &names),
    vec![
      "chat id -100".to_owned(),
      "user id 7".to_owned(),
      "room title Garden Club".to_owned(),
      "person Robin".to_owned(),
      "'id 7'".to_owned(),
    ]
  );
  assert!(global_violations("check the 2026-10-08 build", &names).is_empty());
}
