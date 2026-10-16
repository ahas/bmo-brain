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

The service reads its configuration at start and holds it for its life:

| Setting | What it is |
|---|---|
| listen address | where the service answers |
| token | the bearer token every caller presents |
| database | the brain's own database: the tables below and the brain's settings table, nothing else |
| data root | where the editing locks, the fragment desks and the failure logs live |
| brain folder | the brain root, a git repository holding every agent's documents |
| chat agent's key | the registration key of the chat agent, whose brain folder is the brain root |
| owner's names | the names the owner goes by in the rooms, counted among the common words ([fragments](fragments.md)) |
| service unit | the unit the service runs as: a failure notice of a run with no unit of its own names this unit's journal, and a run under a launched unit names that unit's |
| agency, receiver, gate | each unit's base address and the token it is called with |
| policy | the files under `policy/`, the service's and each crate's own: the common words, the fragment titles' cues, and the memory, digest and person-book numbers |

Paths the brain writes into an answer are relative to the brain folder or to the data root, never absolute.

## What the brain owns

- **Its tables:** `memories`, `fragments`, `notes`, `watches`, `reminders` and `session_people`. Every row of
  the first five names its agent in an `agent` column; an insert names its agent, and the column has no
  default.
- **Its settings table,** of the key-value shape, for its own keys only:
  - `digest_paused_until` and `digest_pause_noticed`, the owner's pause of both digests and whether the pause
    has been told;
  - the person-book sort's failure counters, one per fingerprint of a failed group, cut or sheet;
  - the day a book starts reading, one key per agent and room;
  - the brain holds, one key `brain_hold:<run>` per job run holding the editors off;
  - the chat-session summaries, one key `<agent>_pending_summary` per agent (「Chat-session summaries」).
- **Its migrations:** the brain's own migrations make the tables and the settings table, each in the shape and
  with the indexes the archive holds it in, so the archive's rows move across as they stand; the one difference
  is the `agent` column's, which has no default.
- **Its folder:** the brain repository and its documents, and under the data root the editing locks with their
  ticket queues and fences, the fragment desks, and the failure logs `digest-failures.log` and
  `people-digest-failures.log`.
- **Its prompts:** the recollectors' task prompts and the rules they read are this repository's.
- **Its tools:** the tools bound to the brain are this repository's, each under `tools/` with its own
  `tool.toml` manifest, which the tool host reads as it reads any tool's: `tools/memory`, the memory rows an
  agent keeps and the note it keeps for a room's digest, and `tools/fragment`, an editor's fragment desk. Each
  answers through the routes below, over HTTP.

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
| 409 | `conflict` | the request contradicts the row's state; the answer carries the refusal code as well |
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
| `GET /health` | `ready` (boolean) and `reason` (string or null) |

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
| `POST /memory/events` | `agent`, `chatId`, `events` (each `kind`, `item` or null) | `rows`: the rows the events wake, by the event alone. An errand's first meeting is written on it. |

The console's routes:

| Route | Body | Answer |
|---|---|---|
| `GET /memories` | query `agent` and `chat`, both optional | `memories`: every row, newest first |
| `POST /memories/active` | `slug`, `active`, `why` (required to take a row down) | `ok` and the row as it now stands |

A row carries `id`, `agent`, `slug`, `type`, `chatId`, `reach`, `condition`, `action`, `forUser`, `messageId`,
`todoId`, `setBy`, `setAt`, `active`, `lastHitAt`, `hits`, `doneAt`, `doneText`, `downReason`, `downAt`,
`candidateWhy`, `candidateAt`, `event` (`kind`, `item`), `eventMetAt` and `movedAt`. `POST /memories/active`
answers 400 when `slug` or `active` is missing or when a row taken down carries no `why`, 404 for no such row,
and 409 with the code `missing`, `other-room`, `already-up` or `already-down`.

### Notes

| Route | Body | Answer |
|---|---|---|
| `POST /notes/add` | as above | a reading |
| `POST /notes/list` | `agent`, `chatId` or null | `notes`: `id`, `agent`, `chatId`, `author`, `text`, `global`, `messageId`, `at`, the notes still waiting for the digest |

### Watches and reminders

| Route | Body | Answer |
|---|---|---|
| `POST /watches/list` | `agent`, `by`, `all` | `watches`: `id`, `agent`, `by`, `name`, `note`, `check`, `every`, `within`, `at`, `item`, `state` (`waiting`, `fired`, `lapsed`, `broken`, `cancelled`), `setAt`, `endedAt` |
| `POST /watches/cancel` | `agent`, `id`, `by` | `ok`; refused `not-theirs` where `by` is neither the room that set it nor the owner's room |
| `POST /watches/launch` | `id` | `launched` (boolean) |
| `POST /watches/claim` | `id`, `run` | `standing` (boolean): whether the watch still stands for this run |
| `POST /watches/end` | `id`, `run`, `outcome` (`fired`, `lapsed`, `broken`), `printed` | `ok`; the memo is queued once, whichever run ends it first |
| `POST /reminders/list` | `agent`, `chatId` | `reminders`: `id`, `agent`, `chatId`, `text`, `at`, `repeat` |

### Fragments

The fragment tool's routes, one per tool schema. Each body is `desk` (the desk the run's session record names)
plus the tool's arguments; each answer is a reading or a refusal (「Fragment refusals」 below). A desk that is
not open is refused `no-desk`.

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
| `POST /fragments/hits` | `agent`, `chatId`, `attached` (each `kind`, `chatId`, `owner`, `slug`) | `ok`; each attached row's count and last hit are written |

The console's and the hand writer's routes:

| Route | Body | Answer |
|---|---|---|
| `GET /fragments` | query `kind`, `chat`, `person`, `item`, `text`, `down`, all optional | `fragments` |
| `POST /fragments/active` | `kind`, `slug`, `active`, `chatId` or null, `owner` or null | `ok` and the row; 404 for no such row |
| `POST /fragments/write` | `path`, `title`, `keywords`, `items`, `shared`, `forEveryAgent` | a reading or a fragment refusal; the index files are drawn after it |
| `POST /fragments/remove` | `path` | a reading; refused `body-stands` where the file still stands |
| `POST /fragments/check` | `kind` or null | `drift` and `titles` (「The drift check」 in `fragments.md`) |
| `POST /fragments/sync` | none | a reading of what the reconcile changed |

A fragment carries `kind`, `chatId`, `owner`, `agent`, `allAgents`, `slug`, `title`, `path`, `scope`,
`position`, `active`, `lastHitAt`, `hits`, `setBy`, `setAt`, `hand`, `handAt`, `keywords`, `items`,
`writtenOn`, `confirmedOn`, `status` and `bodyHash`. `kind` is one of the brain's own `room-rule`,
`library-knowledge` and `person-book`, or a kind its owner hands in (「Shipped libraries」 below).

### Shipped libraries

| Route | Body | Answer |
|---|---|---|
| `POST /libraries/reconcile` | `kind`, `label`, `entries` (each `slug`, `title`, `path`, `position`, `scope` or null, `items`, `writtenOn`, `confirmedOn`, `bodyHash`) | a reading of the rows raised, changed and taken down |

A shipped library is not the brain's. Its owner reads its own index and bodies and hands the entries in whole,
with the word a screen calls its kind by; the brain brings the kind's rows to them (「The shipped libraries」 in
`fragments.md`). The brain names no shipped kind: it takes any kind but its own, and a kind the brain writes
itself is refused `brain-library`.

### Brain documents

| Route | Body | Answer |
|---|---|---|
| `POST /knowledge/read` | `agent`, `chatId`, `slugs` | `documents`: `slug`, `path`, `text`, `version`; a slug outside the room's own and the shared chunks is refused `out-of-room` |
| `POST /people/read` | `agent`, `chatId`, `senders`, `slugs` | `sheets` (per sender, the common sheet and the room sheet where the room keeps one and the file is under its ceiling) and `pieces`, each `path`, `text`, `version`, `owner`; a slug of another shape, a room piece of another room or a common piece of somebody who never spoke in the room is refused `out-of-room` |
| `POST /room-rules/read` | `agent`, `chatId` | `rules`: `id`, `subject`, `text`, one per rule chunk of the room |
| `POST /room-rules/set` | `agent`, `chatId`, `messageId`, `rules` (each `id` or null, `subject`, `text`), `removed` (ids) | a reading; refused `not-owner` unless the receiver confirms the message's sender is the room's owner |

`version` is the body's hash; a session holds a document by its path and version.

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
| `POST /digest/run` | `chatId` or null | `outcome`: `launched`, `running`, `nothing` (no note waits), `resting`, `paused` |
| `POST /digest/now` | `agent`, `chatId` | `outcome`: `resting` (a rest is on), `nothing` (no note waits), `running` with `unit`, `launched` with `unit` and `notes`, or `refused` with `why`, the unit launch's refusal |
| `POST /digest/people` | `chatId`, `agent`, `all`, `split` with `person` and `spans`, `sheets`, `reform` with `batches`, `sort` with `person`, `all` or `status` and `groups`, `since` (a day, `none`, or absent to read it), `forgetRead` with `chatId` or `all` | `outcome` and a reading |
| `POST /digest/pause` | `set`: `status`, `off`, `on`, or a length (`2h`, `90m`, `30s`, a bare number of minutes) | `pausedUntil` (time, or null for none, or `forever`) |
| `POST /people/before-fold` | `chatId`, `agent` | `outcome`: `launched`, `running` or `skipped` (the room keeps no book) |

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
| `POST /rooms/removed` | `chatId`, `preview` (boolean) | `took`: the count of the room's rows per table, its settings keys (each agent's day its book of the room starts reading) and the room folders per agent; with `preview` false they are removed and the brain repository committed |

### Agents forgetting

| Route | Body | Answer |
|---|---|---|
| `POST /agents/reset` | `agent`, `preview` (boolean) | `took`: the count of the agent's rows per table and whether its brain folder stands; with `preview` false its folder is fenced, its editing lock taken ahead of every waiter, its open recollector runs stopped and what they left unwritten put back, its rows and folder removed and the brain repository committed, the folder fenced again and the lock let go; `couldNot` lists what it could not do |
| `POST /agents/removed` | `agent` | `ok`; the agent's editing lock, its ticket queue and its fence are removed |

The chat agent's folder is the brain root, so `POST /agents/reset` refuses it `brain-root`.

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
| recollector session start | `/runs/start` | `workKey`, `agent`, `unit`, `prompt`, `tools`, `workdir`, `desk`, `registration` or null | `run` |
| recollector session wait | `/runs/wait` | `run` | `outcome` (`answered`, `failed`, `overran`, `stopped` or `limit`), `answer` (or null), `filesWritten`, `cost` (or null), `size` (or null) |
| recollector session stop | `/runs/stop` | `run` | `ok` |
| overrun ask | `/runs/overrun` | `run`, `unit`, `capSeconds` | `keep` (boolean) and `until` (or null) |
| run open | `/runs/open` | `run` | `open` (boolean): whether a job's run still stands |
| work key cap and reasoning | `/work-keys/read` | `workKey`, `agent` | `capSeconds`, `reasoning` (or null) |
| item states | `/items/states` | `items` | `items`: each item's `item` and `finished` (boolean), for a fragment's status and a trim's stretch |
| unit launch | `/units/launch` | `unit`, `command` (its words, in order) | `outcome`: `launched`, `running` or `refused`, and `why` (or null) |
| quiet instruction | `/instructions/quiet` | `agent`, `chatId`, `text`, `kind` or null | `ok` |
| document reload notice | `/documents/reload` | `agent`, `paths`, `rooms` | `ok` |

The recollector runs are the agency's sessions; the brain decides what to recollect, writes the prompt, opens
the desk, holds the lock and judges the result. Everything about the model run itself is the agency's: the
logins it goes out on, the run's usage and cache events, and a usage limit. A session that reaches a limit
ends with the outcome `limit`; the agency has rested its login and told the owner where that rest silenced the
last open login, as for every session it runs. The brain writes no rest, and the run stops without a retry.

### The receiver

A message is answered as `chatId`, `messageId`, `senderId` (or null), `senderName`, `sentAt`, `text` and
`replyToMessageId` (or null).

| Operation | Path | Sends | Answers |
|---|---|---|---|
| unread messages | `/messages/unread` | `agent`, `chatId`, `limit`, `since` or null | `messages` past the agent's read marks, oldest first |
| read messages | `/messages/read` | `agent`, `chatId`, `from`, `limit` | `messages` the agent has marked, after `from`, oldest first |
| keep read marks | `/read-marks/keep` | `agent`, `chatId`, `messageIds` | `ok` |
| forget read marks | `/read-marks/forget` | `agent`, `chatId` or null for every room | `forgotten`: the count of marks let go |
| room titles | `/rooms/titles` | none | `rooms`: each room's `chatId`, `title` (or null) and `senders`, the people that have stood as a sender in it |
| room answerers | `/rooms/answerers` | `chatId` or null | `rooms`: each room's `chatId` and `agents`, the agents answering it |
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
| `out-of-place` | a path that is no piece of a brain library, or outside the run's folder |
| `no-body` | a write for a piece whose body is not on disk |
| `unnamed-slots` | a day bracket that leaves its slot names out |
| `title-shape` | a title the shape check refuses, with the reason |
| `repeats` | a new piece whose matter a reachable row holds, with those rows' paths |
| `body-stands` | a hand remove for a piece whose file still stands |

### Document, digest and agent refusals

| Code | Parameters | When |
|---|---|---|
| `brain-library` | `kind` | a shipped-library reconcile names a kind the brain writes itself |
| `out-of-room` | `slugs` | a document read names a slug the room may not be offered |
| `not-owner` | none | a room-rule set whose message the receiver does not confirm as the owner's |
| `one-to-one` | none | a person-book run asked for a room the receiver's roads call a one-to-one |
| `no-brain` | none | an editor finds the live brain folder is no git repository and makes none |
| `too-few-batches` | `needed`, `given` | a reform whose folded messages do not fit the batches it was given |
| `brain-root` | none | an agent reset names the agent whose folder is the brain root |
| `not-theirs` | none | a watch cancelled from a room that neither set it nor is the owner's |
