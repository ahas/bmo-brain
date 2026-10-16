//! Checks that span the repository: what its packages may depend on, its layout, and what no tracked file
//! may hold (Hangul in Rust source, agent names, home paths and private network addresses).

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

#[test]
fn the_testbed_declaration_carries_the_workspace_version() {
  let text = text_of("testbed.toml").expect("testbed.toml is at the root");
  let declaration: toml::Table = text.parse().expect("testbed.toml is TOML");
  assert_eq!(
    declaration.get("version").and_then(toml::Value::as_str),
    Some(env!("CARGO_PKG_VERSION"))
  );
  for key in ["build", "test", "unit"] {
    assert!(
      declaration.get(key).and_then(toml::Value::as_str).is_some(),
      "testbed.toml declares `{key}`"
    );
  }
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
