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
| `crates/tool-call/` | the brain's side of the tool host's call protocol, which its tools share |
| `tools/memory/`, `tools/fragment/` | the tools bound to the brain, each with its `tool.toml`, answering through the brain's routes |
| `prompts/` | the recollectors' tasks and the two rulebooks only they read, which the `prompts` setting points at |
| `policy/` | the service's own policy, the Korean words the prompts quote among it |
| `tests/` | the service's tests and the checks that span the repository |
| `deploy/` | the service's user unit, `bmo-brain.service`, which a deployment links from the deploy tree |
| `testbed.toml` | the testbed's declaration: how it tests, checks, builds and deploys the repository, the tools' placement and their env file among it |

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Running

The service runs as `bmo-brain --config <file> serve`: it runs its migrations and answers on its listen address.
Deployed, it is the user unit `deploy/bmo-brain.service` declares, run out of the deploy tree with the release's
`brain.toml`, the repository's one service, whose health every deployment waits on. The same binary given a run's
words does one piece of work the brain launched apart and ends; given `tool-env` writes the data root's
`env/brain.env`, where the brain's tools find its address and token; and given `health` ends 0 only where the
service answers ready. Its configuration, a TOML file standing under the data root as `env/brain.toml` once
deployed, is stated key by key in `docs/boundary-api.md`. It reads:

- the address it listens on and the bearer token callers present;
- its database, which holds only the brain's own tables and its own settings;
- its brain folder, a git repository of the agents' documents, and its prompts folder, this repository's
  `prompts/` as it is deployed;
- the chat agent's key, which names whose brain it serves, and the owner's number and names;
- the addresses and tokens of the agency, the receiver, the gate and bmo-job;
- its own command, the words that start its binary, which every run it launches apart as a unit of the user's
  service manager begins with.

Its policy files under `policy/` are built into the binary.
