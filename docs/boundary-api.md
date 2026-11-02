# The brain's boundary

This document states every route the brain service serves, the events it publishes, its harness plugin and
every code it answers with. The OpenAPI document generated from the service's own types,
`docs/openapi.json`, is the same contract in machine form, and a test fails where the committed document
differs from what the code generates. The wire format and the API design rules are the common ones of
bmo-chat's `docs/conventions.md`; this document takes them from there and states only what is the brain's.

## What the brain is

An agent's long-term memory as a service (`docs/design.md`). The service keeps many brains, each an
account's and one agent's: its subjects, its layers and its memories, one row a version. A turn asks a
brain's recall for a block under a token budget; a persona is rendered as the same bytes until a piece
changes; a brain moves in and out as a brain file. The brain never executes and calls no other unit.

- **A brain is keyed by `accountKey` and `agentKey`,** strings the caller gives and the brain never reads.
  Every subject, layer and memory belongs to one brain and is reached under its id.
- **Nothing is held outside a brain** (`docs/design.md`, 「What the brain holds besides brains」). A
  memory's `key` and `labels` and a subject's `type` and `key` are the caller's; the brain gives none of them
  a meaning.
- **Its clock is the one thing the brain acts on by itself:** it prunes the event window.

## Configuration

The binary, `bmo-brain`, reads the TOML file `--config` names at start and holds it for its life.
`bmo-brain --config <file> serve` runs the migrations and answers on the listen address;
`bmo-brain --config <file> health` asks the service the file names whether it is ready and ends;
`bmo-brain --config <file> sheets split` splits the live `sheet` rows of the database the file names into
facts (「Splitting sheets」). The MCP server, `bmo-brain mcp`, takes no configuration file (「The harness
plugin」). The binary declares no noun, so the platform's `bmo` hands it no command.

| Setting | Key | What it is |
|---|---|---|
| listen address | `listen` | where the service answers |
| token | `token` | the service's bearer token, which reaches every route |
| console | `[console]` | the console bundle (「The console bundle」), where the brain serves one: `token`, the console's bearer token, which reaches what the screen uses; `bundle`, the folder the build of `client/` wrote the module, its stylesheet and its words into, the release's `console` where none is given; `calls`, the other services the screen calls through the console, each by the service name its own manifest gives it, which the manifest names under `calls`, none where none is given |
| database | `database` | the address of the brain's own PostgreSQL database, which holds its tables and nothing else |
| data root | `data_root` | the data root whose `env/brain.env` the brain's tool reads the service's address and token from; the `tool-env` verb writes it |
| rules | `[rules]` | the numbers the brain reads by (`docs/design.md`), each with a default: `budget_tokens`, `weight_relevance`, `weight_importance`, `weight_recency`, `subject_bonus`, `subject_cap`, `core_fact_cap` (the most facts of one speaker's sheet the core of a recall carries, 8), `decay`, `hit_times_kept`, `same_session_ms`, `strength_gain`, `previous_value_days`, `condition_share` (the share of a condition's words a turn holds before it matches, 0.5), `consolidation_threshold` |

Deployed, the file stands under the data root as `env/brain.toml`, and each release links to it as
`brain.toml`. The service runs as the user unit `deploy/bmo-brain.service`, the repository's one service: a
deployment restarts it, waits on its health command, `bmo-brain --config <deploy tree>/brain.toml health`,
and rolls back where it does not come up ready. Before its flip it waits for quiet (`testbed.toml`, `quiet`): the
receiver's `quiet` command, run by its deployed path, says whether a chat turn or a research run stands under way
that the restart would cut (the receiver's `docs/boundary-api.md`, 「The service and its commands」). The health command asks the listen address (an unspecified
host asked at the loopback one) with the token, waits for the answer however long it takes, and ends 0 where
the service answers ready and 1 where it answers otherwise or its connection is refused or breaks, saying why on
its error stream. A stop (SIGTERM or an interrupt) ends every open event stream and lets the calls under way
finish, and the service then ends; the unit waits for that and never escalates the stop to a kill.

## Common rules

- **Auth.** Every route, `GET /health` among them, needs `Authorization: Bearer <token>`, the service's
  token or the console's; a missing or wrong token is answered 401 `unauthorized`. The service's token
  reaches every route; the console's reaches only what the screen uses (「The console bundle」), and any other
  route is answered 403 `forbidden` to it.
- **Pages.** A list answers `{"items": [...], "more": boolean}`, oldest first by the member's `id` where its
  route names no other order, and is paged by `after`, the last `id` seen, the page continuing past that
  member in the order the list stands in, and `limit` (50 where none is given, at most 500).
- **Times** are epoch milliseconds. **Ids** are signed 64-bit integers the brain assigns.
- **Strings.** `text` of a memory is not blank and at most 64000 characters, and a `recallCondition` or
  `releaseCondition` not blank and at most 1000. Anything outside is answered 400 `bad-request`.

## Health

`GET /health` answers `ready`, `reason` (null when ready), `version` and `rules`, the numbers the brain
reads by. The service is ready where its database answers.

## Events

`GET /events` is the brain's one stream: server-sent events, each with the `event` field its kind and the
`id` field its number, and `data` `{"id", "kind", "at", "data"}`, the inner `data` being the member as a
`GET` answers it, or its key alone for a deletion. A reader asking `Accept: application/json` reads the same
events as a page instead.

- **Narrowing:** `kinds`, a comma-separated list of kinds; `scope`, `brain:<id>`, the events of that brain's
  members.
- **Resuming:** `Last-Event-ID` or `after`. The brain keeps seven days of events; a resume from before the
  oldest it keeps is answered 410 `gone`, and the reader reads the collections again.
- **Keeping the stream open:** a comment line every fifteen seconds. The stream ends when the service is
  stopped, and the reader resumes from its last event once the service answers again.

| Kind | When | `data` |
|---|---|---|
| `brain.created` | a brain is made | the brain |
| `brain.deleted` | a brain is let go of | `{"id"}` |
| `brain.injected` | a brain file was read into a brain | what the injection came to |
| `subject.created` | a subject is made | the subject |
| `subject.changed` | a subject's description changes | the subject |
| `layer.changed` | a layer is added or changed | the layer |
| `memory.written` | a version of a memory is written, first or superseding | the memory |
| `memory.moved` | a memory's status moves | the memory |

## Brains

A brain is `id`, `accountKey`, `agentKey`, `name` and `createdAt`. `accountKey` and `agentKey` are strings
the caller gives, 1 to 200 characters with no control character, and one account has at most one brain per
agent.

| Route | Does | Answers |
|---|---|---|
| `GET /brains` | lists brains by `accountKey` and `agentKey` | 200, a page |
| `POST /brains` | makes one: `accountKey`, `agentKey`, `name` (the agent's key where none is given) | 201, the brain; 409 `conflict` where the account has the agent's brain already |
| `GET /brains/{brainId}` | reads one | 200; 404 |
| `DELETE /brains/{brainId}` | lets it go whole, with everything it held | 200, the brain as it stood; 404 |

Every route below stands under `/brains/{brainId}` and answers 404 `not-found` for a brain that does not
stand.

### Subjects

A subject is something a memory can be about: `id`, `brainId`, `type` (the caller's word for what it is),
`key` (the caller's identifier, unique within the brain and type), `description`, `name` and `createdAt`. A
subject is found by its type and key alone; a name is a memory of type `name` linked to it, and `name` is
the text of its newest `active` one, or null where it has none.

| Route | Does | Answers |
|---|---|---|
| `GET /subjects` | lists by `type`, `key`, `q` (words the description holds) and `ids` (comma-separated ids) | 200, a page |
| `POST /subjects` | `type`, `key`, `description`: makes the subject, or finds the one standing and sets its description where one is given | 201 where new, 200 where it stood |
| `GET /subjects/{subjectId}` | reads one | 200; 404 |
| `PATCH /subjects/{subjectId}` | sets `description` | 200; 404 |
| `GET /subjects/{subjectId}/sheet` | renders the subject's sheet; `roomId`, a subject of type `room`, takes that room's facts in as well; `limit`, at least 1, renders the first that many lines alone, the whole sheet where none is given | 200, `text/plain`; 400 for a `roomId` that is no room or a `limit` below 1; 404 |

A subject's sheet (`docs/design.md`, 「A person is a subject」) is its active `fact` memories that are about
no subject of type `room`, and, where `roomId` names a room, its active facts about that room as well. They
are ordered by importance, highest first, then by `validFrom`, latest first, then by `lastConfirmedAt`, latest
first and a fact never confirmed after the confirmed ones, then by `id`, highest first, and each is one line,
`- [#<id> since <date>] <text>`, the date the `validFrom`'s day in UTC; the lines are joined by a newline,
and a subject with no such fact renders as nothing. Nothing that changes between calls is in it, so the
same rows render as the same bytes.

### Layers

A layer is a heading in the brain's table of contents: `id`, `brainId`, `name` (unique within the brain),
`description`, `position` and `createdAt`. A brain's layers are listed by position.

| Route | Does | Answers |
|---|---|---|
| `GET /layers` | lists them | 200, a page |
| `POST /layers` | `name`, `description`, `position` (after the last where none is given): makes the layer, or finds the one standing at its name | 201 where new, 200 where it stood |
| `GET /layers/{layerId}` | reads one | 200; 404 |
| `PATCH /layers/{layerId}` | sets `description` or `position` | 200; 404 |

### Memories

One row is one memory in one version: `id`, `brainId`, `key` (the caller's identifier, or null; unique among
the live rows of a brain; 1 to 200 characters with no control character and no whitespace, whether written or
injected), `type`, `text`, `layerId` (or null), `position`, `subjects` (each `id`, `type`,
`key`), `status`, `lineageId`, `supersededBy` (or null), `validFrom`, `validTo` (or null), `recallCondition`
(or null), `releaseCondition` (or null), `importance` (1
to 10), `hits`, `lastHitAt`, `lastConfirmedAt`, `strength` (the storage strength), `retrievalStrength`
(computed from the hits as the reading answers), `consolidatedAt`, `actorName`, `reason` (why its status was
moved, as the mover said, or null), `labels` and `createdAt`.

`recallCondition` is the situation in which the memory is needed and `releaseCondition` the situation that ends
it, each the writer's words and null where it carries none (`docs/design.md`, 「Recall」). Both are set by a
first version, a version, a merge and a learning, the version's taken from the version it supersedes where it
gives none; no route changes either on a row that stands. Both are optional: a memory that carries neither is
permanent, reached by its words and its subjects as every memory was.

`type` is the writer's word; the reserved ones are `episode`, `fact`, `rumor`, `situation`, `rule`, `name`,
`persona`, `lore` and `knowledge` (`docs/design.md`, 「Types」). `sheet` is no type: a first version, a
version, a merge or a learning that names it is answered 400 `bad-request`, as is a brain file holding a live
row of it, and a restoration whose closed row is a `sheet` is answered 409 `conflict`. The statuses and the
moves a caller's word may make:

| Status | Means | May move to |
|---|---|---|
| `active` | the memory holds | `done`, `invalidated` |
| `pending` | it waits for a person's approval | `active`, `done`, `invalidated` |
| `done` | what it asked is finished | none |
| `superseded` | a later version replaced it; set by a version alone | none |
| `invalidated` | it stopped holding | none |

| Route | Does | Answers |
|---|---|---|
| `GET /memories` | lists by `type` and `status` (each one value or several, comma-separated), `excludeTypes` (comma-separated), `layerId`, `unfiled` (true for the rows in no layer), `subjectId`, `key`, `lineageId`, `ids` (comma-separated), `supersededBy` (comma-separated: the rows superseded by these), `label` (`key` or `key=value`), `q` (words the text holds) and `situation` (words the `recallCondition` holds), in the order `order` names | 200, a page |
| `POST /memories` | writes a first version: `type`, `text`, `actorName` required; `key`, `layerId`, `position`, `subjectIds`, `subjects` (each `type` and `key`, made where none stands), `importance` (5 where none), `validFrom` (now where none), `status` (`active` where none, or `pending`), `recallCondition`, `releaseCondition`, `labels` | 201, the memory; 409 `conflict` where a live row holds the key |
| `GET /memories/{memoryId}` | reads one | 200; 404 |
| `PATCH /memories/{memoryId}` | moves its status: `status`, `actorName`, `at` (when it stopped holding, now where none), `reason` (kept on the row moved), `text` (for a move to `done`: the words it ended on, written as a new version that is then moved, in one transaction) | 200, the row moved; 400 for `text` on another move; 404; 409 for a move the table does not allow |
| `POST /memories/{memoryId}/restorations` | restores a closed lineage: the row named is the newest of a `done` or `invalidated` lineage; `actorName` required and `text` (the closed row's where none); a new `active` version is written in the lineage, and the closed row stays as it was | 201, the new version; 404; 409 where the row is not the newest of its lineage, is not `done` or `invalidated`, or a live row holds its key |
| `POST /memories/{memoryId}/versions` | writes a new version over a live one: `actorName` required; `text`, `type`, `layerId`, `position`, `subjectIds` or `subjects` (replacing the subjects whole), `importance`, `validFrom`, `recallCondition`, `releaseCondition` (a null clears one), `labels`, each taken from the old version where not given | 201, the new version; the old is `superseded` with `validTo` the new one's `validFrom`; 409 over a row that is not live |
| `POST /merges` | writes one row over several: `memoryIds` (two or more live rows), `type`, `text`, `actorName` required; `key`, `layerId`, `position`, `subjectIds` or `subjects` (the union of the merged rows' subjects where none is given), `importance` (the highest merged where none), `validFrom` (now where none), `recallCondition` and `releaseCondition` (the one the merged rows share where none is given), `labels`; each merged row becomes `superseded` pointing at the new one | 201, the new row; 400; 404; 409 for a row that is not live |
| `GET /lineages/{lineageId}` | every version of a lineage, oldest first | 200, a list; 404 |
| `POST /hits` | records a hand-out: `memoryIds`, `actorName`, `sessionKey` (which marks the rows for that session), `at` | 201, `recordedCount` |
| `POST /confirmations` | records confirmations: `memoryIds`, `actorName`, `at`; each row's storage strength rises by the gap since its last confirmation, nothing within the same session | 201, `recordedCount` |
| `GET /types` | the types in use, each `type`, `count` (its live memories, `active` and `pending`) and `reserved`, the reserved ones first; a type with no live memory is in use only where it is reserved, with the count 0, and the closed rows of every type stay readable through `GET /memories` and `GET /tallies` by status | 200, a list |
| `GET /tallies` | the memories `GET /memories` would list under the same filters (paging and order aside), grouped `by` `status`, `type`, `layer` or `subject`: each group's `group` (the status, the type, or the layer's or subject's id in decimal; null for the rows in no layer or about no subject), `count`, `importance`, `strength` and `retrievalStrength` (each the group's mean, the retrieval strength as of the reading) and `latestValidFrom`, the largest groups first; a row about several subjects counts in each | 200, a list |

`order` is a field the list is ordered by, `id` (where none is given), `position` (by layer position, then
`id`), `validFrom`, `importance` or `hits`, each ascending, or descending with a leading `-`; ties go by
`id` in the same direction.

A version keeps the hits, the strength and the confirmation moment of the version it supersedes; a new
memory starts at strength 1 with no hits.

### Recall

`POST /recall` takes the turn: `room` (a subject by `type` and `key`), `speakers` (subjects), `words`,
`sessionKey`, `sessionOpenedAt`, `budgetTokens` (the brain's own where none is given) and `record` (true
where none is given). It answers `block`, the text the turn is handed; `chosen`, each `memoryId`, `type`,
`part` (`core` or `searched`), `score`, `marked` and `tokenCount`; `tokenCount` and `budgetTokens`; and
`released`, the ids of the rows this recall ended.

- **The core** is the room's active `situation` memories and the head of the sheet of everyone who spoke:
  the first facts `GET /subjects/{subjectId}/sheet` renders for each speaker with the turn's room as
  `roomId`, in its order, at most the rules' `core_fact_cap` of them per speaker, each a line of the block.
  The room's `rule` memories and a speaker's other facts are rows like any other: they reach the block only
  through the searched part.
- **Every line of either part keeps the budget,** a marked one as well: a line that would carry the block
  past `budgetTokens` is left out.
- **The searched part** is the active rows holding one of the turn's words or about a subject the turn
  names, scored by relevance, importance and retrieval strength under the brain's weights, at most the
  subject cap of rows per subject, no near-double of a row already in, under the budget. A `persona` or
  `episode` row, a `pending` row, and a row written after `sessionOpenedAt` are never in it.
- **A row's `recallCondition` is words a turn may say.** Where the turn's words hold at least the rules'
  `condition_share` of a row's `recallCondition`, that situation is said and the row is carried: it is a
  candidate of the searched part and its relevance is 1, the whole a relevance reaches, so it enters the block
  unless the budget or a subject's cap has filled it. A row whose condition the turn does not say, or which
  carries none, is reached by its words and its subjects as before.
- **A `releaseCondition` the turn holds ends the row.** Where at least the rules' `condition_share` of a live
  `active` row's `releaseCondition` is among the turn's words, that situation has come true: the row is never
  handed out, and the recall moves it to `invalidated` with `validTo` the moment and the condition as its
  `reason`, publishing `memory.moved`, in the recall's own transaction. `released` names every row it ended,
  and the release stands whether or not `record` is set.
- **A row handed to the session before** in the same version is marked: its line carries its id and no
  words. With `record` true, the rows handed out are hit and marked for `sessionKey`.
- **A line** is `- [#<id> <type> since <date>] <text>`, a `rumor` prefixed `heard:`, and a row whose
  predecessor was superseded within the brain's previous-value window carries `(before, until <date>:
  <text>)`.

`POST /folds` takes `sessionKey` and clears that session's marks, answering `clearedCount`.

### Writing

The agent writes its memories as it reads (`docs/design.md`, 「Writing」); these routes are the judgement's
material and its mechanics.

| Route | Does | Answers |
|---|---|---|
| `POST /similar` | `text`, `subjects` (each `type` and `key`), `limit` (10 where none, at most 50): the live memories most like the candidate, about one of its subjects or holding its words, neither an `episode` nor a `persona`, scored by the share of the candidate's words they hold and a shared subject | 200, a list |
| `POST /learnings` | a learning applied whole in one transaction: `type`, `text`, `subjects`, `importance`, `source` (`own`, `owner`, `told`, `inferred`), `confirms` (ids confirmed), `strengthens` (an id given a new version with the text), `replaces` (ids invalidated, unless the source is heard), `grounds` (episode ids linked as subjects of type `episode`), `validFrom` (when it began holding; now where none), `key`, `layerId`, `recallCondition`, `releaseCondition`, `actorName`, `labels` | 201: `confirmedCount`, `invalidatedIds`, `written` (the version written, or null where the learning only confirms), `versioned`; 400 for a ground that is no episode; 409 where `strengthens` names a row that is not live |
| `GET /unconsolidated` | the active `episode` rows not yet consolidated, most important first; `limit` (60 where none, at most 500) | 200, a list |
| `POST /consolidated` | `episodeIds`: marks them consolidated | 200, `recordedCount` |

A `told` or `inferred` learning of type `fact` is written as a `rumor`, and replaces nothing. Its `source` is
kept in the row's labels. The recall answer carries `unconsolidatedImportance`, the sum of the importance of
the unconsolidated episodes, and `consolidationDue`, whether it has reached the brain's threshold.

The skill ships the discipline a chat agent's system prompt carries, `skills/brain-harness/every-turn.md`.

### Persona

`GET /persona` answers `text/plain`: the active `persona` memories in their layers' order and their own
position, joined by a blank line, with nothing that changes between calls. `layers` (comma-separated layer
names) narrows the rendering to those layers' pieces in the same order, so a caller that builds a prompt
takes the base layer alone; a named layer that does not stand is 404.

### Brain files

| Route | Does | Answers |
|---|---|---|
| `GET /export` | the brain, or the layers `layers` names (comma-separated) with the subjects and live rows they hold, as a brain file | 200, `application/vnd.sqlite3`; 404 for a layer that does not stand |
| `POST /injections` | reads the brain file in the body (`application/vnd.sqlite3`) into the brain: a layer is reused by its name, a subject by its type and key, a live row whose key a live row of the brain holds supersedes it unless it is the same, every other live row is added; nothing is overwritten | 201, `subjectsAdded`, `layersAdded`, `memoriesAdded`, `memoriesSuperseded`; 400 for a body that is no brain file, of another shape, or holding a live row whose key is no key or whose type is `sheet`; nothing of such a file is read in |

The file is a SQLite database of the tables `brain_file` (`shape`, `exported_at`, `name`), `subjects`,
`layers`, `memories` and `memory_subjects`, its ids local to the file; this brain writes shape 2 and reads
shapes 1 and 2, a shape-1 file's rows carrying no condition.

## The console bundle

The brain's screen (`docs/design.md`, 「The screen」) is a console bundle by bmo-ui's console bundle contract
(bmo-ui's `docs/console-bundles.md`), built from `client/`. The brain answers its manifest, its module, its
stylesheet and its words, and guards every route the screen uses.

| Route | Answers |
|---|---|
| `GET /console-manifest` | 200, the manifest |
| `GET /console-bundle/words` | 200, the bundle's words |
| `GET /console-bundle/{file}` | 200, the module or the stylesheet the manifest names |

- **The manifest** is the contract's shape: the bundle `brain`, its menu title the key `brain.title.screen`,
  its icon, its screens under `/brain`, the service `brain`, the paths of its module, its stylesheet and its
  words, the API prefix `/`, the role `administrator`, no owner key, the event stream read and no socket, no
  event raised as a notice, and under `calls` the services the configuration's `[console] calls` names. The
  brain names no service itself: which service the screen translates through is the console's setting, and the
  deployment names that service here so the console relays the screen's calls to it.
- **The module and the stylesheet** are the files the build of `client/` writes into the bundle folder the
  configuration names, read when the service starts. Each is answered at `/console-bundle/` and a name
  carrying the first sixteen hexadecimal digits of the SHA-256 of its bytes, so its path changes whenever
  its bytes do and may be kept by whoever loaded it. A name the manifest does not name is `not-found`.
- **The words** are each locale's words of the bundle under the locale's code, every key under `brain`, as
  the build writes them beside the module.
- **Where the folder holds no module or no words,** the manifest, the module, the stylesheet and the words
  are all `not-found`, so a console leaves the brain out.
- **The guard.** A call with the console's token is the console relaying for a reader, and it reaches the
  manifest and the words with no role, and the module, the stylesheet and every route the screen uses where
  its `Owner-Role` header is `administrator`; one whose header is missing or names another role is refused
  403 `forbidden`, as is any other route. The OpenAPI document states each route's reach under `x-console`:
  `open`, `screen` or `closed`.
- **The routes the screen uses:** `GET /brains` and `GET /brains/{brainId}`; under a brain, the reads of
  subjects and a subject's sheet, layers, memories, lineages, types and tallies, the persona and the export; the writes of
  subjects, layers, memories, versions, moves, restorations, confirmations and learnings, and the similar
  ask; and `GET /events`.

## Splitting sheets

`bmo-brain --config <file> sheets split [--dry-run]` reads the database the configuration names, not the
routes, and splits every live `sheet` memory of every brain into `fact` memories (`docs/design.md`, 「A
person is a subject」).

- **A line is a fact** where it is a bullet: a `-`, `*` or `•`, or a number followed by `.` or `)`, then its
  words. A heading (a line beginning with `#`) and a line with no words are skipped and counted; any other
  line with words is a fact as it stands.
- **A bullet's leading bracket is read** where it carries one: the first date after `date` or `basis`, else
  a date standing first in it, is the fact's `validFrom`; the date after `confirmed` its `lastConfirmedAt`.
  The bracket is taken off the text, except where its date is a span (`<date>~<date>`), which the text keeps
  as `[<date>~<date>]` ahead of its words. Where no date is read, the fact's `validFrom` is the sheet's.
- **Each fact carries the sheet's** subjects, layer, importance, status (`active` or `pending`), the two
  conditions and labels, with the labels `splitFrom` (the sheet's id), `section` (the words of the nearest heading above it, where
  one stands) and `prefix` (the bracket as it stood, where one did); its `actorName` is `migration`.
- **The sheet is then moved to `done`** with the reason `split into facts`, in the same transaction as its
  facts, so a sheet is split whole or not at all, and a sheet already split, being `done`, is not read again.
- **It prints ids and counts alone,** never a text: a line per sheet, `sheet <id> brain <id> subjects
  <ids> lines <n> facts <n> headings <n> empty <n>`, then a line per brain, `brain <id> sheets <n> facts <n>
  skipped <n>`. `--dry-run` prints the same and writes nothing.

## Its tool

The tool host serves the brain's tool, `tools/brain/`, to a chat agent's session: `tool.toml` declares its
entries in the `bmo` namespace, each for a session holding the `room.chat` capability, and the executable
`brain-tool` answers one call handed on standard input with one tool result on standard output, as the tool
host's call protocol states.

- **It finds the brain** by `BRAIN_ADDRESS` and `BRAIN_TOKEN` in its own environment, else in `env/brain.env`
  under the data root the record's `BMO_DATA_DIR` names, else the deployment's conventional data root under
  the home the host passes through, which `bmo-brain --config <file> tool-env` writes
  from the configuration before a deployment flips.
- **It finds the agent's brain** by the turn's agent, `BMO_TURN_AGENT` in the record's variables, else the
  record's holder: the one brain registered for that agent key. The turn's room, `BMO_TURN_CHAT`, is the room
  a write links where the call names none.
- **Its entries,** each one call: `brain_episode` (`POST /brains/{brainId}/memories`, type `episode`),
  `brain_similar` (`POST …/similar`), `brain_learn` (`POST …/learnings`), `brain_move`
  (`PATCH …/memories/{memoryId}`), `brain_list` (`GET …/memories`), `brain_unconsolidated`
  (`GET …/unconsolidated`), `brain_consolidated` (`POST …/consolidated`) and `brain_fold`
  (`POST …/folds`, the session key the record's `BMO_RUN_NAME`). It writes as `agent-session`.
- **A call waits for the brain's answer** however long it takes: each entry is unbounded to the tool host, so
  no timeout ends it, only the brain's answer, a refused or broken connection, or its caller going away.
- **A deployment places it** in `tools/brain/.placed/` inside the release: the executable, the manifest and
  the schemas; and once the release stays up, registers that folder with the tool host as the service's own
  tool, so no hand touches the registry.

`GET /discipline` answers `text/plain`: the discipline a chat agent's system prompt carries, how it reads and
writes its brain every turn, the same text the skill ships as `every-turn.md`.

## The harness plugin

The plugin is `brain-harness`: the MCP server `bmo-brain mcp` starts on standard input and output, and the
skill `skills/brain-harness/SKILL.md`, which tells a harness when and how to use it. The server reaches the
service at `--brain <url>` (or `BRAIN_URL`) with the token `--token` (or `BRAIN_TOKEN`). Each tool is one
call of one route through the brain's client, `bmo-brain-client`, a tool of a brain's route taking `brainId`
(and `memoryId`) beside the route's own fields; a failure comes back as a tool error carrying the route's
status, code and sentence. A harness finds or makes its brain with `brain_brains` and `brain_brain_add`,
reads what a turn should know with `brain_recall`, writes what it learns with `brain_memory_write` and
`brain_memory_version`. The server sends no notifications.

| Tool | Route |
|---|---|
| `brain_brains` | `GET /brains` |
| `brain_brain_add` | `POST /brains` |
| `brain_subjects` | `GET /brains/{brainId}/subjects` |
| `brain_subject_add` | `POST /brains/{brainId}/subjects` |
| `brain_layers` | `GET /brains/{brainId}/layers` |
| `brain_layer_add` | `POST /brains/{brainId}/layers` |
| `brain_memories_list` | `GET /brains/{brainId}/memories` |
| `brain_memory_write` | `POST /brains/{brainId}/memories` |
| `brain_memory_read` | `GET /brains/{brainId}/memories/{memoryId}` |
| `brain_memory_merge` | `POST /brains/{brainId}/merges` |
| `brain_memory_version` | `POST /brains/{brainId}/memories/{memoryId}/versions` |
| `brain_memory_move` | `PATCH /brains/{brainId}/memories/{memoryId}` |
| `brain_hits` | `POST /brains/{brainId}/hits` |
| `brain_confirmations` | `POST /brains/{brainId}/confirmations` |
| `brain_types` | `GET /brains/{brainId}/types` |
| `brain_recall` | `POST /brains/{brainId}/recall` |
| `brain_fold` | `POST /brains/{brainId}/folds` |
| `brain_learn` | `POST /brains/{brainId}/learnings` |
| `brain_similar` | `POST /brains/{brainId}/similar` |
| `brain_unconsolidated` | `GET /brains/{brainId}/unconsolidated` |
| `brain_consolidated` | `POST /brains/{brainId}/consolidated` |
| `brain_persona` | `GET /brains/{brainId}/persona` |
| `brain_sheet` | `GET /brains/{brainId}/subjects/{subjectId}/sheet` |

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
| 403 | `forbidden` | the console's token on a route the screen does not use, or without the role the screen needs |
| 404 | `no-route` | no route answers this method and path |
| 404 | `not-found` | nothing stands at the key the path names |
| 409 | `conflict` | the member's state does not allow the change |
| 410 | `gone` | an event stream resumed from before the oldest event kept |
| 500 | `failed` | the service failed; the sentence is the failure |
