# Working in this repository

This repository is the brain service, `bmo-brain`: an agent's long-term memory as a service. It keeps many
brains, each an account's and one agent's, as subjects, layers and versioned memories its agent writes as it
reads, hands a turn its recall block, renders a persona, and moves a brain in and out as a brain file;
`docs/design.md` states the shape. It serves them over HTTP and publishes its own events. It never executes
and calls no other unit. It publishes its client,
`bmo-brain-client`, and its harness plugin, `brain-harness`: the MCP server its binary starts as
`bmo-brain mcp` and the skill under `skills/`. It is an upstream: bmo-chat's `docs/conventions.md` holds the
rules it keeps.

## Rules

- **`docs/` is canonical.** Read the document covering a change before changing code. A change that would
  make it wrong or incomplete repairs it first, stating what is true now, and the code is then written to
  match. `docs/design.md` states the shape and the rules of the brain; `docs/boundary-api.md` states every
  route, event, tool and code; `docs/openapi.json` is generated from the code (`bmo-brain openapi`) and a
  test fails where they differ. - **It knows nothing of its callers.** A caller's meaning stays in the keys
  it gives, an account's, an agent's, a subject's, and in `labels` and `<role>Name` fields, which the brain
  never reads, and no route, field, tool, schema, document or test names a particular agent, owner, host or
  caller, or anything only one caller uses.
- **No `bmo-*` dependency but the base libraries** `bmo-chat`, `bmo-chat-client` and `bmo-ui`, and this
  repository's own crates. `tests/repository.rs` fails on any other.
- **English only** in code, comments, strings, tests and documents, and in commit messages. What a machine
  reads is a status code and a kebab-case code; the sentence beside it is English. The README alone also comes
  translated, as `README.ko.md`, `README.ja.md` and `README.zh-CN.md` beside it, each kept to the English one.
- **No personal data** in code, comments, documents, test fixtures or commits: no agent, owner, host or
  company name, mail address, telephone number, home path, token or private network address. Tests use
  neutral names such as `writer-a` and `room:a`. `docs/PERSONAL-DATA.md` states the rule, and
  `tests/repository.rs` names the file, line and kind of each value it finds.
- **Tests live in `tests/`.** A crate's tests are in its own `tests/`; checks that span the repository and the
  service's own tests are in the root `tests/`. No test module sits in `src/`.
- **No directory named after its parent** (no `x/x`), and **only Cargo manifests at the root**: the root
  `Cargo.toml` is the workspace and also the service's package.
- **Documents state what is true now**: no change history, no quoted source and no file-and-line pins.

## Checks

A change moves through three stages in order:

1. Writing: `cargo check --workspace --all-targets`.
2. Review: read the whole diff against what the change is for and the document governing it, then
   `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` (108 columns,
   2-space indentation, as `rustfmt.toml` sets).
3. Verification: the test targets the change touched, then `cargo test --workspace` once, the Cargo suite
   `testbed.toml` declares.

The version is the workspace's: until the first release it is `0.1.0-rev.N`, and each change raises only the
revision. The testbed's declaration, `testbed.toml`, carries none.
