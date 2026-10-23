# The brain's design

This document states the shape of the brain: what it holds, whose it is, how a memory is written, kept,
weakened, superseded and read back, and what the brain does on its own. `docs/boundary-api.md` states the
routes, events, tools and codes that realise this shape. The decisions here follow the owner's restructuring
record (archive document 301), its upgrade record (archive document 303, the brain section) and the memory
study those rest on (archive document 302).

## What the brain is

A brain is one agent's long-term memory. The service keeps many brains, each owned by an account and
belonging to one agent of that account, and serves them over HTTP. Everything an agent reads about the
world beside the conversation in front of it is a memory in its brain: episodes, facts, tips, rules,
situation rules, the sheets of the people it knows, its knowledge, its persona and its lore. Nothing an
agent reads is a file.

- **One table of memories, with a type on each row.** Episodes, facts, situation rules, persona pieces and
  the rest are values of `type`, never tables of their own. No table stands for one feature.
- **A fact is one row, and a change is a new row.** A row is never deleted and never edited in place. A
  correction writes a new version that supersedes the old one, and a fact that no longer holds is closed and
  hidden, with the moment it stopped holding. Who changed what, and when, is read off the versions.
- **The brain writes its own memories.** The recollector, the part that judges what a conversation adds to
  what is already known and that consolidates episodes into facts, is the brain's. It runs on model
  sessions the agency supplies; what to write, when and how is decided here.
- **Per turn it hands out a short block,** chosen by relevance, importance and strength under a token
  budget, never the whole store, and never anything that goes into a system prompt. The persona is the one
  exception: it is rendered on demand as the same bytes every time, so a prompt built on it keeps its cache.

The common wire format and the API design rules are bmo-chat's `docs/conventions.md`; the brain keeps them.

## Whose a brain is

- **A brain is keyed by its account and its agent.** `accountKey` is the account's opaque key as the caller
  gives it; `agentKey` is the agent's opaque key as the agency registers it. The brain reads
  neither:
  it only separates by them. Nothing is held outside a brain.
- **One account sees its own brains alone.** A caller names the brain it reads or writes in the path, and
  every member of that brain carries the brain's id. Two agents of one account have two brains that share
  nothing; two accounts never see each other's.
- **The brains stand in one store,** the service's own PostgreSQL database, every row keyed by its brain,
  so one set of migrations serves every brain. The unit a brain moves in, is backed up in and is let go in
  is the brain itself, and its portable form is the brain file (「Export and injection」).

## The three tables

### Subjects

A subject is something a memory can be about: a person, a room, a code unit, a process, a document, a
place. It has no fixed shape.

- `id`, the brain's; `type`, the caller's word for what it is; `key`, the caller's identifier for it, unique
  within its brain and type; `description`, one short line; `createdAt`.
- **A subject is found by its key alone,** never by a name. A person's key is the account or sender id the
  caller already holds; a room's is the room's id. Because a name never finds or makes a subject, one person
  never splits into several subjects. A subject's names, nicknames and forms of address are memories of the
  reserved type `name` linked to it.
- **A subject is an index the agent reads, not a link into another unit's data.** A memory about archive
  document 301 names a subject of type `document` and key `301`; the agent reads that and goes to the
  archive itself. The brain keeps no foreign reference.
- **A memory's grounds are subjects too.** What a memory rests on, the episode it came from, the document it
  was read in, the person who said it, is linked as a subject like anything else it is about. No column for
  evidence and no link table apart from the subject links exists.
- **Subjects carry no versions.** What changes about a subject is a memory about it.

### Layers

A layer is a heading in the brain's table of contents: a topic, a category, a body of lore, a room's
rulebook. `id`, `name` (unique within the brain), `description`, `position`, `createdAt`.

- **One layer holds many memories; a memory stands in at most one layer.** A memory with no layer is
  unfiled and found through its subjects and its words alone.
- **Layers carry no versions** either.

### Memories

One row is one thing the agent remembers, in one version.

| Field | Means |
|---|---|
| `id` | the brain's |
| `key` | the caller's optional identifier, unique among the live rows of a brain; a re-injection of the same key supersedes the row that carries it |
| `type` | what the row is (「Types」) |
| `text` | the memory itself, as it is handed out |
| `layerId` | the layer it stands in, or null |
| `subjects` | the subjects it is about, any number |
| `status` | `active`, `pending`, `done`, `superseded` or `invalidated` (「Versions」) |
| `lineageId` | the id of the first version in this row's line; a first version's own id |
| `supersededBy` | the id of the version that replaced this one, or null |
| `validFrom`, `validTo` | the span the memory holds for; `validTo` null while it holds |
| `importance` | the recollector's weight at the time of writing, 1 to 10 |
| `hits`, `lastHitAt` | how often, and when last, a reading handed it out |
| `lastConfirmedAt` | when the memory was last confirmed by a source apart from the reading |
| `strength` | its storage strength, raised only by spaced confirmations (「Strength and decay」) |
| `actorName` | who wrote this version: the recollector's run, a tool, a person at the screen |
| `labels` | the caller's own metadata, a map of strings |
| `createdAt` | when this version was written |

### Types

`type` is a word the recollector writes freely. A few are reserved because code depends on their meaning;
every other value is whatever the recollector called it, and the brain lists the types in use with a count
each, so a meaning does not split into two spellings.

| Reserved type | Means |
|---|---|
| `episode` | what happened, as it was said; the raw material consolidation reads; never deleted |
| `fact` | a thing that is so, consolidated or stated outright |
| `rumor` | something told second-hand or inferred; handed out as "heard", never overriding a fact |
| `situation` | a rule for a situation: when, and what to do then |
| `rule` | a rule granted in a room, linked to the room's subject |
| `name` | what a subject is called |
| `sheet` | a person's standing sheet, linked to the person and, for a room's sheet, to the room |
| `persona` | a piece of the agent's persona, rendered into its system prompt |
| `lore` | a piece of the agent's lore, read as reference |
| `knowledge` | a piece of reference knowledge |

## Versions

- **Every row is a version.** A new memory is its own first version: `lineageId` equals its id, `validFrom`
  is the moment it is written (or the moment the writer names), `validTo` is null.
- **A correction is a new row in the same lineage.** The old row's `supersededBy` names the new one, its
  `status` becomes `superseded` and its `validTo` the new row's `validFrom`. The new row carries the lineage
  id. A reading hands out the newest row of a lineage that is `active`, and says for a row whose predecessor
  was superseded in the last thirty days what the previous value was and until when.
- **A fact that stopped holding is invalidated:** `status` `invalidated`, `validTo` the moment it stopped.
  Nothing points at a replacement. A promise kept or an errand finished is `done`, with `validTo` the moment
  it was done and the text it ended on as a new version where the writer gives one.
- **`pending` is one thing only:** a memory waiting for a person's approval. The recollector never parks a
  memory as `pending` because it has one source (「Writing」), and it does not offer common knowledge for
  approval either: what members bring to the shared knowledge the agent collects and keeps on its own, as a
  fact or a rumor by the rule below. A `pending` row is one a person asked to be approved.
- **No row is deleted.** Letting a brain go, by its owner, is the one removal, and it removes the brain
  whole.
  Within a brain, hiding is a status.
- **Reading never changes a row's words or status.** A reading records a hit; only a contradiction, a
  confirmation, a correction or a person's act writes a version.

## Writing

The recollector is handed what a conversation said, who said it and in which room, and decides what the
brain learns from it. It writes in two moments.

- **Episodes at once.** What was said is kept as `episode` rows, each linked to its speakers and its room,
  with an `importance` the recollector weighs from how surprising and how consequential it is. Episodes are
  the material consolidation reads and are never consolidated away.
- **The write judgement for every fact.** Before a fact is written, the recollector reads the live memories
  of the same subjects most like it, at most ten, and judges the candidate as one of four: a duplicate
  (nothing is written; the existing row's `lastConfirmedAt` and strength move), a strengthening (a new
  version of the existing lineage with the fuller text), a contradiction (the old row is closed and the new
  one written, 「Versions」) or new (a first version).
- **A single source is enough when it is the person's own word.** What someone says about themselves, and
  what the owner instructs, is written as a `fact` at once and closes the fact it contradicts, with no
  second
  source waited for. What is told about someone else, and what the recollector infers, is written as a
  `rumor`: handed out as something heard, standing beside the facts and never closing one. A rumor becomes a
  fact when the person confirms it or a second source agrees.
- **Who writes is recorded** on the version as `actorName`: the recollector's run, the tool a session
  used, or the person who changed it at the screen.

## Consolidation

Consolidation is the brain's own pass, outside any answer: it reads episodes not yet consolidated, together
with the live facts about the same subjects, and writes semantic facts, reflections and corrections from
them. It takes the episodes with the highest importance first.

- **Two triggers, and no others:** the sum of the importance of a brain's unconsolidated episodes reaching
  the brain's threshold, and a session being folded or handed off. There is no nightly pass, no pass every N
  turns and no periodic mode: the importance trigger is what stands in for one, the hand-off trigger is the
  other mode.
- **The fold is told to the brain** by the caller that folds the session, on the consolidation route; the
  importance trigger is the brain's own clock.
- **It runs as a model session the brain asks the agency for,** apart, with the recollector's prompt
  document and no tools but the brain's own; the brain reads what it writes back and ends the session. The
  agency is the one unit the brain calls, and it calls it for sessions alone. Which provider and model a
  brain's recollector runs on is the brain's settings.
- **What it writes passes the write judgement** like any fact.

## Strength and decay

Two strengths stand on every memory, after the distinction between how well something is stored and how
easily it comes to mind.

- **Storage strength** is the `strength` column. It rises only on a confirmation spaced from the last one:
  a confirmation within the same session adds nothing; the longer the gap, the more it adds. It never
  falls.
- **Retrieval strength** is not stored: it is computed at reading time from the row's hit history, as the
  logarithm of the sum over its hits of the time since each hit raised to a negative power, so a memory
  handed out often and recently comes to mind easily and one not read for long fades.
- **A weak memory is not removed.** Its retrieval strength drops it out of the automatic block; an explicit
  read by key, subject, type or words still answers it, and a hit there revives it.
- **The numbers are settings,** with placeholders until the owner's own conversation logs have fitted them:
  the importance threshold of consolidation, the decay exponent, the spacing curve of storage strength, the
  weights of the recall score, the per-subject cap and the token budget. The brain reads them from its
  configuration and names each in its health answer.

## Recall

Recall is what a turn is handed. The caller sends the brain the turn: the room's subject, the subjects who
spoke, the words of the turn, the session's key and the moment it opened, and the budget. The brain answers
a block and a list of what it chose.

- **Two parts within one budget** of about one and a half to two thousand tokens: the fixed core, the few
  memories that stand on every turn of that room (its situation rules, the standing sheets of everyone who
  spoke, the room's rules), and the searched part, chosen by score.
- **The score** is a weighted sum of relevance to the turn's words, importance and recency, recency being
  the retrieval strength computed from the row's hits (「Strength and decay」), each normalised; a memory of a
  subject the turn names scores higher; no subject fills more than its cap of
  three or four rows; a diversity pass keeps the block from being one topic.
- **Only live rows, with their span.** A row goes out with the dates it holds for; a row whose predecessor
  was superseded lately goes out with one line saying what the previous value was and until when.
- **What this session already knows is left out.** A memory written after the session opened is not handed
  back to that session: the session saw the conversation it came from. Once the session is folded or handed
  off, the row may attach again.
- **A memory handed out once to a session is marked, not repeated,** while its version is unchanged: the
  brain keeps, per session key, which versions it has handed out, answers the mark in place of the text, and
  clears the marks when the caller reports the fold.
- **A `rumor` is handed out as heard,** marked so in the block. A `pending` row is never handed out.
- **Every hand-out is a hit** on the rows chosen, which the brain records itself.

The caller that selects a turn's reference asks recall and decides what of it the turn is handed; the block
is the brain's offer, the caller's is the last word.

## Persona and lore

- **The persona is memories of type `persona`,** each a piece in a layer, ordered by layer position and
  then by position within the layer. The brain renders a persona on demand as one text, the pieces in that
  fixed order, nothing added that changes between calls: no date, no counter, no hit. The rendering is the
  same bytes until a persona piece is actually changed, and a change is a new version like any other.
- **Placeholders stay placeholders.** A persona piece may carry a placeholder for a value the deployment
  supplies; the brain renders it as it stands, and the caller that builds the prompt fills it.
- **The lore is memories of type `lore`:** the index is layers, the people and places are subjects, and each
  piece is a row, handed out through recall like any reference.
- **Each agent's persona is in its own brain.** The brain knows no persona repository: a persona reaches a
  brain by injection and leaves it by export.

## Export and injection

- **The brain file** is a brain, or part of one, as a SQLite database holding the three tables and the
  memory-subject links in the brain's own shape, with the ids local to the file. It carries the version of
  the shape it was written in.
- **Export** writes a brain file from a brain: the whole brain, or the layers named, with the subjects and
  the live rows those layers hold.
- **Injection** reads a brain file into a brain it names. It overwrites nothing: a row whose `key` matches a
  live row supersedes it as a new version, a subject found by its type and key is reused, a layer found by
  its name is reused, and the rest are added. The brain does not ask where the file came from.
- **Keeping a content repository's backup current** is the owner's act, or an agent's at the owner's word:
  export the layers, put the file in the repository. The brain never reads a repository.

## Rooms, people and rules

- **A room is a subject** of type `room`, keyed by the room's id. What an agent knows about a room is
  memories
  linked to it.
- **A room's rules are memories of type `rule`** linked to the room's subject, each with its `key` the
  caller's
  rule id and its grantee among its subjects. Revoking a rule invalidates it.
- **A person is a subject** of type `person`, keyed by the id the caller knows them by. Their standing sheet
  is a `sheet` row linked to them, and a room's sheet of them is a `sheet` row linked to them and the room.
  The recollector writes and rewrites sheets as versions.
- **What a user said in one room stays in that room's memories** unless it is theirs: the reading narrows
  by the room's subject, and the one carry-over, a user's own words into their private room, is the caller's
  to ask for by naming both rooms.

## What the brain acts on by itself

- **Its clock:** the importance trigger of consolidation, the pruning of its event window, and nothing else.
- **Its one outgoing call:** a model session from the agency, for the recollector.

Reminders, waits on events and watches are not the brain's: a wait is a tool run outside the brain, which
runs the thing waited for, holds it, and wakes the agent with the memory it was told to carry. The brain
keeps the memory; the wait keeps itself.

## What the brain stops holding

These members are retired once their readers have moved, each to the place
that owns it: fragments and libraries (now memories of the reference types, in layers), notes (now
`episode` rows), watches and reminders (now the wait tool outside the brain), session loads and session
notes (the agency's session records) and the room-rules routes (now `rule` memories). Until a reader has
moved,
the old route keeps answering as its document states; a reader moves by a change of its own.

## The data move

The store moves into this shape once, whole, with nothing lost; this table goes once the move is done:

| Today | Becomes |
|---|---|
| a memory row's `scope` | the brain, by the agent the scope names, and the room's subject where the scope names a room |
| a memory row's `kind` (`promise`, `check`, `followup`, `situation`, `state`) | the same word as `type` |
| `standing` | `active`; `done` → `done` with `validTo`; `retired` and `off` → `invalidated`; `candidate` → `pending` |
| `conditions` and `labels` | `labels`, kept whole, with the condition's words folded into the text where the row is a `situation` |
| a fragment of kind `person` | a `sheet` row linked to the person's subject, and to the room's where the key names one |
| a fragment of kind `library-knowledge` | a `knowledge` row in the layer named for its library |
| a fragment of kind `room-rule` | a `rule` row linked to the room's subject |
| a fragment's `hitCount`, `lastHitAt` | `hits`, `lastHitAt` |
| a note | an `episode` row |
| the brain's markdown tree under the data root: its people, knowledge, room and agent documents | memories of the matching types, each document once; a document already held as a fragment moves as that fragment |
| a reminder or a watch | left where it stands until the wait tool outside the brain takes it over |
| a room rule record | a `rule` row |
| session loads and session notes | left for the agency's session records |

## The screen

The brain's console bundle is one screen for a brain: its layers, subjects and memories, each row with its
versions and its status, editable in place. An edit writes a version; taking a fact down sets its status;
neither deletes. Into it merge what the document viewer shows of a brain today, the person sheets, the room
rules, the knowledge, the persona and the lore, and what the memory screen shows, the promises, the checks,
the states and the situation rules. Switching a reference piece on or off for a room is its status. A
repository's documents, the rulebooks, the docs and the agents' instructions, are not here: a checkout is
read in the review screen, and the composed system prompt is the agency's to show. The screen starts from
the structure, not from a folder tree:

- **A mind map:** one subject at the centre, its memories around it, and the other subjects those memories
  reach, coloured by layer.
- **A bubble chart:** one circle per layer or subject, sized by its memories, shaded by how warm they are
  (retrieval strength), so where memory gathers and where it cools is seen at once.
- **A version timeline:** one lineage on a time axis, each version at its `validFrom`.
- **A list** is one more view, with the passage questions, the jump and the translated reading the document
  viewer has today.

The screen's design is shown to the owner before it is built.

## The order of building

1. The three tables, the brain member, versions, the reference and recall routes, the type list, export and
   injection, the data move, the client in both languages and the documents, with the old routes still
   answering.
2. The recollector: episodes, the write judgement, consolidation on its two triggers, strength, through
   sessions asked of the agency.
3. The readers move: the reference selection onto recall, the room rules and sheets onto the subject routes,
   the session records onto the agency's own; then the old routes and tables go. Once the reference
   selection reads this structure, its training data is rebuilt on it and its decision model retrained, at
   the owner's word only, as that unit's own item.
4. The persona and lore of each agent injected from their content repositories' exports, and the agency's
   prompt composition onto the persona rendering.
5. The screen.
