//! What an editor that writes the brain's pieces is handed: the rulebooks every such editor reads, the
//! agency's and this repository's, before its task with its marks filled.

mod support;

use std::fs;

use bmo_brain::editors::prompts::{
  PromptFailure, Prompts, forbidden_titles_text, fragment_writer, render, seoul_timestamp,
};

use support::agency::FakeAgency;

#[test]
fn a_moment_reads_in_korean_time_with_its_offset() {
  assert_eq!(seoul_timestamp(1_767_323_045), "2026-01-02T12:04:05+09:00");
}

#[test]
fn forbidden_titles_are_each_quoted_once_or_none() {
  assert_eq!(forbidden_titles_text(&[]), "none");
  assert_eq!(
    forbidden_titles_text(&[" Family Room ".into(), "Family Room".into(), "".into(), "Work".into()]),
    "\"Family Room\", \"Work\""
  );
}

#[test]
fn a_mark_is_filled_where_it_is_named_and_left_where_it_is_not() {
  assert_eq!(render("{{a}} and {{b}} and {a}", &[("a", "one")]), "one and {{b}} and {a}");
}

#[tokio::test]
async fn a_writer_reads_the_five_rulebooks_in_order_before_its_task() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  fs::create_dir_all(root.join("rules")).unwrap();
  fs::create_dir_all(root.join("people")).unwrap();
  fs::write(root.join("rules/fragment-tool.md"), "\nThe fragment tool rulebook.\n").unwrap();
  fs::write(root.join("rules/recollection.md"), "The recollection rulebook.").unwrap();
  fs::write(root.join("people/task.md"), "Write the book of {{agent}}.\n").unwrap();

  let agency = FakeAgency::default();
  let prompts = Prompts {
    agency: &agency,
    root,
  };
  let prompt = fragment_writer(prompts, "people", "task.md", &[("agent", "agent-a")]).await.unwrap();

  assert_eq!(
    prompt,
    [
      "RULEBOOK prompts/rules/document-writing.md:\nThe document-writing rulebook.",
      "RULEBOOK prompts/rules/plain-writing.md:\nThe plain-writing rulebook.",
      "RULEBOOK prompts/rules/fragment-tool.md:\nThe fragment tool rulebook.",
      "RULEBOOK prompts/rules/recollection.md:\nThe recollection rulebook.",
      "RULEBOOK prompts/rules/self-preservation.md:\nThe self-preservation rulebook.",
      "Write the book of agent-a.",
    ]
    .join("\n\n")
  );
  assert_eq!(agency.calls.of("rulebooks").len(), 1);
}

#[tokio::test]
async fn a_task_that_is_not_there_is_no_prompt() {
  let dir = tempfile::tempdir().unwrap();
  let agency = FakeAgency::default();
  let prompts = Prompts {
    agency: &agency,
    root: dir.path(),
  };

  assert!(matches!(
    fragment_writer(prompts, "people", "task.md", &[]).await,
    Err(PromptFailure::Unread { .. })
  ));
}

#[tokio::test]
async fn a_rulebook_the_agency_does_not_hand_over_is_left_out_with_its_heading() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();

  fs::create_dir_all(root.join("rules")).unwrap();
  fs::create_dir_all(root.join("people")).unwrap();
  fs::write(root.join("rules/fragment-tool.md"), "The fragment tool rulebook.").unwrap();
  fs::write(root.join("rules/recollection.md"), "The recollection rulebook.").unwrap();
  fs::write(root.join("people/task.md"), "Write the book.").unwrap();

  let agency = FakeAgency::default();
  *agency.no_rulebooks.lock().unwrap() = true;
  let prompts = Prompts {
    agency: &agency,
    root,
  };
  let prompt = fragment_writer(prompts, "people", "task.md", &[]).await.unwrap();

  assert_eq!(
    prompt,
    [
      "RULEBOOK prompts/rules/fragment-tool.md:\nThe fragment tool rulebook.",
      "RULEBOOK prompts/rules/recollection.md:\nThe recollection rulebook.",
      "Write the book.",
    ]
    .join("\n\n")
  );
}
