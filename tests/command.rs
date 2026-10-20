//! The brain's command (docs/boundary-api.md, 「The command」): the nouns it declares and, for each noun, the
//! words it takes and the route they call, run through the binary as `bmo` runs it.

mod support;

use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU32, Ordering};

use bmo_brain::command::NOUNS;
use bmo_brain_client::{LibraryPut, NewReminder};
use support::{Served, TOKEN, labels};
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
  assert_eq!(
    NOUNS,
    &[
      "memory",
      "note",
      "fragment",
      "watch set",
      "watch list",
      "reminder list",
      "brain hold",
      "brain wait"
    ][..]
  );
}

#[tokio::test]
async fn a_memory_row_is_added_listed_and_moved_on() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let added = succeeded(
    &workspace
      .run(&[
        "memory",
        "add",
        "--scope",
        "room:a",
        "--kind",
        "promise",
        "--text",
        "Bring the Report back",
        "--label",
        "source=talk",
        "--due",
        "5000",
      ])
      .await,
  );
  assert_eq!(
    added.trim(),
    "memory: 1 [standing] promise: Bring the Report back (scope room:a, due 1970-01-01T00:00:05.000Z, \
     labels source=talk)"
  );

  let listed =
    succeeded(&workspace.run(&["memory", "list", "--scope", "room:a", "--state", "standing"]).await);
  assert_eq!(listed, added);

  let through_road = succeeded(
    &workspace
      .run(&["memory", "--via", "r", "list", "--scope", "room:a", "--state", "standing"])
      .await,
  );
  assert_eq!(through_road, added);

  let done = succeeded(&workspace.run(&["memory", "done", "1", "--text", "Brought it back"]).await);
  assert!(done.contains("[done]"), "{done}");
  assert!(done.contains("Brought it back"), "{done}");

  let revived = succeeded(&workspace.run(&["memory", "revive", "1"]).await);
  assert!(revived.contains("[standing]"), "{revived}");

  let updated = succeeded(
    &workspace
      .run(&["memory", "update", "1", "--state", "candidate", "--label", "owner=writer-a", "--no-due"])
      .await,
  );
  assert!(updated.contains("[candidate]"), "{updated}");
  assert!(updated.contains("labels owner=writer-a"), "{updated}");
  assert!(!updated.contains("due "), "{updated}");

  let retired = succeeded(&workspace.run(&["memory", "retire", "1"]).await);
  assert!(retired.contains("[retired]"), "{retired}");

  let none = succeeded(&workspace.run(&["memory", "list", "--scope", "room:b"]).await);
  assert_eq!(none.trim(), "memory: no rows.");
}

#[tokio::test]
async fn a_note_is_kept() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let kept = succeeded(
    &workspace
      .run(&["note", "--scope", "room:a", "--actor", "writer-a", "--text", "Tea at five"])
      .await,
  );

  assert_eq!(kept.trim(), "note: 1 room:a: Tea at five");
}

#[tokio::test]
async fn a_fragment_is_written_switched_listed_and_removed() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let written = succeeded(
    &workspace
      .run(&[
        "fragment",
        "write",
        "tea",
        "--kind",
        "knowledge",
        "--text",
        "Tea at five",
        "--scope",
        "room:a",
        "--title",
        "Tea",
      ])
      .await,
  );
  assert!(
    written.starts_with("fragment: tea [active] knowledge room:a Tea (version "),
    "{written}"
  );

  let listed = succeeded(&workspace.run(&["fragment", "list", "--scope", "room:a"]).await);
  assert_eq!(listed, written);

  let switched = succeeded(&workspace.run(&["fragment", "switch", "tea", "--state", "off"]).await);
  assert!(switched.contains("[off]"), "{switched}");

  served
    .client
    .library_put("shelf", &LibraryPut::default())
    .await
    .expect("the library is put");
  let synced = succeeded(&workspace.run(&["fragment", "sync", "shelf", "--dry-run"]).await);
  assert_eq!(synced.trim(), "fragment: shelf: in line with its documents.");

  let removed = succeeded(&workspace.run(&["fragment", "remove", "tea"]).await);
  assert_eq!(removed.trim(), "fragment: tea removed.");

  let none = succeeded(&workspace.run(&["fragment", "list", "--scope", "room:a"]).await);
  assert_eq!(none.trim(), "fragment: no fragments.");
}

#[tokio::test]
async fn a_watch_is_set_and_listed() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let set = succeeded(
    &workspace
      .run(&["watch", "set", "--scope", "room:a", "--setter", "writer-a", "--at", "5000"])
      .await,
  );
  assert!(set.starts_with("watch: 1 [armed] room:a {"), "{set}");
  assert!(set.contains("\"kind\":\"at\""), "{set}");

  let listed = succeeded(&workspace.run(&["watch", "list", "--scope", "room:a"]).await);
  assert_eq!(listed, set);

  let none = succeeded(&workspace.run(&["watch", "list", "--scope", "room:b"]).await);
  assert_eq!(none.trim(), "watch: no watches.");
}

#[tokio::test]
async fn a_reminder_is_listed_and_cancelled() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let kept = served
    .client
    .reminder_add(&NewReminder {
      scope: "room:b".to_owned(),
      due_at: 50_000,
      text: "Turn the kettle on".to_owned(),
      setter_name: "writer-a".to_owned(),
      labels: labels(&[]),
    })
    .await
    .expect("the reminder is set");
  let id = kept.id.to_string();

  let listed = succeeded(&workspace.run(&["reminder", "list", "--scope", "room:b"]).await);
  assert!(listed.starts_with(&format!("reminder: {} ", kept.id)), "{listed}");
  assert!(listed.contains("Turn the kettle on"), "{listed}");

  let cancelled = succeeded(&workspace.run(&["reminder", "list", "--cancel", &id]).await);
  assert_eq!(cancelled.trim(), format!("reminder: {id} cancelled."));

  let none = succeeded(&workspace.run(&["reminder", "list", "--scope", "room:b"]).await);
  assert_eq!(none.trim(), "reminder: no reminders.");
}

#[tokio::test]
async fn the_brain_is_held_and_waited_on() {
  let served = Served::new().await;
  let workspace = Workspace::at(&served);

  let held =
    succeeded(&workspace.run(&["brain", "hold", "--holder", "writer-a", "--reason", "editing"]).await);
  assert!(held.starts_with("brain: lock 1 held by writer-a until it is let go: editing"), "{held}");

  let waited = succeeded(&workspace.run(&["brain", "wait", "--wait-ms", "0"]).await);
  assert_eq!(waited, held);

  served.client.unlock(1).await.expect("the lock is let go");

  let free = succeeded(&workspace.run(&["brain", "wait", "--wait-ms", "0"]).await);
  assert_eq!(free.trim(), "brain: no lock stands.");
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
