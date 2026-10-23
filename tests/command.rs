//! The brain's command (docs/boundary-api.md, 「The command」): the nouns it declares and, for each noun, the
//! words it takes and the route they call, run through the binary as `bmo` runs it.

mod support;

use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU32, Ordering};

use bmo_brain::command::NOUNS;
use support::{Piece, Row, Served, TOKEN, labels, piece, row};
use tokio::process::Command;

/// A directory of the test's own, holding the configuration the binary is given.
struct Workspace {
  folder: PathBuf,
  config: PathBuf,
}

impl Workspace {
  fn new(listen: &str) -> Workspace {
    static MADE: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
      "bmo-brain-command-{}-{}",
      std::process::id(),
      MADE.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&folder).expect("the workspace is made");
    let config = folder.join("brain.toml");
    std::fs::write(
      &config,
      format!("listen = \"{listen}\"\ntoken = \"{TOKEN}\"\ndatabase = \"postgres:///unused\"\n"),
    )
    .expect("the configuration is written");
    Workspace { folder, config }
  }

  fn at(served: &Served) -> Workspace {
    Workspace::new(served.base.strip_prefix("http://").expect("the base is an address"))
  }

  async fn run(&self, words: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bmo-brain"))
      .arg("--config")
      .arg(&self.config)
      .args(words)
      .kill_on_drop(true)
      .output()
      .await
      .expect("the binary runs")
  }
}

impl Drop for Workspace {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.folder);
  }
}

fn stdout(output: &Output) -> String {
  String::from_utf8(output.stdout.clone()).expect("the answer is UTF-8")
}

fn stderr(output: &Output) -> String {
  String::from_utf8(output.stderr.clone()).expect("the refusal is UTF-8")
}

fn succeeded(output: &Output) -> String {
  assert!(output.status.success(), "the command was refused: {}", stderr(output));
  stdout(output)
}

#[tokio::test]
async fn the_nouns_are_printed_one_a_line() {
  let workspace = Workspace::new("127.0.0.1:1");

  let printed = succeeded(&workspace.run(&["nouns"]).await);
  let expected: String = NOUNS.iter().map(|noun| format!("{noun}\n")).collect();

  assert_eq!(printed, expected);
  assert_eq!(NOUNS, &["memory", "fragment"][..]);
}

#[tokio::test]
async fn a_memory_row_is_listed_and_moved_on() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);
  let id = served
    .put_row(Row {
      labels: labels(&[("source", "talk")]),
      due_at: Some(5000),
      ..row("room:a", "promise", "Bring the Report back")
    })
    .await
    .to_string();

  let listed =
    succeeded(&workspace.run(&["memory", "list", "--scope", "room:a", "--state", "standing"]).await);
  assert_eq!(
    listed.trim(),
    format!(
      "memory: {id} [standing] promise: Bring the Report back (scope room:a, due 1970-01-01T00:00:05.000Z, \
       labels source=talk)"
    )
  );

  let through_road = succeeded(
    &workspace
      .run(&["memory", "--via", "r", "list", "--scope", "room:a", "--state", "standing"])
      .await,
  );
  assert_eq!(through_road, listed);

  let done = succeeded(&workspace.run(&["memory", "done", &id, "--text", "Brought it back"]).await);
  assert!(done.contains("[done]"), "{done}");
  assert!(done.contains("Brought it back"), "{done}");

  let revived = succeeded(&workspace.run(&["memory", "revive", &id]).await);
  assert!(revived.contains("[standing]"), "{revived}");

  let updated = succeeded(
    &workspace
      .run(&["memory", "update", &id, "--state", "candidate", "--label", "owner=writer-a", "--no-due"])
      .await,
  );
  assert!(updated.contains("[candidate]"), "{updated}");
  assert!(updated.contains("labels owner=writer-a"), "{updated}");
  assert!(!updated.contains("due "), "{updated}");

  let retired = succeeded(&workspace.run(&["memory", "retire", &id]).await);
  assert!(retired.contains("[retired]"), "{retired}");

  let none = succeeded(&workspace.run(&["memory", "list", "--scope", "room:b"]).await);
  assert_eq!(none.trim(), "memory: no rows.");
}

#[tokio::test]
async fn a_fragment_is_listed_and_switched() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);
  served
    .put_fragment(Piece {
      title: Some("Tea".into()),
      ..piece("tea", "knowledge", Some("room:a"), "Tea at five")
    })
    .await;

  let listed = succeeded(&workspace.run(&["fragment", "list", "--scope", "room:a"]).await);
  assert_eq!(listed.trim(), "fragment: tea [active] knowledge room:a Tea (version v1)");

  let switched = succeeded(&workspace.run(&["fragment", "switch", "tea", "--state", "off"]).await);
  assert!(switched.contains("[off]"), "{switched}");
  let off_for = succeeded(&workspace.run(&["fragment", "switch", "tea", "--off-for", "room:b"]).await);
  assert!(off_for.contains("[off]"), "{off_for}");

  let none = succeeded(&workspace.run(&["fragment", "list", "--scope", "room:b"]).await);
  assert_eq!(none.trim(), "fragment: no fragments.");
}

#[tokio::test]
async fn a_form_the_command_does_not_take_ends_two() {
  let workspace = Workspace::new("127.0.0.1:1");

  for words in [
    &["memory", "add", "--scope", "room:a", "--kind", "promise", "--text", "Words"][..],
    &["fragment", "write", "tea", "--kind", "knowledge", "--text", "Words"][..],
    &["note", "--scope", "room:a", "--actor", "writer-a"][..],
    &["watch", "list"][..],
    &["brain", "wait"][..],
  ] {
    assert_eq!(workspace.run(words).await.status.code(), Some(2), "{words:?}");
  }
}

#[tokio::test]
async fn a_refusal_names_the_noun() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let refused = workspace.run(&["memory", "done", "999"]).await;

  assert_eq!(refused.status.code(), Some(1));
  assert!(
    stderr(&refused).contains("bmo-brain: memory: the brain answered 404 not-found"),
    "{}",
    stderr(&refused)
  );
}
