**English** | [한국어](README.ko.md) | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

# brain

The brain service, `bmo-brain`: an agent's long-term memory as a service. It keeps many brains, each an
account's and one agent's, as subjects (what a memory is about, found by type and key), layers (the table of
contents) and memories, one row a version, superseded and invalidated but never deleted. A turn asks a
brain's recall for a block under a token budget, a persona is rendered as the same bytes until a piece
changes, and a brain moves in and out as a brain file. The brain never executes and calls no other unit, and
holds nothing outside a brain. Its harness plugin, `brain-harness`, is the MCP server the same binary starts
as `bmo-brain mcp` and the skill that tells a harness how to use it.

`docs/design.md` states the shape and the rules the brain keeps; `docs/boundary-api.md` states every route,
event, tool and code; `docs/openapi.json` is the same contract in machine form, generated from the client's
types.

## Personal data

No personal data stands in this repository: no person's name, handle, account id, mail address, telephone
number, home path, host name, domain, token or private network address, in code, comments, documents, test
fixtures or commit messages. `docs/PERSONAL-DATA.md` states the rule and the check that holds it.

## Layout

| Folder | Holds |
|---|---|
| `src/` | the service's own package: its store (the brains, their subjects, layers, memories, recall, sheets and brain files), routes, clock, MCP server, OpenAPI document and the sheet split |
| `crates/` | `client/`, the crate `bmo-brain-client`, every body and member as a Rust type and one method for each route |
| `package/` | the TypeScript package `bmo-brain-client`, generated from `docs/openapi.json`: every body as a type and a typed call for each route |
| `client/` | the brain's screen, a Nuxt layer on bmo-ui built into the console bundle the brain serves: its pages, components, words, build and tests |
| `migrations/` | the database's migrations, built into the binary |
| `skills/` | `brain-harness/`, the plugin's skill |
| `docs/` | the design, the boundary document, the OpenAPI document, the tests' databases and the personal-data rule |
| `tests/` | the service's tests and the checks that span the repository |
| `deploy/` | the service's user unit, `bmo-brain.service`, which a deployment links from the deploy tree |
| `tools/` | the tools for the tool host: the brain's, `brain/`, and the memory tool, `memory/`, each with its manifest, its entries' schemas and the crate of its executable |

The testbed's declaration, `testbed.toml`, says how the testbed tests, checks, builds and deploys the repository.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
bun install --cwd package --frozen-lockfile
bun run --cwd package check
bun install --cwd client --frozen-lockfile
bun run --cwd client check
```

The tests make their own databases on the PostgreSQL server `BMO_TEST_POSTGRES` names, the local socket's where
it names none. The console bundle's tests under `client/tests/browser/` drive the screen in a real Chromium through
playwright-core, the screen stood alone on the Nuxt host under `client/tests/browser/host/` and the brain's routes
answered by a stand-in in the test; they are skipped where the browser playwright-core pins is not on the machine.

## Clients

The service publishes its client in two languages, both from `docs/openapi.json`: the Rust crate
`bmo-brain-client` under `crates/client`, and the TypeScript package `bmo-brain-client` under `package/`.

- A downstream checkout installs the TypeScript package from its pack: `bun pm pack` in `package/` makes
  `bmo-brain-client-<version>.tgz`, which the checkout adds by path. The package's version is the service's.
- `brainClient(<address>, <token>)` answers a client whose calls are the document's routes, typed by it:
  `brain.GET("/brains/{brainId}/memories/{memoryId}", { params: { path: { brainId, memoryId } } })`. Every call carries the token,
  and `GET /events` answers a page; a caller reading the live stream reads server-sent events itself.
- `bun run --cwd package generate` writes the package's types from the document, and its check fails where
  the committed types differ.

## Running

- `bmo-brain --config <file> serve` runs the migrations and answers on the listen address. The configuration is
  a TOML file whose required keys are `listen`, `token` and `database`; `data_root`, `[rules]`, `[embedding]` and
  `[console]` are optional (`docs/boundary-api.md`, 「Configuration」).
- `bmo-brain --config <file> health` ends 0 only where that service answers ready.
- `bmo-brain mcp --brain <url>` serves the harness tools over MCP on standard input and output, with the token
  in `BRAIN_TOKEN`.
- `bmo-brain openapi` prints the OpenAPI document; `docs/openapi.json` is its output.
- `bmo-brain --config <file> embed` embeds every live row lacking a vector of the configured model.
- `bmo-brain --config <file> tool-env` writes the env file the brain's tool reads the service's address and token
  from, under the data root.
- `bmo-brain --config <file> sheets split` splits the live sheet rows into facts; `--dry-run` writes nothing.
