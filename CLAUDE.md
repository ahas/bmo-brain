# Working in this repository

This repository is the brain service, `bmo-brain`: each chat agent's memory rows (promises, checks,
states, situation rules, notes and watches), its brain documents (knowledge, the person book and room
rules), the fragment index over them, the word-chain game, and the recollector runs that write the
documents. It owns its own database tables and its brain folder. It asks the agency for recollector model
sessions and nothing else, reads messages and rooms from the receiver, and serves raw material to the
gate, which does all reference selection. Other units reach it over HTTP only.

## Rules

- **`docs/` is canonical.** Read the document covering a change before changing code. A change that would
  make it wrong or incomplete repairs it first, stating what is true now, and the code is then written to
  match. `docs/boundary-api.md` states every route this service serves, every call it makes on another
  unit and every status code it answers with.
- **No `bmo-*` dependency but the base libraries** `bmo-chat`, `bmo-chat-client` and `bmo-ui`. Another
  unit is reached through a port in this repository, over HTTP; a helper the old shared crates held is
  copied into a crate here. `tests/repository.rs` fails on any other `bmo-*` dependency and on the old
  shared crate names.
- **A base never knows its consumers.** A port, a crate or a route supplies the mechanism and a safe
  default; no code here names the unit or agent that calls it.
- **English only** in code, comments, strings, tests and documents, and in commit messages. Messages the
  service answers with are English sentences; what a machine reads is a status code or a kebab-case code
  with parameters, and the web unit's language packs render it for people. Korean game vocabulary and
  dictionary data are data, kept in `policy/*.toml` files and under `tests/fixtures/`, never in `.rs`
  files.
- **No agent names, owner names, host names, home paths or private network addresses** in code, documents
  or commits. An agent is named by its registration and the service's configuration; tests use neutral
  names such as `agent-a` and `agent-b`. `tests/repository.rs` fails on these.
- **Tests live in `tests/`.** A crate's tests are in its own `tests/`; checks that span the repository and
  the service's own tests are in the root `tests/`. No test module sits in `src/`; a private function a
  test needs becomes public in its own module.
- **No directory named after its parent** (no `x/x`), and **only Cargo manifests at the root**: the root
  `Cargo.toml` is the workspace and also the service's package.
- **Documents state what is true now**: no change history, no quoted source and no file-and-line pins.

## Checks

A change moves through three stages in order:

1. Writing: `cargo check --workspace --all-targets`.
2. Review: read the whole diff against what the change is for and the document governing it, then
   `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` (108 columns,
   2-space indentation, as `rustfmt.toml` sets).
3. Verification: the test targets the change touched, then `cargo test --workspace` once, the command
   `testbed.toml` declares.

The version is the workspace's, in semver, and `testbed.toml` carries the same one.
