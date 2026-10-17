//! Checks that span the repository: what its packages may depend on, its layout, the testbed's declaration
//! held to every tool's manifest, and what no tracked file may hold (Hangul in Rust source, agent names, home
//! paths and private network addresses).

use std::fs;
use std::io::ErrorKind;
use std::net::Ipv4Addr;
use std::path::{Component, Path};
use std::process::Command;

use serde_json::Value;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The only `bmo-*` packages a package here may depend on: the base libraries.
const BASE_LIBRARIES: [&str; 3] = ["bmo-chat", "bmo-chat-client", "bmo-ui"];

/// Shared crates of the single repository this one was separated from; their parts live in-tree here.
const OLD_SHARED_CRATES: [&str; 11] = [
  "archive-pool",
  "archive-read",
  "bmo-lang",
  "bmo-markup",
  "bmo-policy",
  "bmo-tool",
  "room-acts",
  "seoul-clock",
  "harness-providers",
  "agent-host",
  "test-support",
];

/// File names that declare a package or a project to some other build tool.
const OTHER_MANIFESTS: [&str; 10] = [
  "package.json",
  "pyproject.toml",
  "setup.py",
  "go.mod",
  "deno.json",
  "bun.lockb",
  "pnpm-workspace.yaml",
  "Gemfile",
  "pom.xml",
  "build.gradle",
];

/// The fixture that lists the agent names; it is the one file allowed to hold them.
const NAMES_FIXTURE: &str = "tests/fixtures/repository/names.toml";

/// The Hangul blocks of Unicode: Jamo, compatibility Jamo, Jamo extended A and B, syllables, and the
/// halfwidth forms.
const HANGUL: [(u32, u32); 6] = [
  (0x1100, 0x11FF),
  (0x3130, 0x318F),
  (0xA960, 0xA97F),
  (0xAC00, 0xD7AF),
  (0xD7B0, 0xD7FF),
  (0xFFA0, 0xFFDC),
];

/// Path prefixes a home directory is written under.
const HOME_PREFIXES: [&str; 2] = ["/home/", "/Users/"];

fn metadata() -> Value {
  let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
  let output = Command::new(cargo)
    .args(["metadata", "--no-deps", "--format-version", "1"])
    .current_dir(ROOT)
    .output()
    .expect("cargo metadata runs");
  assert!(
    output.status.success(),
    "cargo metadata failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  serde_json::from_slice(&output.stdout).expect("cargo metadata answers JSON")
}

/// Every file git tracks or would track: the committed ones and the new ones it does not ignore.
fn tracked_files() -> Vec<String> {
  let output = Command::new("git")
    .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
    .current_dir(ROOT)
    .output()
    .expect("git ls-files runs");
  assert!(
    output.status.success(),
    "git ls-files failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let mut files: Vec<String> = String::from_utf8(output.stdout)
    .expect("tracked paths are UTF-8")
    .split('\0')
    .filter(|path| !path.is_empty())
    .map(str::to_owned)
    .collect();
  files.sort();
  files.dedup();
  files
}

/// The text of a tracked file, or nothing when it is gone from the work tree or is not UTF-8 text.
fn text_of(path: &str) -> Option<String> {
  match fs::read(Path::new(ROOT).join(path)) {
    Ok(bytes) => String::from_utf8(bytes).ok(),
    Err(error) if error.kind() == ErrorKind::NotFound => None,
    Err(error) => panic!("{path} is unreadable: {error}"),
  }
}

fn tracked_texts() -> Vec<(String, String)> {
  tracked_files()
    .into_iter()
    .filter_map(|path| text_of(&path).map(|text| (path, text)))
    .collect()
}

fn is_hangul(character: char) -> bool {
  let point = u32::from(character);
  HANGUL.iter().any(|&(first, last)| (first..=last).contains(&point))
}

fn is_word_character(character: char) -> bool {
  character.is_alphanumeric() || character == '_'
}

/// Whether `word` stands in `text` as a whole word, not as part of a longer one.
fn holds_word(text: &str, word: &str) -> bool {
  text.match_indices(word).any(|(start, found)| {
    let before = text[..start].chars().next_back();
    let after = text[start + found.len()..].chars().next();
    !before.is_some_and(is_word_character) && !after.is_some_and(is_word_character)
  })
}

fn names_list(names: &toml::Table, key: &str) -> Vec<String> {
  names
    .get(key)
    .and_then(toml::Value::as_array)
    .unwrap_or_else(|| panic!("{NAMES_FIXTURE} lists `{key}`"))
    .iter()
    .map(|name| name.as_str().expect("a listed name is a string").to_owned())
    .collect()
}

/// An address in a range only a private network uses: the private ranges, and the shared range
/// carrier-grade and overlay networks hand out.
fn is_private_network(address: Ipv4Addr) -> bool {
  let [first, second, ..] = address.octets();
  address.is_private() || (first == 100 && second & 0b1100_0000 == 0b0100_0000)
}

fn private_addresses(text: &str) -> Vec<Ipv4Addr> {
  text
    .split(|character: char| !character.is_ascii_digit() && character != '.')
    .filter_map(|run| run.trim_matches('.').parse::<Ipv4Addr>().ok())
    .filter(|&address| is_private_network(address))
    .collect()
}

fn holds_home_path(text: &str) -> bool {
  HOME_PREFIXES.iter().any(|prefix| {
    text.match_indices(prefix).any(|(start, found)| {
      text[start + found.len()..]
        .chars()
        .next()
        .is_some_and(|next| is_word_character(next) || next == '.')
    })
  })
}

#[test]
fn depends_on_no_bmo_package_but_the_base_libraries() {
  let metadata = metadata();
  let mut refused = Vec::new();
  for package in metadata["packages"].as_array().expect("metadata lists packages") {
    let owner = package["name"].as_str().expect("a package has a name");
    for dependency in package["dependencies"].as_array().expect("a package lists its dependencies") {
      let name = dependency["name"].as_str().expect("a dependency has a name");
      let foreign_bmo = name.starts_with("bmo-") && !BASE_LIBRARIES.contains(&name);
      if foreign_bmo || OLD_SHARED_CRATES.contains(&name) {
        refused.push(format!("{owner} -> {name}"));
      }
    }
  }
  assert!(refused.is_empty(), "dependencies outside the base libraries: {refused:?}");
}

#[test]
fn no_package_takes_an_old_shared_crate_name() {
  let metadata = metadata();
  let taken: Vec<&str> = metadata["packages"]
    .as_array()
    .expect("metadata lists packages")
    .iter()
    .map(|package| package["name"].as_str().expect("a package has a name"))
    .filter(|name| OLD_SHARED_CRATES.contains(name))
    .collect();
  assert!(taken.is_empty(), "packages named after old shared crates: {taken:?}");
}

fn declaration() -> toml::Table {
  let text = text_of("testbed.toml").expect("testbed.toml is at the root");
  text.parse().expect("testbed.toml is TOML")
}

fn table<'a>(within: &'a toml::Table, key: &str) -> &'a toml::Table {
  within
    .get(key)
    .and_then(toml::Value::as_table)
    .unwrap_or_else(|| panic!("testbed.toml has `[{key}]`"))
}

fn text<'a>(within: &'a toml::Table, key: &str) -> &'a str {
  within
    .get(key)
    .and_then(toml::Value::as_str)
    .unwrap_or_else(|| panic!("`{key}` is a string"))
}

fn deploy_step<'a>(declaration: &'a toml::Table, id: &str) -> &'a toml::Table {
  table(declaration, "deploy")
    .get("step")
    .and_then(toml::Value::as_array)
    .expect("testbed.toml declares deploy steps")
    .iter()
    .filter_map(toml::Value::as_table)
    .find(|step| step.get("id").and_then(toml::Value::as_str) == Some(id))
    .unwrap_or_else(|| panic!("testbed.toml declares the deploy step `{id}`"))
}

/// The workspace's binary targets.
fn binary_targets() -> Vec<String> {
  metadata()["packages"]
    .as_array()
    .expect("metadata lists packages")
    .iter()
    .flat_map(|package| package["targets"].as_array().expect("a package lists its targets"))
    .filter(|target| target["kind"].as_array().is_some_and(|kinds| kinds.contains(&Value::from("bin"))))
    .map(|target| target["name"].as_str().expect("a target has a name").to_owned())
    .collect()
}

#[test]
fn the_testbed_declaration_is_in_the_testbeds_format_and_carries_no_version() {
  let declaration = declaration();
  let tables = ["test", "projects", "tools", "check", "build", "deploy", "runner", "events"];
  let strange: Vec<&String> = declaration
    .keys()
    .filter(|key| !["testbed", "name"].contains(&key.as_str()) && !tables.contains(&key.as_str()))
    .collect();

  assert_eq!(declaration.get("testbed").and_then(toml::Value::as_integer), Some(0));
  assert!(strange.is_empty(), "testbed.toml holds keys its format does not name: {strange:?}");
  for key in tables.iter().filter(|key| declaration.contains_key(**key)) {
    table(&declaration, key);
  }
  assert_eq!(
    table(&declaration, "test").get("languages").and_then(toml::Value::as_array),
    Some(&vec![toml::Value::from("cargo")])
  );
}

#[test]
fn the_testbed_declaration_builds_and_places_every_tool() {
  let declaration = declaration();
  let built: Vec<&str> = table(&declaration, "build")
    .get("binaries")
    .and_then(toml::Value::as_array)
    .expect("testbed.toml names the release's binaries")
    .iter()
    .map(|binary| binary.as_str().expect("a binary is named by a string"))
    .collect();
  let targets = binary_targets();
  let mut tools = Vec::new();

  assert_eq!(text(table(&declaration, "tools"), "dir"), "tools");
  for binary in &built {
    assert!(
      targets.iter().any(|target| target == binary),
      "`{binary}` is no binary of the workspace"
    );
  }
  assert!(built.contains(&"bmo-brain"), "the release holds the service's binary");

  for manifest_path in tracked_files().into_iter().filter(|path| {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() == 3 && parts[0] == "tools" && parts[2] == "tool.toml"
  }) {
    let folder = Path::new(&manifest_path).parent().expect("a manifest stands in its folder");
    let name = folder.file_name().expect("a tool folder has a name").to_string_lossy().into_owned();
    let manifest: toml::Table = text_of(&manifest_path)
      .expect("a tool's manifest is text")
      .parse()
      .expect("a tool's manifest is TOML");
    let executable = table(&manifest, "tool")
      .get("run")
      .and_then(toml::Value::as_array)
      .and_then(|run| run.first())
      .and_then(toml::Value::as_str)
      .unwrap_or_else(|| panic!("{manifest_path} names the executable it runs"));
    let mut schemas: Vec<&str> = Vec::new();

    for entry in manifest.get("entry").and_then(toml::Value::as_array).into_iter().flatten() {
      for key in ["input", "output"] {
        if let Some(schema) = entry.get(key).and_then(toml::Value::as_str)
          && !schemas.contains(&schema)
        {
          schemas.push(schema);
        }
      }
    }

    for schema in &schemas {
      assert!(
        Path::new(ROOT).join(folder).join(schema).is_file(),
        "{manifest_path} names {schema}, which is not beside it"
      );
    }
    assert!(
      built.contains(&executable),
      "the release's binaries hold `{executable}` for tools/{name}"
    );

    let placed = format!("tools/{name}/.placed");
    let copied: Vec<String> = [format!("bin/{executable}"), format!("tools/{name}/tool.toml")]
      .into_iter()
      .chain(schemas.iter().map(|schema| format!("tools/{name}/{schema}")))
      .collect();
    let command =
      format!("set -eu\nrm -rf {placed}\nmkdir -p {placed}\ncp {} {placed}/\n", copied.join(" "));
    let step = deploy_step(&declaration, &format!("place-{name}"));

    assert_eq!(text(step, "phase"), "before-flip", "tools/{name} is placed before the flip");
    assert_eq!(text(step, "command"), command, "tools/{name} is placed as its manifest says");
    tools.push(name);
  }

  let placing: Vec<&str> = table(&declaration, "deploy")
    .get("step")
    .and_then(toml::Value::as_array)
    .expect("testbed.toml declares deploy steps")
    .iter()
    .filter_map(|step| step.get("id").and_then(toml::Value::as_str))
    .filter_map(|id| id.strip_prefix("place-"))
    .collect();

  assert!(!tools.is_empty(), "no tool manifest was found under tools/");
  assert!(
    placing.iter().all(|placed| tools.iter().any(|tool| tool == placed)),
    "every placing step names a tool folder: {placing:?}"
  );
}

#[test]
fn the_tools_env_file_is_written_before_the_flip_from_the_linked_configuration() {
  let declaration = declaration();
  let deploy = table(&declaration, "deploy");
  let linked = deploy
    .get("env_links")
    .and_then(toml::Value::as_array)
    .expect("testbed.toml links the configuration into each release");
  let step = deploy_step(&declaration, "tool-env");

  assert!(
    linked
      .iter()
      .filter_map(toml::Value::as_table)
      .any(|link| { text(link, "link") == "brain.toml" && text(link, "target") == "env/brain.toml" })
  );
  assert_eq!(text(step, "phase"), "before-flip");
  assert_eq!(text(step, "command"), "bin/bmo-brain --config brain.toml tool-env");
}

#[test]
fn only_cargo_manifests_at_the_root() {
  let found: Vec<String> = fs::read_dir(ROOT)
    .expect("the repository root is readable")
    .map(|entry| entry.expect("a root entry is readable").file_name().to_string_lossy().into_owned())
    .filter(|name| OTHER_MANIFESTS.contains(&name.as_str()))
    .collect();
  assert!(found.is_empty(), "the root holds {found:?}; only Cargo manifests belong there");
}

#[test]
fn no_directory_is_named_after_its_parent() {
  let twins: Vec<String> = tracked_files()
    .into_iter()
    .filter(|path| {
      let parts: Vec<Component> = Path::new(path).components().collect();
      parts.windows(2).take(parts.len().saturating_sub(2)).any(|pair| pair[0] == pair[1])
    })
    .collect();
  assert!(twins.is_empty(), "these paths repeat a directory inside itself: {twins:?}");
}

#[test]
fn no_hangul_in_rust_source() {
  let found: Vec<String> = tracked_texts()
    .into_iter()
    .filter(|(path, _)| {
      path.ends_with(".rs") && ["src/", "crates/", "tests/"].iter().any(|top| path.starts_with(top))
    })
    .filter(|(_, text)| text.chars().any(is_hangul))
    .map(|(path, _)| path)
    .collect();
  assert!(found.is_empty(), "Rust files holding Hangul: {found:?}");
}

#[test]
fn no_agent_names_in_tracked_files() {
  let names: toml::Table = text_of(NAMES_FIXTURE).expect("the names fixture exists").parse().expect("TOML");
  let words: Vec<String> = names_list(&names, "words").iter().map(|name| name.to_lowercase()).collect();
  let exact_words = names_list(&names, "exact_words");
  let anywhere = names_list(&names, "anywhere");
  let mut found = Vec::new();
  for (path, text) in tracked_texts().into_iter().filter(|(path, _)| path != NAMES_FIXTURE) {
    let lowered = text.to_lowercase();
    let held = words
      .iter()
      .filter(|name| holds_word(&lowered, name))
      .chain(exact_words.iter().filter(|name| holds_word(&text, name)))
      .chain(anywhere.iter().filter(|name| text.contains(name.as_str())));
    found.extend(held.map(|name| format!("{path}: {name}")));
  }
  assert!(found.is_empty(), "agent names in tracked files: {found:?}");
}

#[test]
fn no_home_paths_in_tracked_files() {
  let found: Vec<String> = tracked_texts()
    .into_iter()
    .filter(|(_, text)| holds_home_path(text))
    .map(|(path, _)| path)
    .collect();
  assert!(found.is_empty(), "home paths in tracked files: {found:?}");
}

#[test]
fn no_private_network_addresses_in_tracked_files() {
  let mut found = Vec::new();
  for (path, text) in tracked_texts() {
    found.extend(private_addresses(&text).into_iter().map(|address| format!("{path}: {address}")));
  }
  assert!(found.is_empty(), "private network addresses in tracked files: {found:?}");
}
