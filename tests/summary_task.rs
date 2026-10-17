//! The chat-session summariser's task as the brain serves it (docs/boundary-api.md, 「Chat-session summaries」): the
//! recollection rulebook and the chat session's task from this repository's prompts, their marks filled for the
//! agent by its label or its name, and a prompts folder that cannot be read answered as a failure.

mod support;

use std::path::PathBuf;

use bmo_brain::editors::prompts::filled;
use bmo_brain::policy::PROMPT_WORDS;
use bmo_brain::ports::agency::Registration;
use bmo_brain::ports::http::PortFailure;
use serde_json::{Value, json};

use support::agency::registration;
use support::served::{Served, served_on};
use support::{AGENT, OTHER_AGENT};

fn repository_prompts() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts")
}

// The task as the summariser would be handed it, from the repository's own files, for an agent called `name`.
fn expected(name: &str) -> String {
  let read =
    |path: &str| std::fs::read_to_string(repository_prompts().join(path)).unwrap().trim().to_owned();
  let values = [("agent", name)];

  format!(
    "RULEBOOK prompts/rules/recollection.md:\n{}\n\n{}",
    filled(&read("rules/recollection.md"), &values),
    filled(&read("session-summary/chat.md"), &values)
  )
}

async fn task(served: &Served, agent: &str) -> String {
  let answer: Value = served.post("/session-summary/task", json!({ "agent": agent })).await;

  answer["task"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn the_task_names_a_labelled_agent_by_its_label_with_every_mark_filled() {
  let served = served_on(repository_prompts()).await;

  let task = task(&served, AGENT).await;

  assert_eq!(task, expected("Helper"));
  assert!(task.starts_with("RULEBOOK prompts/rules/recollection.md:\n"));
  assert!(!task.contains("{{"), "a mark was left: {task}");
  assert!(task.contains(&format!("Lines beginning `{}:` are the room", PROMPT_WORDS.words["ko_room"])));
  assert!(task.contains("lines beginning `Helper:` are"));
  assert_eq!(served.agency.calls.of("registrations"), [json!({ "agent": AGENT })]);
}

#[tokio::test]
async fn an_agent_with_no_label_is_named_by_its_name_and_so_is_one_the_agency_does_not_register() {
  let served = served_on(repository_prompts()).await;

  served.agency.registrations.lock().unwrap().push(Registration {
    label: Some("  ".to_owned()),
    ..registration(OTHER_AGENT)
  });

  for agent in [OTHER_AGENT, "agent-z"] {
    let task = task(&served, agent).await;

    assert_eq!(task, expected(agent));
    assert!(!task.contains("{{"), "a mark was left: {task}");
  }
}

#[tokio::test]
async fn a_prompts_folder_that_cannot_be_read_answers_failed_and_a_nameless_ask_is_refused() {
  let served = served_on(PathBuf::from("/nonexistent/prompts")).await;

  let failure = served
    .caller
    .post::<Value>("/session-summary/task", &json!({ "agent": AGENT }))
    .await
    .unwrap_err();

  let PortFailure::Answered { status, answer, .. } = failure else { panic!("{failure}") };
  let answer = answer.unwrap();
  assert_eq!((status, answer.code.as_str()), (500, "failed"));
  assert!(answer.error.contains("recollection.md"), "{}", answer.error);

  let nameless = served
    .caller
    .post::<Value>("/session-summary/task", &json!({ "agent": " " }))
    .await
    .unwrap_err();

  assert!(matches!(nameless, PortFailure::Answered { status: 400, .. }), "{nameless}");
}
