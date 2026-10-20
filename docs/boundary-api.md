# The brain's boundary

This document states every route the brain service serves, the events it publishes, its harness plugin and
every code it answers with. The OpenAPI document generated from the service's own types, `docs/openapi.json`,
is the same contract in machine form, and a test fails where the committed document differs from what the
code generates. The wire format and the API design rules are the common ones of bmo-chat's
`docs/conventions.md`; this document takes them from there and states only what is the brain's.

## What the brain is

Memory as middleware. The brain stores memory rows, notes, watches, reminders, fragments and the libraries
they come from, and the locks that say who is editing the store; it serves them through this API and
publishes its own events. It never executes: it opens no session, runs no model, launches nothing, calls no
other service and registers no hook. It calls nothing at all.

- **Every member is held under the caller's opaque scope.** `scope` is a string the caller writes and the
  brain never reads; where a member is about someone or something, `subjectKey` is the caller's opaque key for
  it. `<role>Name` fields (`actorName`, `setterName`, `holderName`) say who made or holds a member, and
  `labels` is the caller's own metadata, a map of strings to strings. The brain gives none of them a meaning.
- **Time is the one thing the brain acts on.** A reminder whose moment comes and a watch whose condition holds
  become events, `reminder.due` and `watch.fired`, which a client reads and acts on. What happens then is the
  client's.
- **A person sheet is a fragment** of kind `person` keyed by its `subjectKey`; the brain keeps no other shape
  for it.

## Configuration

The binary, `bmo-brain`, reads the TOML file `--config` names at start and holds it for its life.
`bmo-brain --config <file> serve` runs the migrations and answers on the listen address;
`bmo-brain --config <file> health` asks the service the file names whether it is ready and ends. The MCP
server, `bmo-brain mcp`, takes no configuration file (「The harness plugin」).

| Setting | Key | What it is |
|---|---|---|
| listen address | `listen` | where the service answers |
| token | `token` | the bearer token every caller presents |
| database | `database` | the address of the brain's own PostgreSQL database, which holds its tables and nothing else |

Deployed, the file stands under the data root as `env/brain.toml`, and each release links to it as
`brain.toml`. The service runs as the user unit `deploy/bmo-brain.service`, the repository's one service: a
deployment restarts it, waits on its health command, `bmo-brain --config <deploy tree>/brain.toml health`, and
rolls back where it does not come up ready. The health command asks the listen address (an unspecified host
asked at the loopback one) with the token, waits at most five seconds, and ends 0 where the service answers
ready and 1 otherwise, saying why on its error stream.

## Common rules

- **Auth.** Every route, `GET /health` among them, needs `Authorization: Bearer <token>`; a missing or wrong
  token is answered 401 `unauthorized`.
- **Pages.** A list answers `{"items": [...], "more": boolean}`, oldest first by the member's `id`, and is
  paged by `after` (the last `id` seen) and `limit` (50 where none is given, at most 500). The libraries and
  the fragment kinds are ordered by name instead, and `after` is the last name seen.
- **Times** are epoch milliseconds. **Ids** are signed 64-bit integers the brain assigns. A fragment and a
  library are also keyed by a name the caller gives (`key`, `libraryName`): 1 to 200 characters, none of them
  `/`, a control character or whitespace, and neither `.` nor `..`.
- **Strings.** `scope` is 1 to 200 characters; `text` of a row, a note or a reminder is not blank and at most
  64000 characters; a fragment's text at most a million. Anything outside is answered 400 `bad-request`.

## Health

`GET /health` answers `ready`, `reason` (null when ready) and `version`. The service is ready where its database
answers.

## Events

`GET /events` is the brain's one stream: server-sent events, each with the `event` field its kind and the `id`
field its number, and `data` `{"id", "kind", "at", "data"}`, the inner `data` being the member as a `GET`
answers it, or its key alone for a deletion. A reader asking `Accept: application/json` reads the same events as
a page instead.

- **Narrowing:** `kinds`, a comma-separated list of kinds; `scope`, the events of members held under one
  scope (an event of a member with no scope, such as a library or a lock, is never narrowed in).
- **Resuming:** `Last-Event-ID` or `after`. The brain keeps seven days of events; a resume from before the
  oldest it keeps is answered 410 `gone`, and the reader reads the collections again.
- **Keeping the stream open:** a comment line every fifteen seconds.

| Kind | When | `data` |
|---|---|---|
| `memory.created` | a row is added | the row |
| `memory.changed` | a row's words, state, labels or due moment change, or a scope's withering moves it | the row |
| `note.created` | a note is added | the note |
| `watch.changed` | a watch is set, claimed, ended or cancelled | the watch |
| `watch.fired` | a watch's condition holds, or a client fires it | the watch |
| `reminder.created` | a reminder is set | the reminder |
| `reminder.due` | a reminder's moment comes | the reminder |
| `reminder.deleted` | a reminder is cancelled | `{"id"}` |
| `fragment.changed` | a fragment is written, merged into, split out, switched or synced | the fragment |
| `fragment.deleted` | a fragment is removed, merged away or synced away | `{"key"}` |
| `library.changed` | a library is put | the library |
| `lock.changed` | a lock is taken, let go or runs out | the lock |
| `scope.changed` | a scope is marked forgotten or remembered, or let go | the scope's holding |

## Memory rows

A memory row is one thing to remember: `id`, `scope`, `subjectKey` (or null), `kind`, `text`, `state`,
`conditions`, `labels`, `dueAt` (or null), `matchCount`, `lastMatchedAt` (or null), `createdAt`, `changedAt`,
and `watches`, every watch whose condition names the row. `kind` is the caller's word for what the row is; the
brain reads none. `conditions` is a list of the caller's objects, kept as given.

The states, and the moves a `PATCH` may make; moving to the state a row is in already changes nothing.

| State | Means | May move to |
|---|---|---|
| `standing` | the row holds | `done`, `retired`, `candidate`, `off` |
| `done` | what it asked is finished | `standing`, `retired` |
| `retired` | it is no longer of use | `standing` |
| `candidate` | it waits for someone's approval | `standing`, `retired` |
| `off` | it is set aside, or withered | `standing`, `retired` |

| Route | Does | Answers |
|---|---|---|
| `GET /memories` | lists rows by `scope`, `scopePrefix`, `subjectKey`, `kind`, `state`, `label` (`key` for a label that is set, `key=value` for its value), `dueBefore` (a `dueAt` at or before it) and `q` (words the text holds, any case) | 200, a page |
| `POST /memories` | adds a row: `scope`, `kind`, `text`, `conditions`, `labels` required; `subjectKey`, `dueAt`, and `state` (`standing` where none is given) | 201, the row |
| `GET /memories/{memoryId}` | reads a row | 200, the row; 404 `not-found` |
| `PATCH /memories/{memoryId}` | changes `text`, `state`, `labels` (replacing them whole) or `dueAt` (null clears it) | 200, the row; 404; 409 `conflict` for a move the table does not allow |
| `POST /memory-matches` | records that a reading matched rows: `memoryIds`, `actorName`, `spentMs`, `tokenCount` | 201, the match; each row's `matchCount` grows by one and `lastMatchedAt` is set; a row that does not stand is 400 |

A match is `id`, `memoryIds`, `actorName`, `spentMs`, `tokenCount` and `createdAt`.

## Notes

A note is something to keep for a later reading: `id`, `scope`, `subjectKey` (or null), `text`, `actorName`,
`createdAt`. `GET /notes` lists them by `scope` and `subjectKey`; `POST /notes` adds one (`scope`, `text`,
`actorName` required, `subjectKey`), answered 201.

## Watches

A watch waits on a condition and fires once it holds: `id`, `scope`, `condition`, `state`, `setterName`,
`holderName` (or null), `labels`, `firedAt` (or null), `createdAt`, `changedAt`. The condition is an object
whose `kind` says what it waits on; any other field it carries is kept as given.

| `condition.kind` | Its fields | Fires |
|---|---|---|
| `at` | `at`, a moment | once the moment has come |
| `memory` | `memoryId`, a row's id; `state`, one of the row states | once that row is in that state, at once where it is already |
| `external` | `key`, the caller's string | when a client moves it to `fired` |

| State | Means | May move to |
|---|---|---|
| `armed` | waiting on its condition | `fired`, `cancelled` |
| `fired` | its condition held | `claimed`, `ended`, `cancelled` |
| `claimed` | a holder is acting on it | `ended`, `cancelled` |
| `ended` | acted on | none |
| `cancelled` | given up | none |

| Route | Does | Answers |
|---|---|---|
| `GET /watches` | lists by `scope`, `setterName`, `holderName`, `state` | 200, a page |
| `POST /watches` | sets one: `scope`, `condition`, `setterName`, `labels` | 201, the watch, `armed` (or `fired` where its condition already holds); 400 for a condition of no known kind, missing its fields, or naming no row |
| `GET /watches/{watchId}` | reads one | 200; 404 |
| `PATCH /watches/{watchId}` | moves it: `state`, and `holderName` to claim it | 200; 404; 400 for a claim with no holder; 409 for a move the table does not allow, or a claim while another holder holds it |

A claim by the holder that holds it already changes nothing. The brain fires an `at` watch on its clock, which
looks every second, and a `memory` watch as the row it names moves.

## Reminders

A reminder is a moment to be told of: `id`, `scope`, `dueAt`, `text`, `labels`, `setterName`, `createdAt`,
`firedAt` (or null). The brain publishes `reminder.due` once its moment comes, one that came while the service
was down included, and sets `firedAt`; a reminder fires once, and a caller that wants it again sets another.

| Route | Does | Answers |
|---|---|---|
| `GET /reminders` | lists by `scope`, `dueBefore` and `fired` (true or false) | 200, a page |
| `POST /reminders` | sets one: `scope`, `dueAt`, `text`, `setterName`, `labels` | 201, the reminder |
| `DELETE /reminders/{reminderId}` | cancels it | 204; 404 |

## Fragments

A fragment is one piece of reference text: `id`, `key`, `kind`, `scope` (or null for one shared by every
scope), `subjectKey` (or null), `title` (or null), `path` (or null, where the caller keeps it), `text`,
`version` (the SHA-256 of the text, in hex), `libraryName` (or null), `state` (`active` or `off`),
`switchedOff` (the scopes it is switched off for), `labels`, `hitCount`, `lastHitAt` (or null), `createdAt`
and `changedAt`. The key is the caller's and names it in every route.

| Route | Does | Answers |
|---|---|---|
| `GET /fragments` | lists by `kind`, `scope`, `subjectKey`, `libraryName`, `state`, `q` (words its title or text holds, any case), `ids` (comma-separated ids) and `keys` (comma-separated keys) | 200, a page |
| `PUT /fragments/{fragmentKey}` | writes one whole: `kind`, `text` required; `scope`, `subjectKey`, `title`, `path`, `libraryName`, `state`, `switchedOff`, `labels`; `state` and `switchedOff` left out keep what the fragment had (`active` and none for a new one) | 201 where it is new, 200 where it replaced one |
| `PATCH /fragments/{fragmentKey}` | sets `switchedOff` (whole) and `state` | 200; 404 |
| `DELETE /fragments/{fragmentKey}` | removes it | 204; 404 |
| `POST /fragment-merges` | folds the fragments `keys` into the one `into`: its text the body's `text`, or the texts in the order named joined by a blank line; `title` where given; every other named fragment removed | 201, the fragment kept; 400 for no fragment to fold or one that does not stand |
| `POST /fragment-splits` | cuts the fragment `key` into `pieces`, each `key` and `text` and any field a write takes, the rest taken from the fragment cut; the fragment cut is removed unless a piece keeps its key | 201, a page of the pieces; 400 for fewer than two pieces or a fragment that does not stand |
| `POST /fragment-hits` | records that a reading attached fragments: `keys`, `actorName`, `scope` | 201, `recordedCount`; each fragment's `hitCount` grows by one and `lastHitAt` is set; a key that names none is passed over |
| `GET /fragment-kinds` | the kinds: every kind a fragment or a library names, each `name` and `description` (empty where none was given) and `fragmentCount` | 200, a page by name |

## Libraries

A library is a named set of documents a caller keeps elsewhere and hands in whole: `name`, `documents`,
`kinds` and `changedAt`. A document is `key`, `kind`, `scope` (or null), `subjectKey` (or null), `title` (or
null), `path` (or null), `text` and `version`, the SHA-256 of its text in hex; `kinds` gives the words a
reader calls each kind by, each `name` and `description`. The library is the caller's; its fragments are the
brain's copy of it, made by a sync.

| Route | Does | Answers |
|---|---|---|
| `GET /libraries` | lists them, with their documents | 200, a page by name |
| `GET /libraries/{libraryName}` | reads one | 200; 404 |
| `PUT /libraries/{libraryName}` | puts one whole: `documents` (each without `version`, which the brain reckons), `kinds` | 201 where new, 200 where it replaced one; 400 for two documents of one key |
| `POST /library-syncs` | brings the fragments of `libraryName` in line with its documents; with `dryRun=true` only says what would change | 200, the sync; 404 for no such library |

A sync is `libraryName`, `drift` and `applied`. Each drift entry is `key` and `change`: `added` (a document
with no fragment), `changed` (a fragment whose text, version or place differs from its document), `removed` (a
fragment of the library whose document is gone) or `claimed` (a fragment of the document's key that belongs to
no library or to another, which the sync takes over). Applied, a fragment keeps its state, switches and counts;
everything else comes from its document.

## Knowledge

`GET /knowledge` answers what a reader should know for a set of scopes: `scopes` (comma-separated, required),
and to narrow it `slugs` (comma-separated fragment keys), `kinds` (comma-separated) and `subjectKey`. It
answers `scopes`, the scopes asked; `fragments`, the active fragments held under one of them or shared, switched
off for none of them, in the order `slugs` names them or else by id; and `memories`, the standing rows held
under one of them, by id. `subjectKey` narrows both.

## Locks

A lock says who is editing the store. It holds nothing back: a writer that wants the store to itself takes a
lock first and lets it go after, and every writer that keeps to that waits on the others. A lock is `id`,
`holderName`, `reason`, `createdAt` and `expiresAt` (or null for one that stands until it is let go).

| Route | Does | Answers |
|---|---|---|
| `GET /locks` | the locks that stand; with `waitMs` (at most ten minutes) it answers once none stands or the wait runs out | 200, a page |
| `POST /locks` | takes the lock: `holderName`, `reason`, `ttlMs` | 201, the lock; 409 while another holder holds one; the holder holding it already has it renewed, answered 200 |
| `DELETE /locks/{lockId}` | lets it go | 204; 404 |

A lock past its `expiresAt` is let go on the brain's clock, with `lock.changed`.

## Scopes

What the brain holds under a scope a caller gave it is the scope's holding: `scope`, `counts` (each of
`memories`, `memoryMatches`, `notes`, `watches`, `reminders`, `fragments`, `fragmentHits` with its count) and
`state` (`remembered` or `forgotten`).

| Route | Does | Answers |
|---|---|---|
| `GET /scopes/{scope}` | reads the holding | 200; 404 where nothing is held under it |
| `PATCH /scopes/{scope}` | `state`: `forgotten`, so its rows wither, or `remembered` again | 200; 404 |
| `DELETE /scopes/{scope}` | lets go of everything held under it; with `dryRun=true` only answers what would go | 200, the holding as it stood; 404 |

**Withering.** While a scope is forgotten, the brain's clock moves each of its standing rows that no reading has
matched for thirty days, counted from its last match or else its creation, to `off`, with `memory.changed`.
Remembered again, nothing withers; a withered row stays `off` until a client moves it.

## The command

`bmo-brain` is the service and its command in one binary. The platform's `bmo` asks it `nouns` and hands a
command whose leading words match a declared noun to it, the noun's words included.

- **`bmo-brain nouns`** prints the nouns it carries, one a line: `memory`, `note`, `fragment`, `watch set`,
  `watch list`, `reminder list`, `brain hold` and `brain wait`. No noun carries a mark: every command asks
  the service's routes over HTTP, so it runs wherever the service is reachable.
- **The service asked** is the one the configuration names: the file `--config` gives, else the one
  `BMO_BRAIN_CONFIG` names, else the `brain.toml` a deployment links in the directory above the binary's.
  The command reads the configuration's `listen` (an unspecified host asked at the loopback one) and `token`,
  and calls the routes through the brain's client.
- **Moments** are given as epoch milliseconds or an RFC 3339 time, and printed as RFC 3339 in UTC to the
  millisecond. **Labels** are given as `key=value`, one `--label` each. **A condition** is a JSON object.
- **`--via <road>`**, which `bmo` hands on after the noun, is taken and passed over: the brain reads no turn.
- **Output** begins each member on a line, prefixed with the noun, and prints its text as it stands. **Ending:**
  0 where the command finished; 1 where the brain refused or a value did not read, with `bmo-brain: <the
  refusal>` on standard error; 2 where the words are no form the command takes.

| Noun | Form | Route |
|---|---|---|
| `memory` | `add --scope <scope> --kind <kind> --text <text>` | `POST /memories` |
| | `update <id>` | `PATCH /memories/{memoryId}` |
| | `done <id> [--text <text>]` | `PATCH /memories/{memoryId}` |
| | `retire <id>`, `revive <id>`, `candidate <id>` | `PATCH /memories/{memoryId}` |
| | `list` | `GET /memories` |
| `note` | `--scope <scope> --actor <name> [--text <text>]` | `POST /notes` |
| `fragment` | `list`, `write <key>`, `remove <key>`, `switch <key>`, `sync <library>` | the fragment and library routes above |
| `watch set` | `--scope <scope> --setter <name>` and one condition | `POST /watches` |
| `watch list` | | `GET /watches` |
| `reminder list` | `[--scope <scope>]`, or `--cancel <id>` | `GET /reminders`, `DELETE /reminders/{reminderId}` |
| `brain hold` | `--holder <name> --reason <text> [--ttl-ms <n>]` | `POST /locks` |
| `brain wait` | `[--wait-ms <n>]` | `GET /locks` with `waitMs` |

- **`memory add`** takes `--scope`, `--kind` (also as `--type`) and `--text` required, then `--subject`,
  `--condition` (one flag each, an object kept as given), `--label`, `--due` and `--state`; `--text -` reads
  standard input. **`memory update`** takes at least one of `--text`, `--state`, `--label` (replacing the
  labels whole), `--due` or `--no-due`. **`memory done`** moves a row to `done`, with the text it ends on
  where `--text` gives one. **`memory list`** narrows by `--scope`, `--scope-prefix`, `--subject`, `--kind`,
  `--state`, `--label` (`key` or `key=value`), `--due-before`, `--q`, `--after` and `--limit`.
- **`note`** takes `--scope` and `--actor` required, then `--subject` and `--text`; with no `--text` it reads
  standard input.
- **`fragment list`** narrows by `--kind`, `--scope`, `--subject`, `--library`, `--state`, `--q`, `--ids`,
  `--keys`, `--after` and `--limit`. **`fragment write <key>`** takes `--kind` and `--text` required, then
  `--scope`, `--subject`, `--title`, `--path`, `--library`, `--state` and `--label`. **`fragment switch
  <key>`** takes `--state` or `--off-for` (one flag each, replacing the scopes it was off for). **`fragment
  sync <library>`** takes `--dry-run`.
- **`watch set`** takes exactly one condition: `--at` a moment, `--memory` a row with the `--state` it waits
  for, or `--external` a key. **`watch list`** narrows by `--scope`, `--setter`, `--holder`, `--state`,
  `--after` and `--limit`.
- **`reminder list`** narrows by `--scope`, `--due-before`, `--fired` or `--waiting`, `--after` and
  `--limit`; `--cancel <id>` cancels the reminder it names instead of listing.
- **`brain hold`** takes `--holder` and `--reason` required, and `--ttl-ms` for how long it stands; with none
  it stands until it is let go. **`brain wait`** waits `--wait-ms` (ten minutes where none is given) for no
  lock to stand, and narrows with `--after` and `--limit`.

## The harness plugin

The plugin is `brain-harness`: the MCP server `bmo-brain mcp` starts on standard input and output, and the skill
`skills/brain-harness/SKILL.md`, which tells a harness when and how to use it. The server reaches the service at
`--brain <url>` (or `BRAIN_URL`) with the token `--token` (or `BRAIN_TOKEN`). Each tool is one call of one
route through the brain's client, `bmo-brain-client`; a failure comes back as a tool error carrying the route's
status, code and sentence. A harness reads what it should remember for a scope with `brain_knowledge` and
`brain_memories`, writes rows and notes as it learns, and sets reminders and watches whose events its own loop
reads on `GET /events`. The server sends no notifications.

| Tool | Route |
|---|---|
| `brain_memories` | `GET /memories` |
| `brain_memory_add` | `POST /memories` |
| `brain_memory` | `GET /memories/{memoryId}` |
| `brain_memory_update` | `PATCH /memories/{memoryId}` |
| `brain_notes` | `GET /notes` |
| `brain_note_add` | `POST /notes` |
| `brain_watches` | `GET /watches` |
| `brain_watch_add` | `POST /watches` |
| `brain_watch_update` | `PATCH /watches/{watchId}` |
| `brain_reminders` | `GET /reminders` |
| `brain_reminder_add` | `POST /reminders` |
| `brain_reminder_cancel` | `DELETE /reminders/{reminderId}` |
| `brain_fragments` | `GET /fragments` |
| `brain_fragment_put` | `PUT /fragments/{fragmentKey}` |
| `brain_knowledge` | `GET /knowledge` |
| `brain_lock` | `POST /locks` |
| `brain_unlock` | `DELETE /locks/{lockId}` |

## The client

The service publishes its client in two languages, both from `docs/openapi.json`:

- **Rust:** `bmo-brain-client`, under `crates/client`, carries every body and member above as a Rust type
  and one method for each route. The document is generated from those types, so the crate and the
  document cannot differ.
- **TypeScript:** the package `bmo-brain-client`, under `package/`, carries the document's bodies as the
  types `openapi-typescript` generates and a typed call for each route over `openapi-fetch`. Every call
  carries the bearer token, and the call asks for JSON, so `GET /events` answers a page; a caller that
  reads the live stream reads server-sent events from the same route itself. `bun run generate` writes the
  types from the document, and the package's check fails where they differ.

A downstream checkout installs the TypeScript package from its pack: `bun pm pack` in `package/` makes
`bmo-brain-client-<version>.tgz`, which the checkout adds by path. The package and the document both carry
the service's version, and the package's tests fail where its types or its version differ from the
document.

## Codes

Every failure is the common error shape, `code` and `error`.

| Status | `code` | When |
|---|---|---|
| 400 | `bad-request` | the body, the query or a key is not the shape the route takes; the sentence names what is wrong |
| 401 | `unauthorized` | the token is missing or wrong |
| 404 | `no-route` | no route answers this method and path |
| 404 | `not-found` | nothing stands at the key the path names |
| 409 | `conflict` | the member's state does not allow the change, or another holder holds the lock |
| 410 | `gone` | an event stream resumed from before the oldest event kept |
| 500 | `failed` | the service failed; the sentence is the failure |
