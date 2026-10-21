# brain

The brain service, `bmo-brain`: memory as middleware. It stores memory rows, notes, watches, reminders,
fragments of reference text and the libraries they come from, the locks that say who is editing the store, and
the caller's own records — a room's rules, a session's loads and a chat session's note — each under the caller's
opaque scope, and serves them over HTTP. It never executes and calls no other service;
its clock turns reminders and watches into events a client reads. Its harness plugin, `brain-harness`, is the MCP
server the same binary starts as `bmo-brain mcp` and the skill that tells a harness how to use it.

`docs/boundary-api.md` states every route, event, tool and code; `docs/openapi.json` is the same contract in
machine form, generated from the client's types.

## Personal data

No personal data stands in this repository: no person's name, handle, account id, mail address, telephone
number, home path, host name, domain, token or private network address, in code, comments, documents, test
fixtures or commit messages. `docs/PERSONAL-DATA.md` states the rule and the check that holds it.

## Layout

| Folder | Holds |
|---|---|
| `.graph/` | the code dependency graph: its settings and its nodes, one feature to a node |
| `src/` | the service's own package: its store, routes, clock, MCP server and OpenAPI document |
| `crates/` | `client/`, the crate `bmo-brain-client`, every body and member as a Rust type and one method for each route |
| `package/` | the TypeScript package `bmo-brain-client`, generated from `docs/openapi.json`: every body as a type and a typed call for each route |
| `migrations/` | the database's migrations, built into the binary |
| `skills/` | `brain-harness/`, the plugin's skill |
| `docs/` | the boundary document, the OpenAPI document and the personal-data rule |
| `tests/` | the service's tests and the checks that span the repository |
| `deploy/` | the service's user unit, `bmo-brain.service`, which a deployment links from the deploy tree |

The testbed's declaration, `testbed.toml`, says how the testbed tests, checks, builds and deploys the repository.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
bun install --cwd package --frozen-lockfile
bun run --cwd package check
```

The tests make their own databases on the PostgreSQL server `BMO_TEST_POSTGRES` names, the local socket's where
it names none.

## Clients

The service publishes its client in two languages, both from `docs/openapi.json`: the Rust crate
`bmo-brain-client` under `crates/client`, and the TypeScript package `bmo-brain-client` under `package/`.

- A downstream checkout installs the TypeScript package from its pack: `bun pm pack` in `package/` makes
  `bmo-brain-client-<version>.tgz`, which the checkout adds by path. The package's version is the service's.
- `brainClient(<address>, <token>)` answers a client whose calls are the document's routes, typed by it:
  `brain.GET("/memories/{memoryId}", { params: { path: { memoryId } } })`. Every call carries the token,
  and `GET /events` answers a page; a caller reading the live stream reads server-sent events itself.
- `bun run --cwd package generate` writes the package's types from the document, and its check fails where
  the committed types differ.

## Running

- `bmo-brain --config <file> serve` runs the migrations and answers on the listen address. The configuration is
  a TOML file of three keys: `listen`, `token` and `database`.
- `bmo-brain --config <file> health` ends 0 only where that service answers ready.
- `bmo-brain mcp --brain <url>` serves the harness tools over MCP on standard input and output, with the token
  in `BRAIN_TOKEN`.
- `bmo-brain openapi` prints the OpenAPI document; `docs/openapi.json` is its output.
