# brain

The brain service, `bmo-brain`. Each chat agent has one brain: its memory rows (promises, checks, states,
situation rules, notes and watches), its brain documents (knowledge, the person book and room rules) and
the fragment index over them. Recollector runs read a room's conversation and write those documents;
the service decides what to recollect and when, and asks the agency only for the model session that does
it. The gate reads the service's material over HTTP to choose what a turn is shown; the service itself
selects nothing.

## Layout

| Path | Holds |
|---|---|
| `src/` | the service's own package: its database migrations, routes, editors, schedules and the ports to other units |
| `crates/brain-support/` | helpers the crates share: database access, the clock, text measures, policy loading, the editing lock's process start times and whole-published files |
| `crates/memory-rows/` | the memory, note and watch rows and the operations on them |
| `crates/fragment-desk/` | the fragment desk over an agent's brain documents |
| `tests/` | the service's tests and the checks that span the repository |
| `testbed.toml` | the build and test commands, the service unit and the version |

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Running

The service runs as the `bmo-brain` unit `testbed.toml` names, with `bmo-brain --config <file> serve`: it runs
its migrations and answers on its listen address. The same binary given a run's words does one piece of work the
brain launched apart and ends. Its configuration, a TOML file, is stated key by key in `docs/boundary-api.md`. It
reads:

- the address it listens on and the bearer token callers present;
- its database, which holds only the brain's own tables and its own settings;
- its brain folder, a git repository of the agents' documents, and its prompts folder;
- the chat agent's key, which names whose brain it serves, and the owner's number and names;
- the addresses and tokens of the agency, the receiver and the gate;
- its own command, the words that start its binary, which every run it launches apart through the agency begins
  with.

Its policy files under `policy/` are built into the binary.
