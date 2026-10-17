# The brain's boundary

This document states every route the brain service serves, every call it makes on another unit, and every
code it answers with. Another unit reaches the brain over HTTP only, and the brain reaches another unit the
same way. A route or a call that is not here is not part of the brain's boundary.

The brain serves raw material: rows, documents and their index entries. It selects nothing. The gate decides
what a turn is shown and asks the brain for the material it chooses from; the brain answers for one agent at a
time and never names the unit or the agent that calls it.

The word-chain game is not the brain's. It is a tool of its own, `bmo-word-chain`, which holds its word book,
its rules, its routes and the codes its room lines go out as; the brain serves no word-chain route and holds no
word-chain data.

## Configuration

The service's binary, `bmo-brain`, reads its configuration from the TOML file its `--config` names, at start, and
holds it for its life. `bmo-brain --config <file> serve` runs the migrations and then answers on its listen address;
the same binary with a run's words does that one piece of work and ends (「Runs launched apart」), and with
`tool-env` writes where its tools find it and ends (「Its tools」).

Deployed, the file stands under the data root as `env/brain.toml`, and each release links to it as `brain.toml`;
a deployment that finds no such file is refused (`testbed.toml`). The service's own address and token reach its
tools through `env/brain.env`, written from it (「Its tools」).

| Setting | Key | What it is |
|---|---|---|
| listen address | `listen` | where the service answers |
| token | `token` | the bearer token every caller presents |
| database | `database` | the address of the brain's own database: the tables below and the brain's settings table, nothing else |
| data root | `data_root` | where the editing locks, the fragment desks, the failure logs and the owner's room rules live |
| brain folder | `brain` | the brain root, a git repository holding every agent's documents |
| prompts folder | `prompts` | this repository's `prompts/`: the recollectors' tasks and the rulebooks only they read |
| chat agent's key | `chat_key` | the registration key of the chat agent, whose brain folder is the brain root |
| owner's number | `owner` | the owner's user id: a global memory row reaches the owner's room unless told otherwise, and its text is checked without the owner's own name |
| owner's names | `owner_names` | the names the owner goes by in the rooms, counted among the common words ([fragments](fragments.md)) |
| service unit | `service_unit` | the unit the service runs as: a failure notice of a run with no unit of its own names this unit's journal, and a run under a launched unit names that unit's |
| agency, receiver, gate | `[agency]`, `[receiver]`, `[gate]`, each with `base` and `token` | each unit's base address and the token it is called with |
| own command | `own_command` | the words that start this service's binary with its configuration, which every run the brain launches apart begins with (「Runs launched apart」) |
| policy | none | the files under `policy/`, the service's and each crate's own, built into the binary: the common words, the fragment titles' cues, the words an item is named by, the Korean words the prompts quote, and the memory, digest and person-book numbers |

Paths the brain writes into an answer are relative to the brain folder or to the data root, never absolute.

## What the brain owns

- **Its tables:** `memories`, `fragments`, `notes`, `watches`, `reminders` and `session_people`. Every row of
  the first five names its agent in an `agent` column; an insert names its agent, and the column has no
  default. Beside them, `fragment_kinds`: the word a screen calls each shipped kind by, as its owner last handed
  it in, one row a kind.
- **Its settings table,** of the key-value shape, for its own keys only:
  - `digest_paused_until` and `digest_pause_noticed`, the owner's pause of both digests and whether the pause
    has been told;
  - the person-book sort's failure counters, one per fingerprint of a failed group, cut or sheet;
  - the day a book starts reading, one key `people_digest_since:<agent>:<chatId>` per agent and room, in epoch
    seconds, which the brain sends with every unread read and count it asks the receiver for;
  - the brain holds, one key `brain_hold:<run>` per job run holding the editors off;
  - the chat-session summaries, one key `<agent>_pending_summary` per agent (「Chat-session summaries」).
- **Its migrations:** the brain's own migrations make the tables and the settings table, each in the shape and
  with the indexes the archive holds it in, so the archive's rows move across as they stand; the one difference
  is the `agent` column's, which has no default. `fragment_kinds` has no archive counterpart: the archive's
  screens named the kinds in their own code, and the table fills as each owner hands its entries in.
- **Its folder:** the brain repository and its documents, and under the data root the editing locks with their
  ticket queues and fences, the fragment desks, the failure logs `digest-failures.log` and
  `people-digest-failures.log`, and the owner's room rules, one document `room-rules/<chatId>.json` per room
  (「Owner room rules」).
- **Its prompts:** the recollectors' task prompts and the rulebooks only they read, the fragment tool's and the
  recollection's, are this repository's, under `prompts/`: each editor's tasks in a folder of its own
  (`digest/task.md`, `people/{task,sort,trim,split}.md`), the chat-session summariser's task
  (`session-summary/chat.md`), and the two rulebooks under `rules/`. The rulebooks every document writer shares,
  document writing, plain writing and the one against destroying the system, are the agency's, read through its
  rulebook read. A writer's prompt is the five in that order, document writing, plain writing, the fragment tool,
  recollection and self preservation, each headed by its name, then its task. The task and this repository's two
  rulebooks have their `{{name}}` marks filled: the run's own (the agent it writes for by its label, the batch,
  the documents) and the Korean words the prompts quote, which stand in `policy/prompts.toml` so the prompt
  files hold none; a mark nothing fills is left as it stands. The agency's rulebooks go in as they are handed over.
- **Its tools:** the tools bound to the brain are this repository's, each under `tools/` with its own
  `tool.toml` manifest, which the tool host reads as it reads any tool's: `tools/memory`, the memory rows an
  agent keeps, the note it keeps for a room's digest and the watch it sets, and `tools/fragment`, an editor's
  fragment desk. Each answers through the routes below, over HTTP, and opens no database.
  - **The call:** the tool host's protocol 1. The executable is handed one JSON object on standard input,
    the protocol version, `kind` (`call`, with the entry's full name and its arguments) and the session record,
    and answers one on standard output: a tool result, `content` text pieces and `isError`. A refusal is the
    route's sentence marked `isError` (the fragment tool's headed `REFUSED: `, a watch's `watch: `), a failure
    to reach the brain `ERROR: ` and its reason; `memory_list` answers its reading as `structuredContent`
    `{lines}` under its output schema, the same object as its text. A request that does not read ends the
    executable unclean with why on standard error. `crates/tool-call` is the brain's side of it.
  - **What a tool reads of the record:** `dataRoot`; the turn, from the run its `place` names (`run.json`, none
    for a person's turn) over `runVariables`: `BMO_TURN_AGENT` the agent, `BMO_TURN_CHAT` the room; and an
    editor's `fragmentDesk`. Every other field is passed over.
  - **Where the brain stands:** `BRAIN_ADDRESS` (its base address) and `BRAIN_TOKEN` from the tool's own
    environment, else from `env/brain.env` under the record's data root; a tool placed by the host has a clean
    environment.
  - **That file is written from the configuration** by `bmo-brain --config <file> tool-env`, which every
    deployment runs before its flip: `BRAIN_ADDRESS` is `http://` and the listen address, an unspecified host
    (`0.0.0.0`, `::`) answered as the loopback one, and `BRAIN_TOKEN` the configured token. It is written under
    the configured data root, readable by the user alone, and only where it would change; a value an env file
    cannot hold bare (empty, or carrying whitespace, a quote, `#`, `$` or `\`) refuses it with exit code 1 and
    nothing written.
  - **A deployment places each tool** in its folder's `.placed/` inside the release, the folder the tool host
    reads: the executable the release's `bin/` holds, the manifest and every schema file its entries name. The
    steps that do it stand in `testbed.toml`, whose `[tools]` table names `tools/` and whose release binaries
    include every tool's executable; the repository's own test holds the declaration to every `tool.toml`.
  - **A turn a companion's reset overtook** is refused by the agency before its tools are called; the brain's
    tools read no reset point.

## Wire conventions

The brain follows the chat wire protocol's conventions (the protocol document of `bmo-chat`):

- **Bodies.** JSON, with field names in camelCase. An optional field may be left out; a nullable field is sent
  as null when it has no value. A tool argument keeps its schema's meaning under its camelCase name
  (`for_every_agent` is `forEveryAgent`, `distinct_from` is `distinctFrom`).
- **Numbers.** Room, person, message, row and item numbers are signed 64-bit integers. Times are Unix seconds.
- **Auth.** Every route needs an `Authorization: Bearer <token>` header carrying the brain's token. A missing
  or wrong token is answered 401. The brain calls another unit with that unit's token from its own
  configuration.
- **Refusals are answers.** A request the brain understood and will not carry out is answered 200 with a typed
  refusal: `answered` false and a `refusal` object holding `code`, `message` (an English sentence) and the
  code's own parameters. An answer carried out says `answered` true.
- **Retrying.** The brain offers a call again only when the connection was refused, within a patience window
  of two minutes, offered every two seconds; a call that reached a listener is never offered again. A caller of
  the brain is held to the same rule.

### The error shape

Every non-2xx answer carries `code` (a string below) and `error` (an English sentence saying what went wrong).

| Status | `code` | When |
|---|---|---|
| 400 | `bad-request` | the body is not JSON, or not the shape the route takes; the sentence names what is missing |
| 401 | `unauthorized` | the token is missing or wrong |
| 404 | `no-route` | no route at this method and path |
| 404 | `not-found` | the row or run the request names does not exist |
| 409 | `not-ready` | the service is up but cannot answer yet: its database or brain folder is not open |
| 409 | `conflict` | the request contradicts the row's state; the answer carries the refusal code as well, in `refusal` |
| 500 | `failed` | the service failed while answering; the sentence is the failure |

## The turn a request is for

A route a tool calls carries the turn the tool runs in:

| Field | Type | Meaning |
|---|---|---|
| `agent` | string | the agent the turn is for, by its registration name |
| `chatId` | number or null | the turn's own room |
| `messageId` | number or null | the message the turn answers |

A request whose turn names no agent is refused `no-agent`; a room operation whose turn names no room, and whose
arguments name none, is refused `no-room`. Nothing is filed under a default agent.

## Routes the brain serves

### Health

| Route | Answer |
|---|---|
| `GET /health` | `ready` (boolean) and `reason` (string or null): not ready where the database does not answer or the brain folder is no git repository, the reason saying which |

### Memory rows

The memory tools' routes, one per tool schema. Each body is `turn` plus the tool's arguments; each answer is a
reading, `lines` (a list of strings, the reading one line each), or a refusal (「Memory refusals」 below).
`chat` names another room than the turn's, and `global` names a row tied to no room; the two are never given
together.

| Route | Tool | Arguments |
|---|---|---|
| `POST /memory/list` | `memory_list` | `chat`, `global`, `all`, `type` |
| `POST /memory/add` | `memory_add` | `type`, `action` (required); `condition`, `on`, `until` with `name`, `every`, `within`; `slug`, `for`, `by`, `message`, `todo`, `reach`, `replaces`, `chat`, `global` |
| `POST /memory/update` | `memory_update` | `memory`, `action` (required); `chat`, `message` |
| `POST /memory/done` | `memory_done` | `memory` (required); `text`, `chat`, `message` |
| `POST /memory/retire` | `memory_retire` | `memory`, `why` (required); `chat`, `global`, `message` |
| `POST /memory/revive` | `memory_revive` | `memory` (required); `chat`, `global`, `message` |
| `POST /memory/candidate` | `memory_candidate` | `memory`, `why` (required); `chat`, `global`, `message` |
| `POST /notes/add` | `note_add` | `text` (required); `author`, `global`, `chat`, `message` |
| `POST /watches/set` | `watch_set` | `note` (required); `until`, `at`, `name`, `every`, `within`, `item`, `chat`, `message` |

`memory` takes a row's number or its slug. `on` is an event: `deployed`, `job-ended` or `queue-changed`, the
first two with an optional `:<item>`.

The rows the gate reads and the facts it hands back:

| Route | Body | Answer |
|---|---|---|
| `POST /memory/standing` | `agent`, `chatId` | `rows`: the room's open rows that wait on nothing and the errands whose event has come, oldest first. States unmoved for twelve hours are released first. |
| `POST /memory/conditions` | `agent`, `chatId` | `rows`: `slug`, `condition`, `action` of every open row with a condition, the room's own and the global rows whose reach takes in the room's kind. Rows unmatched for sixty days are withered first, at most once an hour for a room. |
| `POST /memory/matched` | `agent`, `chatId`, `matched` (slugs), `spent` (slugs) | `ok`. A matched row's count and last match are written; a spent row is taken down with the matcher as its reason. |
| `POST /memory/events` | `agent`, `chatId`, `events` (each `kind`, `item` or null) | `rows`: the rows the events wake, by the event alone: the room's situation rows and the global rows that reach it. Every errand an event wakes, of any agent and room, has its first meeting written on it, and each row answered its count and last match. |

`POST /memory/standing` and `POST /memory/events` answer each row whole, in the row shape below. A row's kind of
room is read off the receiver's roads: a global row reaches the room where its `reach` is `all` or the room's
kind (`owner` or `group`).

The console's routes:

| Route | Body | Answer |
|---|---|---|
| `GET /memories` | query `agent` and `chat`, both optional | `memories`: every row, or one agent's, or one room's, newest first; 400 where `chat` does not start with a whole number |
| `POST /memories/active` | `slug`, `active`, `why` (required to take a row down) | `ok` and `memory`, the row as it now stands |

A row carries `id`, `agent`, `slug`, `type`, `chatId`, `reach`, `condition`, `action`, `forUser`, `messageId`,
`todoId`, `setBy`, `setAt`, `active`, `lastHitAt`, `hits`, `doneAt`, `doneText`, `downReason`, `downAt`,
`candidateWhy`, `candidateAt`, `event` (`kind`, `item`), `eventMetAt` and `movedAt`. `POST /memories/active`
answers 400 when `slug` or `active` is missing or when a row taken down carries no `why`, 404 `not-found` for no
such row, and 409 `conflict` with the refusal `missing` (the row went while it was changed), `other-room` (it
moved to another room meanwhile), `already-up` or `already-down`. A slug names one row whatever its agent.

### Notes

| Route | Body | Answer |
|---|---|---|
| `POST /notes/add` | as above | a reading |
| `POST /notes/list` | `agent`, `chatId` or null | `notes`: `id`, `agent`, `chatId`, `author`, `text`, `global`, `at`, the notes still waiting for the digest, oldest first |

### Watches and reminders

| Route | Body | Answer |
|---|---|---|
| `POST /watches/list` | `by` (the room that set them) or null for every room, `agent` or null, `all` | `watches`, oldest first, the standing ones or with `all` every one: each `id`, `agent`, `chatId`, `by`, `kind` (`condition` or `timer`), `name` (as a line shows it), `note`, `check`, `every` (seconds), `dueAt`, `deadlineAt`, `state` (`active`, `fired`, `lapsed`, `broken`, `cancelled`), `unit`, `lastCheckedAt`, `lastExit`, `item`, `setAt`, `endedAt`; and `lines`, the same watches as a session reads them back |
| `POST /watches/cancel` | `id`, `by` | a reading; refused `no-watch` for no such watch, `not-theirs` where `by` is neither the room that set it nor the owner's room, `ended` where it has already ended, and `no-owner` where the configuration names no owner |
| `POST /watches/launch` | `id` | `launched` (boolean) |
| `POST /watches/claim` | `id`, `run` | `standing` (boolean): whether the watch still stands and is held by this run |
| `POST /watches/end` | `id`, `run`, `outcome` (`fired`, `lapsed`, `broken`), `exit` and `printed` (what the last check exited with and printed; `printed` is its output where it fired and its error where it broke) | `ended` (boolean): whether this run ended it; its memo is queued once, whichever run ends it first, and a run that no longer holds it ends nothing |
| `POST /reminders/set` | `agent`, `chatId`, `at` (an ISO moment or `YYYY-MM-DD HH:MM` on the Seoul clock) or `every` (`daily HH:MM`, `weekly <mon…sun> HH:MM` or `monthly <1-31> HH:MM`), `reason`, and optionally `memory` (an id or a slug) and `user` | a reading of the reminder's line; refused `usage` without a reason or with both or neither of `at` and `every`, and `reminder-refused` for a reason over 1000 characters, a moment that does not read or is past, a rule that does not read, or a memory that is gone, another agent's, another room's or no longer open |
| `POST /reminders/cancel` | `id` | a reading; refused `no-reminder` where there is none; a repeating one goes with every moment after it |
| `POST /reminders/list` | `chatId`, `agent` or null, `all` | `reminders`, soonest first, the ones still to come or with `all` every one: each `id`, `agent`, `chatId`, `user`, `dueAt`, `every` (the repeat rule or null), `reason`, `item`, `fired`; and `lines`, the same as a session reads them back |
| `POST /watches/stopped` | `unit` | `ended` (boolean): the standing watch the unit holds ended as cancelled with its memo telling the room that set it nothing more is coming; false where the unit holds none. The agency calls it before it stops a watch's unit, so the clock has nothing standing to launch again |

A watch's run is the brain's own binary, `watch-runner <id> <unit>` (「Runs launched apart」), which claims and ends
its watch as these routes do. A memo goes to the agent that set the watch, in the room that set it, as a quiet
instruction of kind `watch`; the watch is ended only once the agency has taken the memo.

### Fragments

The fragment tool's routes, one per tool schema. Each body is `desk` (the desk the run's session record names,
as the session start handed it) plus the tool's arguments; each answer is a reading or a refusal (「Fragment
refusals」 below). A desk is found only among the desks open under the data root: a `desk` naming any other
file, or a desk already closed, is refused `no-desk`. Each call reads the desk afresh; a call that writes saves
it whole, or leaves it as it stood on a refusal, and writes nothing to the table, which the batch writes once
the run ends.

- **Arguments as the tool schemas take them:** `keywords` is one text of words separated by commas; `items`
  are positive whole numbers; `status` is `open` or `done`; `from` of a merge names at least one piece, and
  `pieces` of a split at least one, each `path`, `title`, `keywords`, `items` and `shared` or null. A body
  of another shape is 400.
- **The readings:** a search answers one line per row the run may reach, with its own writes laid over them, or
  `No row matches.`, cut under the answer ceiling with a line saying how to narrow it; a write, a merge and a
  split answer the rows they wrote; a remove what it took down; owed the pieces still owed a row, or that
  nothing is owed.

| Route | Tool | Arguments |
|---|---|---|
| `POST /fragments/desk/search` | `fragment_search` | `text`, `item`, `person`, `status`, `down` |
| `POST /fragments/desk/write` | `fragment_write` | `path`, `title`, `keywords`, `items` (required); `shared`, `forEveryAgent`, `distinctFrom` |
| `POST /fragments/desk/remove` | `fragment_remove` | `path` |
| `POST /fragments/desk/merge` | `fragment_merge` | `into`, `from`, `title` (required); `keywords`, `items`, `shared` |
| `POST /fragments/desk/split` | `fragment_split` | `from`, `pieces` |
| `POST /fragments/desk/owed` | `fragment_owed` | none |

The rows the gate reads, and what it hands back:

| Route | Body | Answer |
|---|---|---|
| `POST /fragments/offer` | `agent`, `chatId`, `kinds` | `fragments`: the active rows of those kinds the turn's agent may be offered in the room, in position order (「What a room may be offered」 in `fragments.md`) |
| `POST /fragments/hits` | `agent`, `chatId`, `attached` (each `kind`, `chatId`, `owner`, `slug`) | `ok`; each attached row's count and last hit are written, the row found by those four among the active rows the turn's agent may read |

The console's and the hand writer's routes:

| Route | Body | Answer |
|---|---|---|
| `GET /fragments` | query `kind`, `chat`, `person`, `item`, `text`, `down`, all optional | `fragments`: the rows that are up, or with `down` every row, narrowed by a kind, a room, a person, an item, or a word their title or a keyword carries; 400 for a kind there is none of |
| `POST /fragments/active` | `kind`, `slug`, `active`, `chatId` or null, `owner` or null, `agent` or null | `ok` and `fragment`, the row as it now stands, its standing and its hand set to `active` (「A standing set by hand」 in `fragments.md`); the row is the named agent's, the chat agent's where none is named. 400 for a kind there is none of or a field missing, 404 `not-found` for no such row |
| `POST /fragments/write` | `path`, `title`, `keywords`, `items`, `shared` or null, `chatId` or null, `forEveryAgent` or null | a reading of the row's index line; refused `row-refused` where the row checks refuse it, with the reasons; the index files are drawn after it. A knowledge chunk new to the table names `shared` or a `chatId` |
| `POST /fragments/remove` | `path` | a reading; refused `body-stands` where the file still stands |
| `POST /fragments/check` | `kind` or null | `drift` (each `trouble`: `unlisted`, `bodiless` or `moved`, with `kind`, `chatId`, `owner`, `slug` and `said`) and `titles` (each refused title's `kind`, `chatId`, `owner`, `slug`, `path`, `title` and `problem`, and `counted`: each brain kind's refused and standing rows) (「The drift check」 in `fragments.md`) |
| `POST /fragments/sync` | none | a reading of what the reconcile changed, one line per brain library, and of each index file drawn |

A fragment carries `kind`, `chatId`, `owner`, `agent`, `allAgents`, `slug`, `title`, `path`, `scope`,
`position`, `active`, `lastHitAt`, `hits`, `setBy`, `setAt`, `hand`, `handAt`, `keywords`, `items`,
`writtenOn`, `confirmedOn`, `status` and `bodyHash`. `kind` is one of the brain's own `room-rule`,
`library-knowledge` and `person-book`, or a kind its owner hands in (「Shipped libraries」 below).

### Shipped libraries

| Route | Body | Answer |
|---|---|---|
| `POST /libraries/reconcile` | `kind`, `label`, `entries` (each `slug`, `title`, `path`, `position`, `scope` or null, `items`, `writtenOn`, `confirmedOn`, `bodyHash`) | a reading of one line: `<kind>: <n> entries · <n> added · <n> updated · <n> raised · <n> taken down · <n> bodies read · <n> settled` |
| `GET /fragments/kinds` | none | `kinds`: each kind's `kind` and `label`, the brain's own three first and then every kind an owner has handed in, by kind |

A shipped library is not the brain's. Its owner reads its own index and bodies and hands the entries in whole,
with the word a screen calls its kind by; the brain brings the kind's rows to them (「The shipped libraries」 in
`fragments.md`) and keeps the label in `fragment_kinds`, the last one handed in standing. The brain names no
shipped kind: it takes any kind but its own, and a kind the brain writes itself is refused `brain-library`. A
body with an empty `kind` or `label` is answered 400. The brain's own kinds answer their labels themselves.

### Brain documents

| Route | Body | Answer |
|---|---|---|
| `POST /knowledge/read` | `agent`, `chatId`, `slugs` | `documents`, in the order asked: `slug`, `path`, `text`, `version`; a slug that is not among the knowledge chunks the room may be offered (its own and the shared ones, 「What a room may be offered」 in `fragments.md`) is refused `out-of-room` with those `slugs`, and nothing is read |
| `POST /people/read` | `agent`, `chatId`, `senders`, `slugs` | `sheets`: for each sender in turn, the person's common sheet, then their sheet of this room where the room keeps a room part, each `person`, `part` (`common` or `room`), `path`, `text`, `version`, a sheet that is missing, empty or over its ceiling left out, and none at all for an agent keeping no long-term memory; and `pieces`, in the order asked: `slug`, `path`, `text`, `version`, `owner`; a slug that is not among the person-book pieces the room may be offered (its room part where it keeps one, the common pieces of those who spoke in it) is refused `out-of-room` with those `slugs` |

`text` is the file trimmed, and `version` its hash; a session holds a document by its path and version. A path is
relative to the brain root, and a piece whose file is gone is left out.

### Owner room rules

The rules the owner grants in a room: each names one sender (`subject`, a user id) and what that person may
ask the room's agents for (`text`). They are not the room-rule chunks of the brain's library: they are rows the
owner sets, read by whoever decides whose message an agent answers in the room, which fills them into its own
block. A room's rows are one document, `room-rules/<chatId>.json` under the data root, every agent of the room
reading the same rows. A change replaces or removes one rule by its id and is made only on a message the
receiver confirms the room's owner sent; a rule that is revoked leaves nothing behind.

| Route | Body | Answer |
|---|---|---|
| `POST /room-rules/read` | `chatId` | `rules`: each `id`, `subject`, `text`, in the order they were set; none for a room with no document |
| `POST /room-rules/set` | `chatId`, `messageId`, `id`, `rule` (`subject`, `text`) or null to revoke | `rules`, the room's rows as they now stand; refused `not-owner` unless the receiver confirms the message's sender is the owner |

An `id` is 1 to 80 letters, digits, `_` or `-`; a `subject` is a whole number; a `text` is not blank and at most
8000 characters, kept trimmed. Any other is 400. A change while another change of the same room holds its lock is
409 `conflict` with the refusal `busy`; nothing waits for the lock.

### Session document loads

What a session has already been handed of the brain's documents, so a document goes in full once per version
and as a mark after that.

| Route | Body | Answer |
|---|---|---|
| `POST /sessions/loads` | `session` | `loads`: `path`, `version` |
| `POST /sessions/loads/keep` | `session`, `loads` (each `path`, `version`), `at` | `ok`; the sessions untouched for a week are let go at the same stroke |
| `POST /sessions/loads/forget` | `session` | `ok`; a folded session holds nothing |

A document the brain changes is forgotten in every session holding it before the reload notice goes out.

### Chat-session summaries

The handoff note a chat agent's folded session leaves for its next one. The note is handed in already
sanitised and already whole: the lines that carry it and the summary cut to its cap, or the lines saying the
summary was lost where none came, as the next fresh session opens with it. The brain keeps it as it came, in
its settings table under `<agent>_pending_summary`, one per agent, the next keep overwriting it; a read answers
it until a clear forgets it.

| Route | Body | Answer |
|---|---|---|
| `POST /session-summary/keep` | `agent`, `note` | `ok` |
| `POST /session-summary/read` | `agent` | `note` (string or null) |
| `POST /session-summary/clear` | `agent` | `ok` |

### The digests

| Route | Body | Answer |
|---|---|---|
| `POST /digest/run` | `chatId` or null | `outcome`: `paused` (the owner told once of what waits behind the pause), `resting`, `nothing` (no note waits, in the room where one is named), or the memo digest's launch: `launched` or `running` with `unit`, or `refused` with `why` |
| `POST /digest/now` | `agent`, `chatId` | `outcome`: `resting` (a rest is on), `nothing` (no note waits), `running` with `unit`, `launched` with `unit` and `notes`, or `refused` with `why`, the unit launch's refusal |
| `POST /digest/people` | `chatId`, `agent`, `all`, `split` with `person` and `spans`, `sheets`, `reform` with `batches`, `sort` with `person`, `all` or `status` and `groups`, `since` (a day, `none`, or absent to read it), `forgetRead` with `chatId` or `all` | `outcome`: `answered` with a reading for what is read or set in the request (`status`, `since`, `forgetRead`), or for the work itself its launch: `launched` or `running` with `unit`, or `refused` with `why` |
| `POST /digest/pause` | `set`: `status`, `off`, `on`, or a length (`2h`, `90m`, `30s`, a bare number of minutes) | `pausedUntil` (time, or null for none, or `forever`) and a reading of how the digests stand |
| `POST /people/before-fold` | `chatId`, `agent` | `outcome`: `launched` or `running` with `unit`, `skipped` (the agent's book is not written from the room), or `refused` with `why` |

`POST /digest/people` names its agent by registration, the chat agent where it names none. It refuses before it
launches anything: `no-memory` for an agent that keeps no long-term memory, `not-answered` for a room the agent
does not answer, `one-to-one` for a room that keeps no room part, and `too-few-batches` for a reform whose folded
messages do not fit the batches it was given. A body whose forms do not go together is answered 400, the
sentence naming the form it takes.

### Brain holds

| Route | Body | Answer |
|---|---|---|
| `POST /brain/hold` | `run` | `held` and `taken` (the editing locks a batch holds now, tried without waiting) |
| `GET /brain/hold` | none | `holders` (runs) and `taken` (locks) |
| `POST /brain/wait` | none | `free` (boolean) and a reading: every lock free, or still taken after nine minutes |

`POST /brain/wait` looks every five seconds for at most nine minutes, so a caller's own wait is longer.

### Rooms leaving

| Route | Body | Answer |
|---|---|---|
| `POST /rooms/forgotten` | `chatId` | `ok`; the room's withering stamps are let go |
| `POST /rooms/removed` | `chatId`, `preview` (boolean) | `took`: `rows`, the count of the room's rows per table (`memories`, `fragments`, `notes`, `reminders`, and `watches` the room set or that watch it), `settings`, its keys (each agent's day its book of the room starts reading), and `folders`, the room's folder under each agent's, relative to the brain root; with `preview` false the rows and keys are removed, and each folder is removed and its removal committed under the editing lock of the agent's folder it stands in; `couldNot` lists what could not be removed |

### Agents forgetting

| Route | Body | Answer |
|---|---|---|
| `POST /agents/reset` | `agent`, `preview` (boolean) | `took`: `rows`, the count of the agent's rows per table (`memories`, `reminders`, `watches`, `notes`, `fragments`), and `folder`, its brain folder relative to the brain root where it stands, else null; with `preview` false every program still working in its folder is killed, the folder fenced and its editing lock taken ahead of every waiter (a taker still holding it killed rather than waited for), what a stopped run left unwritten put back, its rows removed, its read marks forgotten at the receiver, its book of every room it answers made to read from now, its folder removed and the removal committed, the folder fenced again and the lock let go; `couldNot` lists what it could not do. Its recollector runs are the agency's to stop, which the agency's own reset does first |
| `POST /agents/removed` | `agent` | `ok` and `left`, what could not be removed; the agent's editing lock, its ticket queue and its fence are removed. The folder is the one the agency registers for the agent, so the agency calls this before it forgets the registration; an agent it registers no folder for has nothing to remove |
| `POST /agents/fence` | `agent` | `ok`; the agent's folder fenced now, so no writer begun before this moment takes the folder's editing lock again. An agent the agency registers no folder for has nothing fenced |
| `POST /agents/lock` | `agent` | `path`: the agent's folder's editing lock file, relative to the data root, which the caller may take as any writer takes it; null where the agency registers no folder for the agent |
| `POST /agents/book-since` | `agent`, `chatId`, `at` | `ok`; the agent's person book of the room reads from `at`, in seconds, on: the day a book starts reading (「What the brain owns」) set to it |
| `POST /agents/put-back` | `agent` | `ok`; the agent's folder put back to its last commit in the brain repository, so what its stopped recollector runs wrote and left uncommitted there goes. It takes no lock: the caller holds the folder's editing lock, taken through its path, while it asks. An agent the agency registers no folder for, or whose folder does not stand, has nothing undone; the brain root holds every other agent's folder, so the chat agent's is refused `brain-root` |

An agent's folder is its registration's brain folder, read through the agency's registration lookup. The chat
agent's is the brain root, so `POST /agents/reset` and `POST /agents/put-back` refuse it `brain-root`.

## Calls the brain makes

Each call is one POST of a JSON body under the serving unit's configured base address, carrying that unit's
token, at the path the tables below give. The paths are the brain's side of the contract, and the serving
unit's own boundary document states the operation at the same path. An operation that sends none sends an
empty object. A call is answered, refused and retried by the wire conventions above. The brain holds no other
unit's tables or files.

### The agency

| Operation | Path | Sends | Answers |
|---|---|---|---|
| registration lookup | `/registrations/lookup` | `agent`, or null for all | `agents`: each agent's `name`, `label` (or null), `key`, `brainFolder`, `awake`, `recollectorRegistration` (or null), `reasoning` (or null) |
| recollector session start | `/runs/start` | `workKey`, `agent`, `chatId`, `unit` or null, `capSeconds`, `step` and `steps` (or null), `prompt`, `tools`, `workdir`, `desk`, `registration` or null | `run` |
| recollector session wait | `/runs/wait` | `run` | `outcome` (`answered`, `failed`, `overran`, `stopped` or `limit`), `answer` (or null), `filesWritten`, `cost` (or null), `size` (or null) |
| recollector session stop | `/runs/stop` | `run` | `ok` |
| overrun ask | `/runs/overrun` | `run`, `unit`, `capSeconds` | `keep` (boolean) and `until` (or null) |
| run open | `/runs/open` | `run` | `open` (boolean): whether a job's run still stands |
| cap end | `/runs/cap-ends` | `unit` | `until` (or null): the moment the open session started under that unit runs out of its cap, the latest where several stand; null where none stands |
| clock keep | `/runs/kept-until` | `run`: the run or unit a lock ticket names | `until` (or null): the moment the open overrun ask on it stands until, the keep's end where it was kept and the moment it must be answered by while it is not yet; null where no ask stands open or it was cut |
| rulebook read | `/rulebooks/read` | `names` | `rulebooks`: each named rulebook's `name` and `text`, trimmed, in the order asked |
| work key cap and reasoning | `/work-keys/read` | `workKey`, `agent` | `capSeconds`, `reasoning` (or null) |
| item states | `/items/states` | `items` | `items`: each item's `item` and `finished` (boolean), for a fragment's status and a trim's stretch |
| unit launch | `/units/launch` | `unit`, `kind`, `title`, `agent` or null, `command` (its words, in order) | `outcome`: `launched`, `running` (a run of that unit stands, and nothing is started) or `refused`, and `why` (or null) |
| units running | `/units/running` | `units` | `running`: those of the units a run stands under, as a unit, an open execution or a run the agency's host holds |
| quiet instruction | `/instructions/quiet` | `agent`, `chatId`, `text`, `kind` or null | `ok` |
| document reload notice | `/documents/reload` | `agent`, `paths`, `rooms` | `ok` |

The recollector runs are the agency's sessions; the brain decides what to recollect, writes the prompt, opens
the desk, holds the lock and judges the result. Everything about the model run itself is the agency's: the
logins it goes out on, the run's usage and cache events, and a usage limit. A session that reaches a limit
ends with the outcome `limit`; the agency has rested its login and told the owner where that rest silenced the
last open login, as for every session it runs. The brain writes no rest, and the run stops without a retry.

What the brain sends and does around one session:

- **The cap** is the work key's `capSeconds` for the agent the run writes for: the agency folds in the request
  wait of the registration that agent's recollectors run on, where that is longer.
- **The start** names the work key, the agent, the room of the batch, the unit the batch runs in (null where it
  runs in none), the cap, which batch it is of how many (null for a run that is no batch of a planned run), the
  prompt, the tools (the file tools `read`, `write`, `edit`, `glob` and `grep`, and the brain's `fragment`
  tool), the folder the run stands in, the desk and the registration its agent's recollectors run on. The
  agency keeps the run's record from it: opened at the start and closed at the session's end with its outcome,
  what it cost and the size it ended carrying.
- **The clock** is the brain's: once the session passes its cap the brain makes the overrun ask, naming the
  batch's unit, or the run where the batch runs in none. Kept, it asks again at `until`, or one cap later where
  `until` is null; not kept, or where the ask has no answer, the brain stops the session, waits for its end and
  fails the batch as cut at its cap.
- **The end:** `answered` with an answer is the run's answer, the files it wrote named from the folder it stands
  in; `answered` with none, `failed`, `overran` and `stopped` fail the batch; `limit` stops it without a retry.

#### Runs launched apart

Work that must outlive a request runs apart: one run of the brain's own binary in a unit of its own, started
through the agency's unit launch, which answers `running` where a run of that unit already stands, so the unit's
name is the run's mutex. The command is the own command, then the run's words below, then `--unit <unit>`: the run
knows its unit by it, and its clock, its brain hold and a failure's record are named by it.

| Unit | `kind` | `agent` | `title` | Words |
|---|---|---|---|---|
| `bmo-digest` | `digest` | null | `All pending notes` | `digest` |
| `bmo-digest-<chatId>` | `digest` | null | the room's name | `digest --chat <chatId>` |
| `bmo-people-digest` | `people-digest` | null | `Rooms in turn` | `people-digest` |
| `bmo-people-sort-<agent>` | `people-digest` | the agent | `Sorting the person book` | `people-digest --sort --all --agent <agent>`, or `--person <userId>` for one person, with `--groups <n>` where a sort is bounded |
| `bmo-people-digest-<agent>-<chatId>` | `people-digest` | the agent | the room's name | `people-digest --chat <chatId> --agent <agent>`, and `--all` for a catch-up; asked by hand, `--split` (with `--person <userId>` and `--spans <n>`), `--sheets` or `--reform --batches <n>` |
| `bmo-watch-<id>-<milliseconds>` | `watch` | null | `<room's name> watch #<id>` | `watch-runner <id> <unit>` |

- **A room's name** is its title as the receiver's room titles give it; a room with none is `Private chat` where
  its roads are the owner's and `Room <chatId>` otherwise.
- **The due digests:** the memo digest launches `bmo-digest`; the person book launches `bmo-people-digest` and,
  for every agent keeping a long-term memory, the brain root's first, that agent's `bmo-people-sort-<agent>`, so
  one agent's sort never waits on another's. Each is launched only where no run of its unit stands.
- **The brain's clock** runs inside `serve`, each pass on its own period and never overlapping the one before it;
  a pass that fails is said on the service's error output and the next goes on.
  - The memo digest is due three minutes after the service starts and every hour after.
  - The person book is due five minutes after it starts and every three hours after.
  - The reminder pass runs every thirty seconds from the start: every reminder whose moment has come, one that
    came due while the brain was down included, wakes its agent in its room as a quiet instruction of kind
    `reminder`, saying how late it comes where it is more than five minutes past; a repeating one moves to its
    first moment still ahead, so missed moments come to one wake, and any other is fired for good. A moment is
    taken only once the agency has taken its wake.
  - The watch sweep runs every thirty seconds, its first pass thirty seconds after the start, once the agency's
    running units can be told: it ends, with no memo, every standing watch serving a memory no longer open; fires
    every timer whose moment has come, one that came due while the brain was down included, with its memo; then
    launches again the watches nobody holds, as below.
- **Before a room's session folds,** the agent's worker of that room is launched, unless the agent's book is not
  written from the room; where that cannot be read, it is launched all the same.
- **A watch's run** is named by its unit: the brain writes the unit and the moment it was armed on the watch
  before launching it, and the run claims the watch under that name. A launch the agency refuses leaves the
  watch standing for a later pass.
- **Watches nobody holds:** on each pass the brain asks the agency's units running about every standing watch's
  unit, and launches again every condition watch whose unit no run holds, once a minute has passed since it was
  armed, since a run just launched is not yet among the running. A watch past its deadline is launched all the
  same: only a run may check it one last time before it lapses.

### The receiver

A message is answered as `chatId`, `messageId`, `senderId` (or null), `senderName`, `sentAt`, `text` and
`replyToMessageId` (or null).

| Operation | Path | Sends | Answers |
|---|---|---|---|
| unread messages | `/messages/unread` | `agent`, `chatId`, `limit`, `since` or null | `messages` past the agent's read marks, sent at or after `since` where it is given, oldest first |
| unread counts | `/messages/unread-counts` | `agent`, `since`: each `chatId` and `since`, the moment the agent's book of that room starts reading | `rooms`: each room holding messages past the agent's read marks, its `chatId` and `count`; a room named in `since` counts only the messages sent at or after its moment |
| read messages | `/messages/read` | `agent`, `chatId`, `from`, `limit` | `messages` the agent has marked, after the message `from` in the room's order (or, where that message is gone, with an id above `from`), oldest first |
| read count | `/read-marks/count` | `agent`, `chatId` | `count`: the agent's marks on the room's messages that still stand |
| keep read marks | `/read-marks/keep` | `agent`, `chatId`, `messageIds` | `ok` |
| forget read marks | `/read-marks/forget` | `agent`, `chatId` or null for every room | `forgotten`: the count of marks let go |
| room titles | `/rooms/titles` | none | `rooms`: each room's `chatId`, `title` (or null) and `senders`, the people that have stood as a sender in it |
| room answerers | `/rooms/answerers` | `chatId` or null | `rooms`: each room's `chatId` and `agents`, the agents answering it: the awake agents it names, or the default answerer where it names none awake |
| spoke between | `/rooms/spoke-between` | `chatId`, `person`, `from`, `to` | `spoke` (boolean): the person sent a message in the room between the two moments, both included |
| own number | `/rooms/own-number` | `agent`, `chatId` | `number` (or null): the sender the agent's own words stand under in the room: the number the room's road gave the agent, or for the default answerer where it gave none, who the road says the bot is there |
| spoken in | `/rooms/spoken-in` | `chatId`, `people` | `people`: those who have stood as a sender in the room |
| display names | `/people/names` | `people` | `names`: each person's `person` and `name` or null: the name their latest message went under, a room where the owner goes by a name of that room alone read last |
| waits on a happening | `/promises/waits` | `text` | `waits` (boolean): whether the text waits for something to happen first, by the receiver's reading of promises |
| roads | `/rooms/roads` | `chatId` | `kind` (`owner` or `group`) and `oneToOne` (boolean) |
| sender is owner | `/messages/sender-is-owner` | `chatId`, `messageId` | `owner` (boolean) |
| owner spoke | `/rooms/owner-spoke` | `chatId`, `person`, `before`, `seconds` | `owner` (boolean): the person is the owner; `spoke` (boolean): the owner sent a message in the room in that span |
| rest state | `/rest` | none | `resting` (boolean) and `until` (or null) |
| owner notice | `/owner/notice` | `code` and its parameters beside it | `ok` |

### The gate

| Operation | Path | Sends | Answers |
|---|---|---|---|
| document sanitiser | `/sanitiser/documents` | `path`, `text` | `text`, or a refusal (`answered` false) naming why |
| sheet judgement | `/sheets/judge` | `agent`, `part` (`common` or `room`), `chatId` (the room's, or null for the common part), `person`, `pieces` (each `path` and `text`), `ceiling`, `registration` or null | `sheet`, or null where none came |

Which texts the sanitiser needs to look at is the gate's: the brain sends every document an editor changed and
writes back only a text that came back changed.

### Owner notices

What the brain tells the owner goes as a code and its parameters through the receiver's owner notice, which the
web unit's language packs render. The brain writes no sentence of it but a failure's `cause`: why a run did not
stand, an English sentence the notice carries as it is. Where the receiver does not take a failure's notice, its
line goes to the failure log instead (「What the brain owns」).

| Code | Parameters | When |
|---|---|---|
| `digest-paused` | `until` (epoch seconds, or null for a pause until it is lifted), `notes` (the count not yet filed), `rooms` (each `chatId`, `title` or null, `agent` by its label, `messages` unread) | the first hourly launch that finds the owner's pause holding with work waiting behind it, once for that pause |
| `digest-failed` | `chatId`, `title` or null, `agent` by its label, `notes` (the count left pending), `cause` | a room's notes digest did not stand; its notes are tried again at the next digest |
| `rule-candidate` | `agent` by its label, `candidate` (the item as the candidates document holds it) | a digest batch wrote a candidate rule awaiting the owner's approval, one notice per candidate |
| `people-digest-failed` | `chatId`, `title` or null, `agent` by its label, `messages` (the count left unmarked), `cause` | a room's person-book batch failed its last try; its messages are read again at the next run |
| `people-sort-failed` | `work`, `heading` (the fixed heading's name, or null), `person`, `chatId` (or null for the common part), `title` or null, `cause` | the sort failed a second time at the same work on the same material, which is skipped until that material changes |

The sort's `work` is what failed and what is skipped: `cut` (cutting a book, and that person's room is skipped),
`group` (sorting a group of pieces, and that person's room is skipped), `sheet` (writing a room sheet over the
ceiling again, which is not written again, so that person's room sheet is not attached meanwhile), `trim-stretch`
(walking a stretch of a fixed heading, which goes back into the heading unwalked while the trim goes on past it)
and `trim-heading` (a fixed heading still past its ceiling after its walk, left as it stands until its text
changes).

## Codes the brain answers with

### Memory refusals

Every refusal of the memory routes carries a code and an English sentence. The codes with parameters:

| Code | Parameters | When |
|---|---|---|
| `no-agent` | | the turn names no agent |
| `no-room` | | a room operation names no room |
| `why-missing` | | a row taken down or raised with no reason |
| `action-missing` | | a row filed with no action |
| `state-missing` | | a state moved on with no text of where it stands |
| `situation-unpaired` | | a situation row with neither a condition nor an event |
| `situation-unfinished` | `handle`, `slug` | a situation row put to done; `slug` is what takes it down instead |
| `event-unknown` | `given`, `kinds` | an event outside the vocabulary |
| `event-takes-no-item` | `kind`, `given` | an item on an event that names none |
| `event-bad-item` | `given` | an event whose item is no item number |
| `usage` | `verb` | a request the route cannot read |
| `memory-refused` | | every other refusal, its sentence saying why |

A listing line marks a global row with `global-scope` (`reach`) and a handed-over strand with `from-item`
(`item`); the reading writes them in English, and a row object carries them for the console to render.

### Fragment refusals

| Code | When |
|---|---|
| `no-desk` | the desk named is not open |
| `no-names` | the desk was opened without the names the checks read |
| `out-of-place` | a path that is not relative, is no piece of a brain library, is outside the places the run may write, or is in another agent's memory |
| `no-body` | a write for a piece whose body is not on disk |
| `unnamed-slots` | a day bracket that leaves its slot names out |
| `title-shape` | a title the shape check refuses, with the reason |
| `keywords` | keywords carrying a common word (「Keywords」 in `fragments.md`), outside a person's address piece |
| `repeats` | a new piece whose matter a reachable row holds, the sentence naming those rows |
| `nothing-stands` | a remove, merge or split naming a path with no file and no row |
| `bad-merge` | a merge naming no piece to fold into the kept one |
| `bad-split` | a split naming fewer pieces than a cut makes |
| `body-stands` | a hand remove for a piece whose file still stands |
| `row-refused` | a hand write the row checks refuse, with `reasons` |

### Document, digest and agent refusals

| Code | Parameters | When |
|---|---|---|
| `brain-library` | `kind` | a shipped-library reconcile names a kind the brain writes itself |
| `out-of-room` | `slugs` | a document read names a slug the room may not be offered |
| `not-owner` | none | a room-rule set whose message the receiver does not confirm as the owner's |
| `one-to-one` | none | a person-book run asked for a room the receiver's roads call a one-to-one |
| `no-memory` | `agent` | a person-book run asked for an agent that keeps no long-term memory |
| `not-answered` | `agent` | a person-book run asked for a room the agent does not answer |
| `no-brain` | none | an editor finds the live brain folder is no git repository and makes none |
| `too-few-batches` | `needed`, `given` | a reform whose folded messages do not fit the batches it was given |
| `brain-root` | none | an agent reset or put-back names the agent whose folder is the brain root |
| `not-theirs` | none | a watch cancelled from a room that neither set it nor is the owner's |
| `no-watch` | none | a watch cancel naming no watch |
| `ended` | none | a watch cancel naming a watch that has already ended |
| `no-owner` | none | a watch cancel while the configuration names no owner, so the owner's room is unknown |
| `reminder-refused` | none | a reminder set the checks refuse, its sentence saying why |
| `no-reminder` | none | a reminder cancel naming no reminder |
