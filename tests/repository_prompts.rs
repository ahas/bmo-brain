//! The repository's own prompts as the editors load them (docs/boundary-api.md, 「Its prompts」): every editor's
//! task under `prompts/` composed behind the rulebooks with every mark filled, the Korean words the prompts quote
//! read from the policy, and the prompt files themselves holding no Korean and no name of their own.

mod support;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use bmo_brain::editors::archive::Note;
use bmo_brain::editors::book::PersonPiece;
use bmo_brain::editors::memo::{DigestNotes, digest_prompt};
use bmo_brain::editors::people::prompts::{
  BatchOptions, Stretch, TrimPrompt, people_prompt, sort_prompt, split_prompt, trim_prompt,
};
use bmo_brain::editors::prompts::{Prompts, filled};
use bmo_brain::policy::PROMPT_WORDS;
use regex::Regex;

use support::agency::FakeAgency;
use support::receiver::message;

const LABEL: &str = "Agent A";

fn repository_prompts() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts")
}

fn word(name: &str) -> &'static str {
  PROMPT_WORDS
    .words
    .get(name)
    .map(String::as_str)
    .unwrap_or_else(|| panic!("no word {name}"))
}

// Every prompt file under the repository's prompts folder, relative to it.
fn prompt_files() -> Vec<(String, String)> {
  let root = repository_prompts();
  let mut found = vec![];

  for folder in ["digest", "people", "session-summary", "rules"] {
    for entry in std::fs::read_dir(root.join(folder)).unwrap() {
      let path = entry.unwrap().path();

      found.push((
        path.strip_prefix(&root).unwrap().to_string_lossy().into_owned(),
        std::fs::read_to_string(&path).unwrap(),
      ));
    }
  }

  found.sort();
  found
}

// A composed prompt checked whole: every rulebook in order, no mark left, the agent named by its label where the
// fragment tool's rulebook names it, and the Korean words of the policy in place.
fn assert_composed(prompt: &str, task_words: &[&str]) {
  let headings: Vec<&str> = prompt.lines().filter(|line| line.starts_with("RULEBOOK ")).collect();

  assert_eq!(
    headings,
    [
      "RULEBOOK prompts/rules/document-writing.md:",
      "RULEBOOK prompts/rules/plain-writing.md:",
      "RULEBOOK prompts/rules/fragment-tool.md:",
      "RULEBOOK prompts/rules/recollection.md:",
      "RULEBOOK prompts/rules/self-preservation.md:",
    ]
  );
  assert!(!prompt.contains("{{"), "a mark is left: {prompt}");
  assert!(prompt.contains(&format!("A title never carries {LABEL},")));
  assert!(prompt.contains(&format!("(when, if, only, {}, {})", word("ko_when"), word("ko_case"))));
  assert!(prompt.contains(&format!("\"keywords\": \"{}\"", word("ko_address_keywords"))));

  for task_word in task_words {
    assert!(prompt.contains(task_word), "{task_word} is not in the prompt");
  }
}

#[tokio::test]
async fn every_editor_loads_its_task_from_the_repositorys_prompts_with_every_mark_filled() {
  let agency = FakeAgency::default();
  let root = repository_prompts();
  let prompts = Prompts {
    agency: &agency,
    root: &root,
  };
  let notes = [Note {
    id: 1,
    agent: "agent-a".into(),
    chat_id: -100,
    author: Some(42),
    text: "the meeting moved to Friday".into(),
    shared: false,
    created_at: 1_767_323_045,
  }];
  let names = BTreeMap::from([(42, "Owner".to_owned())]);
  let digest = digest_prompt(
    prompts,
    &DigestNotes {
      agent: LABEL,
      chat_id: -100,
      notes: &notes,
      owners: &HashSet::from([1]),
      forbidden_titles: &["Family Room".to_owned()],
      names: &names,
      documents: "None.",
    },
  )
  .await
  .unwrap();

  assert_composed(
    &digest,
    &[
      word("ko_from_now_on"),
      &format!(
        "`{}`, `{}`, `{}` and `{}` become",
        word("ko_facts"),
        word("ko_views"),
        word("ko_judgements"),
        word("ko_rules")
      ),
      &format!("`{}` becomes `Source room`", word("ko_source_room")),
      "the meeting moved to Friday",
    ],
  );

  let batch = [message(-100, 1, 7, 1_767_323_045)];
  let people =
    people_prompt(prompts, LABEL, -100, Some("Family Room"), 99, &batch, &BatchOptions::default())
      .await
      .unwrap();

  assert_composed(
    &people,
    &[
      &format!("- {}, names and nicknames", word("ko_address")),
      &format!("(\"{}\", \"{}\")", word("ko_ending_a"), word("ko_ending_b")),
      &format!("\"{}\"", word("ko_working_request")),
      "message 1",
    ],
  );

  let split = split_prompt(
    prompts,
    LABEL,
    -100,
    None,
    &Stretch {
      user_id: 7,
      span: "an older stretch of the book",
      number: 1,
      count: 2,
      recorded_on: "2026-01-02",
    },
  )
  .await
  .unwrap();

  assert_composed(
    &split,
    &[&format!(
      "{}, {}, {}",
      word("ko_speech_levels"),
      word("ko_elder_brother"),
      word("ko_elder_sister")
    )],
  );

  let group = [PersonPiece {
    path: "rooms/-100/people/chunks/p7-habits.md".into(),
    text: "- [date 2026-01-02 · confirmed 2026-01-02] says hello".into(),
  }];
  let sort = sort_prompt(prompts, LABEL, -100, None, 7, Some("Person"), &group, &[]).await.unwrap();

  assert_composed(&sort, &[&format!("Korean stays for {},", word("ko_address")), "p7-habits"]);

  let trim = trim_prompt(&TrimPrompt {
    prompts,
    agent: LABEL,
    chat_id: Some(-100),
    chat_title: None,
    user_id: 7,
    display_name: Some("Person"),
    heading: "habits",
    piece_path: "rooms/-100/people/chunks/p7-habits.md",
    piece: "",
    holding_path: "holding.md",
    dropped_path: "dropped.md",
    chunks_path: "rooms/-100/people/chunks",
    stretch: "a stretch",
    item_count: 1,
    total_items: 1,
    item_states: "none",
    forbidden_titles: &[],
  })
  .await
  .unwrap();

  assert_composed(&trim, &[&format!("the {} and other names", word("ko_address"))]);
}

#[test]
fn the_chat_session_summary_task_fills_its_agent_and_its_room_speaker() {
  let task = std::fs::read_to_string(repository_prompts().join("session-summary/chat.md")).unwrap();
  let prompt = filled(&task, &[("agent", LABEL)]);

  assert!(!prompt.contains("{{"), "{prompt}");
  assert!(prompt.contains(&format!(
    "Lines beginning `{}:` are the room; lines beginning `{LABEL}:` are",
    word("ko_room")
  )));
}

#[test]
fn the_prompt_files_hold_no_korean_and_every_korean_mark_has_its_word() {
  let mark = Regex::new(r"\{\{(ko_\w+)\}\}").unwrap();
  let mut used = BTreeSet::new();
  let files = prompt_files();

  assert_eq!(files.len(), 8, "{:?}", files.iter().map(|(path, _)| path).collect::<Vec<_>>());

  for (path, text) in &files {
    assert!(
      !text.chars().any(|character| ('\u{AC00}'..='\u{D7A3}').contains(&character)),
      "{path} holds Korean"
    );

    for found in mark.captures_iter(text) {
      used.insert(found[1].to_owned());
    }
  }

  assert_eq!(used, PROMPT_WORDS.words.keys().cloned().collect::<BTreeSet<_>>());
}
