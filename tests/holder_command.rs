//! `bmo-brain holder declare <file>` (docs/boundary-api.md, 「Declarations and holders」): a holder's set read from
//! its TOML file into the body `POST /declarations` takes, the brain's reach read off the data root's env file, and
//! no declaration where no brain is reached, which passes.

use bmo_brain::holder::{HolderFile, declare, reach};
use serde_json::json;

// A folder of the test's own under the system's temporary folder, made empty.
fn folder(name: &str) -> std::path::PathBuf {
  let at = std::env::temp_dir().join(format!("brain-holder-{name}-{}", std::process::id()));

  let _ = std::fs::remove_dir_all(&at);
  std::fs::create_dir_all(&at).unwrap();
  at
}

#[test]
fn a_holder_s_file_is_the_declaration_body() {
  let held: HolderFile = toml::from_str(
    r#"
held_by = "service-a"

[[memory]]
key = "when"
type = "tip"
recall_condition = "a situation of service a"
text = "Use service a so."
importance = 7
"#,
  )
  .unwrap();

  assert_eq!(
    held.declaration(),
    json!({ "heldBy": "service-a", "memories": [
      { "key": "when", "type": "tip", "text": "Use service a so.", "recallCondition": "a situation of service a", "importance": 7 }
    ] })
  );
}

#[test]
fn the_reach_is_read_off_the_env_file_each_value_verbatim() {
  let root = folder("reach");

  assert!(reach(&root).is_none(), "no file stands");

  std::fs::create_dir_all(root.join("env")).unwrap();
  std::fs::write(root.join("env/brain.env"), "BRAIN_ADDRESS=http://127.0.0.1:9/\nBRAIN_TOKEN=a=b#c\n")
    .unwrap();

  let reached = reach(&root).unwrap();

  assert_eq!((reached.address.as_str(), reached.token.as_str()), ("http://127.0.0.1:9", "a=b#c"));
}

#[tokio::test]
async fn no_brain_reached_declares_nothing_and_passes() {
  let root = folder("none");
  let file = root.join("holder.toml");

  std::fs::write(&file, "held_by = \"service-a\"\n").unwrap();
  // SAFETY: the test sets the variable before anything reads it, and no other test of this binary reads it.
  unsafe { std::env::set_var("BMO_DATA_DIR", &root) };

  let outcome = declare(&file).await;

  assert_eq!(outcome.code, 0, "{}", outcome.line);
  assert!(outcome.line.contains("nothing declared"), "{}", outcome.line);
}
