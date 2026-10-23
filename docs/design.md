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
situation rules, its knowledge, its persona and its lore. Nothing an agent reads is a file, and nothing it
reads about a person is a sheet kept apart: a person's standing sheet is rendered from the facts about them.

- **One table of memories, with a type on each row.** Episodes, facts, situation rules, persona pieces and
  the rest are values of `type`, never tables of their own. No table stands for one feature.
- **A fact is one row, and a change is a new row.** A row is never deleted and never edited in place. A
  correction writes a new version that supersedes the old one, and a fact that no longer holds is closed and
  hidden, with the moment it stopped holding. Who changed what, and when, is read off the versions.
- **The agent writes its memories as it reads.** The chat agent, the supervisor of its rooms, reads every
  message of them anyway, so it decides at that moment what the brain learns and writes it through the
  brain's tools; the discipline of a turn is a text the brain ships and the agent's system prompt carries. The brain supplies the
  judgement's material, the live memories like a candidate, and does the judgement's mechanics: a confirmation,
  a version, a closed fact, a new row, in one call. The brain runs no model and calls no unit.
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
| `importance` | the writer's weight at the time of writing, 1 to 10 |
| `hits`, `lastHitAt` | how often, and when last, a reading handed it out |
| `lastConfirmedAt` | when the memory was last confirmed by a source apart from the reading |
| `strength` | its storage strength, raised only by spaced confirmations (「Strength and decay」) |
| `actorName` | who wrote this version: the agent's session, a tool, a person at the screen |
| `labels` | the caller's own metadata, a map of strings, but for the one label the brain reads (「Types」) |
| `createdAt` | when this version was written |

### Types

`type` is a word the writer writes freely. A few are reserved because code depends on their meaning;
every other value is whatever the writer called it, and the brain lists the types in use with a count
each, so a meaning does not split into two spellings.

No type is called `sheet`. A person's standing sheet is a rendering of the facts about them (「A person is
a subject」), never a row, and a write that names `sheet` as its type is refused.

| Reserved type | Means |
|---|---|
| `episode` | what happened, as it was said; the raw material consolidation reads; never deleted |
| `fact` | a thing that is so, consolidated or stated outright |
| `rumor` | something told second-hand or inferred; handed out as "heard", never overriding a fact |
| `situation` | a rule for a situation: when, and what to do then |
| `rule` | a rule granted in a room, linked to the room's subject |
| `name` | what a subject is called |
| `persona` | a piece of the agent's persona, rendered into its system prompt |
| `lore` | a piece of the agent's lore, read as reference |
| `knowledge` | a piece of reference knowledge |

One label is reserved for the same reason: `always=true` on a `rule` row marks a rule that stands on every
turn of its room (「Rooms, people and rules」), and the rule without the label, or with another value, does
not. The brain reads it on a `rule` alone; on any other row it is a label like the caller's own. Every other
label is the caller's.

## Versions

- **Every row is a version.** A new memory is its own first version: `lineageId` equals its id, `validFrom`
  is the moment it is written (or the moment the writer names), `validTo` is null.
- **A correction is a new row in the same lineage.** The old row's `supersededBy` names the new one, its
  `status` becomes `superseded` and its `validTo` the new row's `validFrom`. The new row carries the lineage
  id. A reading hands out the newest row of a lineage that is `active`, and says for a row whose predecessor
  was superseded in the last thirty days what the previous value was and until when.
- **A merge is one new row written over several.** Rows about the same thing, each a lineage of its own,
  are folded into one: the new row starts a lineage, and every merged row becomes `superseded` with its
  `supersededBy` naming the new row and its `validTo` the new row's `validFrom`. The new row takes the most
  hits, the earliest confirmation and the highest strength among them, so a reading does not forget what
  the pieces had earned. Nothing is deleted: each merged row stays in its own lineage, pointing at the row
  that stands for it now.
- **A fact that stopped holding is invalidated:** `status` `invalidated`, `validTo` the moment it stopped.
  Nothing points at a replacement. A promise kept or an errand finished is `done`, with `validTo` the moment
  it was done and the text it ended on as a new version where the writer gives one. Whoever moves a row may
  say why, and the reason stays on the row it moved.
- **A closed lineage is restored by a new version,** never by reopening the closed row: the newest version
  of a `done` or `invalidated` lineage is followed by a new `active` version in the same lineage, its text
  the closed one's or the restorer's, and the closed row stays as it was, a record of the span it held for.
  A lineage that holds a live version already has nothing to restore.
- **`pending` is one thing only:** a memory waiting for a person's approval. The agent never parks a
  memory as `pending` because it has one source (「Writing」), and it does not offer common knowledge for
  approval either: what members bring to the shared knowledge the agent collects and keeps on its own, as a
  fact or a rumor by the rule below. A `pending` row is one a person asked to be approved.
- **No row is deleted.** Letting a brain go, by its owner, is the one removal, and it removes the brain
  whole.
  Within a brain, hiding is a status.
- **Reading never changes a row's words or status.** A reading records a hit; only a contradiction, a
  confirmation, a correction or a person's act writes a version.

## Writing

The agent, reading a message in its turn, decides what the brain learns from it and writes it then, through
the brain's tools. It writes in two moments.

- **Episodes at once.** What was said that may matter later is kept as `episode` rows, each linked to its
  speakers and its room, with an `importance` the agent weighs from how surprising and how consequential it
  is. Episodes are the material consolidation reads and are never consolidated away.
- **The write judgement for every fact.** Before a fact is written, the agent asks the brain for the live
  memories of the same subjects most like it, at most ten, and judges the candidate as one of four: a
  duplicate (nothing is written; the existing row's `lastConfirmedAt` and strength move), a strengthening (a
  new version of the existing lineage with the fuller text), a contradiction (the old row is closed and the new
  one written, 「Versions」) or new (a first version). The judgement is the agent's; the brain applies it whole
  in one call, so a judgement is never half applied.
- **A single source is enough when it is the person's own word.** What someone says about themselves, and
  what the owner instructs, is written as a `fact` at once and closes the fact it contradicts, with no second
  source waited for. What is told about someone else, and what the agent infers, is written as a `rumor`:
  handed out as something heard, standing beside the facts and never closing one. A rumor becomes a fact when
  the person confirms it or a second source agrees. The brain holds this rule: a learning whose source is
  hearsay or inference is written as a rumor whatever it was called, and closes nothing.
- **A fact is one row.** What the agent learns about a person is written one thing to a row, each through
  the write judgement: in English, dated by its `validFrom` (the moment it began holding: the moment of the
  message it was read in, or the date the speaker names), linked to the person, and linked to a room only
  where it holds in that room alone. Nothing gathers a person's facts into a row of its own; their sheet is
  rendered from these rows (「A person is a subject」).
- **A memory is written in English,** whatever language the room speaks; a short quotation in the original
  words stands only where the exact wording matters.
- **Who writes is recorded** on the version as `actorName`: the agent's session, the tool it used, or the
  person who changed it at the screen.

## Consolidation

Consolidation is the agent's pass outside any answer: it reads episodes not yet consolidated, together with
the live facts about the same subjects, and writes semantic facts, reflections and corrections from them. It
takes the episodes with the highest importance first.

- **Two triggers, and no others:** the sum of the importance of a brain's unconsolidated episodes reaching the
  brain's threshold, and a session being folded or handed off. There is no nightly pass, no pass every N
  turns and no periodic mode: the importance trigger is what stands in for one, the hand-off trigger is the
  other mode.
- **The brain says when it is due:** every recall answers whether the importance threshold is reached, and
  the agent consolidates in its turn when it is, and at its fold. The brain reads the unconsolidated
  episodes out, most important first, and is told which it consolidated; it runs no pass of its own.
- **What the agent writes passes the write judgement** like any fact, through the same call.

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
  weights of the recall score, the per-subject cap, the core's cap of facts per speaker, the core's cap of
  rules and the token budget.
  The brain reads them from its configuration and names each in its health answer.

## Recall

Recall is what a turn is handed. The caller sends the brain the turn: the room's subject, the subjects who
spoke, the words of the turn, the session's key and the moment it opened, and the budget. The brain answers
a block and a list of what it chose.

- **Two parts within one budget** of about one and a half to two thousand tokens: the fixed core, the few
  memories that stand on every turn of that room (its situation rules, the room's `always` rules, and the
  head of the sheet of everyone who spoke), and the searched part, chosen by score. No line of either part
  runs past the budget, a marked one no more than another.
- **The core carries the room's `always` rules** (「Rooms, people and rules」), at most the core's cap of
  rules, six where the configuration names none, in the sheet's order: by importance, highest first, and
  among equals newest first. A rule without the label, or one past the cap, is a candidate of the searched
  part like any other row, and reaches a turn when its words or its subjects call for it.
- **The core carries the head of each speaker's sheet:** the first of their facts in the sheet's order
  (「Rooms, people and rules」), at most the core's cap per speaker, eight where the configuration names
  none. A person's sheet can hold hundreds of facts and a turn has room for a few; the rest of a speaker's
  facts reach a turn only through the searched part, scored and capped as any other row is.
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
- **The persona is short and goes into the system prompt; the lore does not.** The persona already reflects
  the lore as far as a prompt should, so the rendering stays a few pieces. The lore is memories of type
  `lore`: the index is layers, the people and places are subjects, and each piece is a row. A turn gets the
  lore it needs the way it gets any reference: the gate's Laya reads the brain's table of contents, the
  layers, picks the pieces for the turn, and the gate serves them; nothing of the lore stands in the prompt.
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
  caller's rule id and its grantee among its subjects. Revoking a rule invalidates it.
- **A rule that must stand on every turn of its room carries the label `always`:** a standing instruction
  about how to behave in the room, never the record of an incident. The owner decides which rules carry it;
  a rule without it is read when a turn calls for it. Recall's core reads the label, and so may any caller
  that attaches a room's rules to every turn itself. Setting or clearing it is an edit like any other, a new
  version; a learning that strengthens a rule keeps the label the rule carries unless it names the
  label itself.
- **A person is a subject** of type `person`, keyed by the id the caller knows them by. What is known about
  them is rows linked to them, a fact one row each, and a fact that holds only in one room is linked to that
  room as well.
- **A person's sheet is rendered, never stored.** Their sheet is their live `fact` rows that are about no
  room; their sheet in a room is those together with the live facts linked to them and to that room. The
  rendering orders the facts by importance, highest first, and among equal importance newest first: by
  `validFrom`, latest first, then by the moment last confirmed, latest first and a row never confirmed after
  the confirmed ones, then by id, highest first. Each fact is one line carrying the row's id and the date it
  holds from, and the rendering adds nothing that changes between calls, so the same rows give the same
  bytes, as the persona's rendering does. The rendering is the whole sheet; a reader may ask for its first
  lines alone, in the same order. A change to a sheet is a change to a fact: a new version, a closed row or a
  new row.
- **A `sheet` row a store still holds live is split into facts** by the brain's own command
  (`docs/boundary-api.md`, 「Splitting sheets」): one fact per line of it, carrying its subjects, and the
  sheet row is then `done`.
- **What a user said in one room stays in that room's memories** unless it is theirs: the reading narrows
  by the room's subject, and the one carry-over, a user's own words into their private room, is the caller's
  to ask for by naming both rooms.

## What the brain acts on by itself

- **Its clock:** the pruning of its event window, and nothing else.
- **No outgoing call.** The brain calls no unit and runs no model; what it holds is written by its callers.

Reminders, waits on events and watches are not the brain's: a wait is a tool run outside the brain, which
runs the thing waited for, holds it, and wakes the agent with the memory it was told to carry. The brain
keeps the memory; the wait keeps itself.

## What the brain holds besides brains

Two members of the shape the brains replace stand, for the console alone: the memory rows held under a
caller's opaque scope and the fragments of reference text. The console's memory and document screens read
them, list them, move a row's state and switch a fragment; nothing adds one, and they go once those screens
read brains (「The screen」).

Nothing else stands outside a brain. An episode is an `episode` row; a reference piece is a memory of a
reference type in a layer; a room's rule is a `rule` memory; a wait or a reminder is the wait tool's, outside
the brain (「What the brain acts on by itself」); a session's loaded documents and its handoff note are the
agency's session records.

## The screen

The brain's console bundle is the screen where a person sees one agent's long-term memory and sets it
right. The agent never uses it: the agent reads and writes through the brain's tools, and the screen shows
what came of that. What a person changes there takes the shape a tool's change takes, a new version or a
status move, and nothing is deleted. The screen is built from `client/` as a browser module on bmo-ui's
console bundle contract and served by the brain itself (`docs/boundary-api.md`, 「The console bundle」).

- **Who opens it.** An administrator, and no member. The brain does not know who sits at the screen: the
  console relays it with its own token and the reader's role, and the screen shows every brain the brain
  keeps. A write from the screen carries the screen's own `actorName`.
- **One brain at a time.** The toolbar's first control chooses the brain among those `GET /brains` answers,
  each by its name. Two brains share nothing, so no list ever mixes them.
- **The index** stands on the left in three tabs. The layers, in position order, each with its description
  and its count of live memories, the unfiled last; the subjects, grouped by type, each called by its newest
  active `name` memory or else by its type and key; the types in use, the reserved ones first, each with its
  count. Choosing one narrows the memories to it.
- **The memories** stand in the middle as a table on a wide screen and a list on a phone, narrowed by the
  toolbar's filters, which are the memory list's query as it stands: status (live, that is `active` and
  `pending`, by default, or any one status, or all), type (every type but `episode` by default), layer,
  subject, key, label and words, each status and type with its count. They are ordered newest first by
  `validFrom`, or by importance or hits; a layer's memories by their position. The filters, the view and the
  memory open stand in the address, so an address handed on opens the same row.
- **The row view** stands on the right, or over the list on a phone: the text, the type, the key, the layer
  and position, the subjects as chips, the grounds (the `episode` subjects and a learning's source), the
  status with its span, the lineage (which version it is, the versions before and after it, the row that
  superseded it), the hits, both strengths, the importance, the moment of its consolidation, its writer
  and its labels. A superseded version opens under a band leading to the live one.
- **Moves.** Only the moves the status table allows stand as buttons: an `active` row is done or
  invalidated, a `pending` row is approved to `active`, done or invalidated, a `done` or `invalidated`
  lineage is restored, a superseded row has none. A move down asks for its reason; a move to `done` may take
  the words it ended on, written as a version and moved in one call. Confirming a row records a
  confirmation. A live `rule` row turns its `always` label on or off with one tap, a new version with the
  label added or removed and nothing else changed, and the row view says whether the rule stands on every
  turn.
- **Editing in place.** An edit of a live row's text, type, layer, position, subjects, importance or labels
  is saved as a new version. An edit is made over the version read: where another writer wrote a version in
  between, the brain refuses it with a conflict, and the screen shows the new version and keeps the words
  being written. A closed version, an episode's text, the fields the brain sets (ids, lineage, `validTo`,
  hits, strengths, the moments) and a subject's type and key and a layer's name are not editable. Layers
  are added and their descriptions and positions changed, and subjects' descriptions changed, in place.
- **Teaching.** A person teaches through the write judgement: the screen shows the live memories most like
  the words to teach, at most ten, the person judges them as a duplicate, a strengthening, a contradiction or
  new, and one learning with the source `owner` applies it. A piece placed at a position in a layer, a
  persona's, a lore's or a knowledge piece, is written as a memory into that layer.
- **The reading language.** The brain's memories and its persona are English, and the person at the screen may
  read another language. A toggle in the toolbar chooses the reading language: the console's own locale by
  default, or English. In a language other than English, a memory's text wherever it stands (the list, the row
  view, the timeline, the book and the memories offered while teaching), a subject's sheet and a persona layer's
  rendering are shown translated through the translator the console names, page by page as they are shown; each
  translated text in the row view, the timeline, the sheet and the rendering says so and opens its English
  original with one tap, and the toggle turns every text back to English at once. The English stands while a
  translation is on its way, so a translation never holds the English view back, and where one fails the English
  stays with one sentence saying so. Where the console names no translator, the toggle does not stand and the
  screen reads English.
- **A subject's sheet** stands folded over the memories narrowed to that subject, as the brain renders it from
  the facts about it (「A person is a subject」), and not at all where the subject has no fact.
- **Writing in another language.** The brain's rows stay English: no word of another script reaches a row. Words
  typed in the console's own language to teach, to edit a text, or as a move's reason or the words a row ended on
  are translated into English through the same translator before they are saved, and the screen shows the
  English to confirm, with a label saying it was translated; the English, which the person may still change, is
  what is saved. Where the translation fails or no translator is named, the screen says so in one sentence and
  saves nothing until the words are English.
- **The layer index** reads the persona and the lore like a book: the layers in position order, each layer's
  pieces in theirs, moved up and down as new versions. A persona layer shows its rendering, the very bytes
  the brain hands a system prompt, placeholders left as they stand, and a change to a persona piece says
  first that the prompts built on it start their cache afresh once. A layer is exported as a brain file from
  here.
- **A version timeline:** one lineage on a time axis, each version a point at its `validFrom` and a bar for
  the span it held, the point's colour its status and its mark its writer; the rows a merge folded into it
  reach it as lines, and the episodes a conclusion rests on stand under the axis at their moments. A point
  opens the version with the words it changed from the one before.
- **A bubble chart:** one bubble per type or per layer, its horizontal place the group's mean importance,
  its vertical place the mean retrieval strength and its size its count of live memories, so the important
  memories that are cooling stand at the lower right. A bubble narrows the list to its group, and a table
  shows the same figures.
- **A mind map:** one subject at the centre, its live memories around it coloured by layer, the strongest
  first and the rest folded into one count, and the other subjects those memories reach outside them. A
  memory opens its row; an outer subject comes to the centre.
- **Live.** The screen reads the brain's event stream narrowed to the brain it shows, and reads again the
  rows and counts an event touches.

A repository's documents, the rulebooks, the docs and the agents' instructions, are not here: a checkout is
read in the review screen, and the composed system prompt is the agency's to show.

## The order of building

1. The three tables, the brain member, versions, the reference and recall routes, the type list, export and
   injection, the data move, the client in both languages and the documents, with the old routes still
   answering.
2. The writing: the material of the write judgement, the one call that applies it, the consolidation
   reads and marks, and the due signal in recall.
3. The readers move: the reference selection onto recall, the room rules and sheets onto the subject routes,
   the session records onto the agency's own; then the old routes and tables go, all but the memory rows and
   fragments the console reads (「What the brain holds besides brains」). Once the reference
   selection reads this structure, its training data is rebuilt on it and its decision model retrained, at
   the owner's word only, as that unit's own item.
4. The persona and lore of each agent injected from their content repositories' exports, and the agency's
   prompt composition onto the persona rendering.
5. The screen, and with it the memory rows and fragments go.
