//! What the person-book writer is told: each message as it reads it, and its task filled from the batch.

mod support;

use std::fs;

use bmo_brain::editors::archive::Message;
use bmo_brain::editors::people::prompts::{
  BatchOptions, SUCCESS_MARKER, document_count_text, message_line, people_prompt,
};
use bmo_brain::editors::prompts::Prompts;

use support::agency::FakeAgency;
use support::receiver::message;

#[test]
fn a_message_reads_with_its_time_its_sender_and_what_it_answers() {
  let message = Message {
    chat_id: -5,
    message_id: 12,
    sender_id: Some(7),
    sender_name: "Tester".into(),
    sent_at: 1_767_323_045,
    text: "hello".into(),
    reply_to_message_id: Some(10),
  };

  assert_eq!(
    message_line(&message),
    "12 | 2026-01-02T12:04:05+09:00 | 7 | Tester | hello [replies to 10]"
  );
  assert_eq!(
    message_line(&Message {
      sender_id: None,
      reply_to_message_id: None,
      ..message
    }),
    "12 | 2026-01-02T12:04:05+09:00 | unknown | Tester | hello"
  );
}

#[test]
fn a_count_of_documents_reads_as_words() {
  assert_eq!(document_count_text(0), "no documents");
  assert_eq!(document_count_text(1), "1 document");
  assert_eq!(document_count_text(3), "3 documents");
}

#[tokio::test]
async fn a_batch_names_whom_it_writes_for_and_leaves_out_the_agents_own_messages() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  fs::create_dir_all(root.join("rules")).unwrap();
  fs::create_dir_all(root.join("people")).unwrap();
  fs::write(root.join("rules/fragment-tool.md"), "Fragment tool.").unwrap();
  fs::write(root.join("rules/recollection.md"), "Recollection.").unwrap();
  fs::write(
    root.join("people/task.md"),
    "{{agent}} in {{chatId}}{{chatTitleSuffix}} writes for {{writers}} into {{peopleDir}}: {{documentCount}}; end with {{successMarker}}",
  )
  .unwrap();

  let agency = FakeAgency::default();
  let prompts = Prompts {
    agency: &agency,
    root,
  };
  let batch = vec![message(-5, 1, 7, 0), message(-5, 2, 99, 0), message(-5, 3, 4, 0), message(-5, 4, 7, 0)];
  let prompt =
    people_prompt(prompts, "Helper", -5, Some("Family Room"), 99, &batch, &BatchOptions::default())
      .await
      .unwrap();

  assert!(prompt.ends_with(&format!(
    "Helper in -5 (title: Family Room) writes for 7, 4 into rooms/-5/people: no documents; end with {SUCCESS_MARKER}"
  )));
}
