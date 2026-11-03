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
- **Per turn it hands out a short block,** chosen by relevance to the turn under a token
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
| `status` | `active`, `pending`, `done`, `superseded`, `invalidated` or `retracted` (「Versions」) |
| `lineageId` | the id of the first version in this row's line; a first version's own id |
| `supersededBy` | the id of the version that replaced this one, or null |
| `supersedes` | the ids of the rows this version replaced, none for a first version |
| `supersededAt` | when this row stopped being current: the moment of the version, merge or move that closed it; null while it is current |
| `validFrom`, `validTo` | the span the memory holds for; `validTo` null while it holds |
| `recallCondition` | the situation in which it is needed, in words, or null (「Recall」) |
| `releaseCondition` | the situation that ends it, in words, or null (「Recall」) |
| `importance` | the writer's weight at the time of writing, 1 to 10 |
| `useCount`, `lastUsedAt` | how often, and when last, it was used: handed out, reported used, confirmed or read explicitly (「Use」) |
| `lastConfirmedAt` | when the memory was last confirmed by a source apart from the reading |
| `actorName` | who wrote this version: the agent's session, a tool, a person at the screen |
| `labels` | the caller's own metadata, a map of strings |
| `createdAt` | when this version was written |

The two conditions are optional. A memory that carries neither is a permanent one: no situation has to be
said for it to come to mind, and nothing but a person's act or a correction ends it.

### Types

`type` is a word the writer writes freely. A few are reserved because code depends on their meaning;
every other value is whatever the writer called it, and the brain lists the types in use with the count of
live memories each, so a meaning does not split into two spellings; a type whose rows are all closed is no
longer in use, and its history is read by status.

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

## Versions

- **Every row is a version.** A new memory is its own first version: `lineageId` equals its id, `validFrom`
  is the moment it is written (or the moment the writer names), `validTo` is null.
- **A condition changes the way the text does.** The two situations a memory may carry, the one that recalls it
  and the one that ends it, are the writer's like its words: a first version sets them, a new version changes
  them and takes the old version's where it gives none, and a merge carries them, while no route changes either
  on a row that stands.
- **A correction is a new row in the same lineage.** The old row's `supersededBy` names the new one, its
  `status` becomes `superseded`, its `validTo` the new row's `validFrom` and its `supersededAt` the moment the
  new row was written. The new row carries the lineage id and names the row it replaced in `supersedes`. A reading hands out the newest row of a lineage that is `active`, and says for a row whose predecessor
  was superseded in the last thirty days what the previous value was and until when.
- **A merge is one new row written over several.** Rows about the same thing, each a lineage of its own,
  are folded into one: the new row starts a lineage, and every merged row becomes `superseded` with its
  `supersededBy` naming the new row, its `validTo` the new row's `validFrom` and its `supersededAt` the moment
  of the merge. The new row takes the use of
  the most used among them and the earliest confirmation, so a reading does not forget what the pieces had
  earned. Nothing is deleted: each merged row stays in its own lineage, pointing at the row
  that stands for it now.
- **A fact that stopped holding is invalidated:** `status` `invalidated`, `validTo` the moment it stopped.
  Nothing points at a replacement. A promise kept or an errand finished is `done`, with `validTo` the moment
  it was done and the text it ended on as a new version where the writer gives one. Whoever moves a row may
  say why, and the reason stays on the row it moved. A row moved to a closed status records the moment of the
  move as its `supersededAt`.
- **A statement the speaker takes back is retracted** ("I take that back", "no, I said it wrong"): `status`
  `retracted`, which means it was never so, where `invalidated` means it held and stopped. Its `validTo` is
  the moment of the retraction. Where the speaker says what they meant instead, the retraction writes that
  as a new version and the retracted row's `supersededBy` names it, so the new row's previous value is read as
  retracted rather than as a value that held until a date. A retracted row is not restored.
- **A row written later may hold an earlier span.** A write whose `validFrom` is earlier than the `validFrom`
  of the live row it would replace, the live row holding its key or the row a learning would close, does not
  replace it: the new row is written already superseded by the live row, its `validTo` the live row's
  `validFrom`, in the live row's lineage, with the reason `earlier-than-live`, and the live row stays
  current. Told in the wrong order, the past takes its place behind the present.
- **The past is read as it stood.** A reading as of a moment answers the rows that held then: those whose span
  covers it, `validFrom` at or before it and `validTo` after it, or, with no `validTo`, not closed before it by
  their `supersededAt`, whatever their status now; a retracted row never held and is never answered.
- **A closed lineage is restored by a new version,** never by reopening the closed row: the newest version
  of a `done` or `invalidated` lineage (never a `retracted` one) is followed by a new `active` version in the same lineage, its text
  the closed one's or the restorer's, and the closed row stays as it was, a record of the span it held for.
  A lineage that holds a live version already has nothing to restore.
- **`pending` is one thing only:** a memory waiting for a person's approval. The agent never parks a
  memory as `pending` because it has one source (「Writing」), and it does not offer common knowledge for
  approval either: what members bring to the shared knowledge the agent collects and keeps on its own, as a
  fact or a rumor by the rule below. A `pending` row is one a person asked to be approved.
- **No row is deleted.** Letting a brain go, by its owner, is the one removal, and it removes the brain
  whole.
  Within a brain, hiding is a status.
- **Reading never changes a row's words or status.** A reading records a use (「Use」); only a contradiction, a
  confirmation, a correction or a person's act writes a version.

## Writing

The agent, reading a message in its turn, decides what the brain learns from it and writes it then, through
the brain's tools. It writes in two moments.

- **Episodes at once.** What was said that may matter later is kept as `episode` rows, each linked to its
  speakers and its room, with an `importance` the agent weighs from how surprising and how consequential it
  is. Episodes are the material consolidation reads and are never consolidated away.
- **The write judgement for every fact.** Before a fact is written, the agent asks the brain for the live
  memories of the same subjects most like it, at most ten, and judges the candidate as one of four: a
  duplicate (nothing is written; the existing row is confirmed, a use recorded and its `lastConfirmedAt`
  moved), a strengthening (a
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
  where it holds in that room alone. Where the writer knows when the memory is needed and what ends it, it says
  so in the same call, as the situations `recallCondition` and `releaseCondition` (「Recall」); both may stand
  empty, and what a memory knows is unchanged by them. Nothing gathers a person's facts into a row of its own;
  their sheet is rendered from these rows (「A person is a subject」).
- **A memory is written in English,** whatever language the room speaks; a short quotation in the original
  words stands only where the exact wording matters.
- **Who writes is recorded** on the version as `actorName`: the agent's session, the tool it used, or the
  person who changed it at the screen.

## Consolidation

Consolidation is the agent's pass outside any answer: it reads episodes not yet consolidated, together with
the live facts about the same subjects, and writes semantic facts, reflections and corrections from them. It
takes the episodes in the brain's priority order.

- **Two triggers, and no others:** the sum of the importance of a brain's unconsolidated episodes reaching the
  brain's threshold, and a session being folded or handed off. There is no nightly pass, no pass every N
  turns and no periodic mode: the importance trigger is what stands in for one, the hand-off trigger is the
  other mode.
- **The brain says when it is due:** every recall answers whether the importance threshold is reached, and so
  does a reading of the consolidation state alone, the same numbers reckoned the same way with nothing
  recorded, for a caller that wants to know without a recall; the agent consolidates in its turn when it is,
  and at its fold. The brain reads the unconsolidated
  episodes out in priority order and is told which it consolidated; it runs no pass of its own.
- **Priority is the expected value of consolidating an episode,** after Mattar and Daw's (2018) value of a
  memory backup: priority = gain × (1 + need), highest first, reckoned at the reading from what the brain
  holds and never stored, so no stored value changes.
  - **Gain** = importance × (1 + novelty). Novelty is one less the greatest share of the episode's stems
    (「Recall」) that a live row which is no episode and shares one of its subjects holds: the stems they
    share over the episode's stems. An episode whose words a fact of its subject already holds has novelty
    zero and gains its importance alone, however long the fact; an episode with no subject or no stem, or
    whose subjects no such row shares, has novelty one.
  - **The reading is bounded by the words index.** The rows a novelty is reckoned against are read once per
    subject for the whole reading, and only those holding at least one stem of an episode about that subject,
    which the words index finds; a row holding none of an episode's stems contains none of them, so the bound
    changes no novelty, and a subject linked to a great many rows costs only the rows that share its
    episodes' words.
  - **Need** is the sum over the episode's subjects of ln(1 + the use counts of the live rows about that
    subject added up) (「Use」): an episode about what the agent keeps using is needed more. An episode with
    no subject has need zero.
  - Equal priorities go by importance, highest first, then the older episode first.
- **A strong episode tags its neighbours** (retroactive tagging, after Dunsmoor and colleagues, 2015), in
  the order alone. An episode whose importance reaches `tag_importance` is a tag. Every other unconsolidated
  episode of one of the tag's rooms, that is linked to a subject of type `room` the tag is linked to, written
  within `same_session_ms` before or after the tag, that is not a tag itself and stands below it in priority
  order, is read out directly after the tag, in its own priority order, and names the tag. An episode two tags
  reach follows the one higher in the order; one standing above the tag keeps its place.
- **The reading shows its reasons:** each episode is answered with its priority, its gain, its need and the
  tag it follows, where one pulled it.
- **What the agent writes passes the write judgement** like any fact, through the same call.

## Use

The brain records how a memory is used and lets nothing fade by itself.

- **Every use is recorded on the row:** a recall's hand-out, the agent's report of the rows it used, a
  confirmation, and an explicit read by key, subject, type or words, each a use at its moment. The row keeps
  how often it was used (`useCount`), when last (`lastUsedAt`), and the moments of its newest uses, at most
  `hit_times_kept` of them. A version carries the use of the version it follows.
- **Nothing ends a memory but an event.** No strength, decay or threshold stands on a memory, and neither age
  nor disuse drops it from a reading or from recall: a memory ends only when its release condition comes true,
  a correction supersedes it, a merge folds it into another, or it is moved to `done` or `invalidated`.
- **Use orders, it never excludes.** Recall puts the row used last first among equally relevant and equally
  important rows (「Recall」); a reading of the screen orders by it where asked.
- **The numbers are settings,** with placeholders until the owner's own conversation logs have fitted them:
  the importance threshold of consolidation, recall's length weighting (BM25's standard k1 and b) and its
  selection's balance, the per-subject cap, the core's cap of facts per speaker, the share of a condition's
  words a turn holds before the condition matches, the token budget, the most use moments a row keeps, and
  consolidation's tag importance and its window, the span of one session. The brain reads them from its
  configuration and names each in its health answer.

## Recall

Recall is what a turn is handed. The caller sends the brain the turn: the room's subject, the subjects who
spoke, the words of the turn, the session's key and the moment it opened, and the budget. The brain answers
a block and a list of what it chose.

- **Two parts within one budget** of about one and a half to two thousand tokens: the fixed core, the few
  memories that stand on every turn of that room (its situation rules and the head of the sheet of everyone
  who spoke), and the searched part, chosen by relevance. No line of either part runs past the budget, a
  marked one no more than another.
- **No rule of the room stands in the core.** A room's rules reach a turn through selection, never by
  standing: each is a candidate of the searched part like any other row, chosen when the turn's words or
  subjects call for it, and a caller that selects a turn's reference picks among them as it picks any
  reference.
- **The core carries the head of each speaker's sheet:** the first of their facts in the sheet's order
  (「Rooms, people and rules」), at most the core's cap per speaker, eight where the configuration names
  none. A person's sheet can hold hundreds of facts and a turn has room for a few; the rest of a speaker's
  facts reach a turn only through the searched part, scored and capped as any other row is.
- **Cues.** A turn's cues are the subjects it names that stand in the brain, its room and its speakers, and
  its words: lowercased, English stop words left out, each reduced to its Snowball stem. A memory's words are
  indexed the same way, so a turn's word reaches every memory holding a word of the same stem, and a stop word
  reaches none.
- **The searched part's candidates** are the live rows a cue reaches, about a subject the turn names or
  holding one of its stems, and the rows whose recall condition the turn says. Nothing drops a candidate for
  its age or its disuse.
- **Relevance** is how strongly the turn's cues point at a row, after ACT-R's spreading activation:

      relevance_i = Σ_j  W_j · (S − ln fan_j)

  over the cues j the row is linked to, where W_j = 1 / J for the J cues the turn carries, fan_j is the
  number of live memories of the brain linked to cue j, each counted once, and S = ln N, N being the number of
  live memories in the brain at the recall. Each matched cue so gives (1 / J) · ln(N / fan_j), its inverse
  document frequency, never below zero: a cue few memories share points at them strongly, one nearly every
  memory shares points at them hardly at all, and a row matching more of the turn's cues stands above one
  matching fewer of them. Relevance goes one hop, from a cue to the memories linked to it.
- **A stem weighs less in a long row,** after BM25's term weight with the term frequency taken as one
  (Robertson and Zaragoza 2009). A matched stem cue gives

      (1 / J) · ln(N / fan_j) · (k1 + 1) / (1 + k1 · (1 − b + b · len_i / avg_len))

  where len_i is the number of the row's stems and avg_len the mean of that number over the brain's live
  memories at the recall, k1 being `bm25_k1` and b `bm25_b`. A row of average length gives a stem its plain
  inverse document frequency; a long row, which holds many common stems by its length alone, gives each less,
  and a short one more, so a short precise row is not outranked by a long one that happens to hold more of the
  turn's common words. A matched subject cue gives (1 / J) · ln(N / fan_j) with no length term, a subject link
  being no word of the text.
- **A situation is words.** A memory's `recallCondition` says when it is needed, and a turn says it where the
  turn's words hold at least the share of the condition's words the rules name: that row is a candidate though
  neither its own words nor its subjects name it, and its relevance is the most a cue set gives, ln N, as
  though every cue were linked to that row alone.
- **Rows near in meaning join the candidates** (「Vectors」): the turn's words are embedded once, and the
  `vector_candidates` live rows of the searched types nearest to them by cosine are a second ranked list beside
  the rows ranked by relevance. The two lists are fused by Reciprocal Rank Fusion (Cormack, Clarke and
  Buettcher 2009): a row's fused score is the sum over the lists it stands in of 1 / (k + its rank there), k
  being `rrf_k` and a rank counted from one and shared by the rows of equal relevance or equal cosine, so a
  row high in both lists stands above one high in one alone, and rows a list cannot tell apart stay equal.
  Where no vector list stands, the turn's words not embedded, the fused order is the relevance order.
- **The order** of the candidates is the fused score, highest first; between equal scores importance, highest
  first; then the latest use, a row never used after the used ones (「Use」). Nothing is normalised or
  weighed against anything else.
- **Selection is Maximal Marginal Relevance** (Carbonell and Goldstein 1998) over that order, the fused score
  standing where the relevance stands in it. The next row is
  the candidate that maximises λ · R_i − (1 − λ) · max_j Sim(i, j) over the rows already in the block, the
  core's among them, where Sim is the Jaccard overlap of the two rows' stem sets and λ is `mmr_lambda`; equal
  values go by the order. R_i is the relevance rescaled to 0 to 1 between the least and the greatest of the
  candidates, for this arithmetic alone, so the two terms stand on one scale; where every candidate's is the
  same, R_i is one. No subject fills more than its cap of rows, a line that would carry the block past the
  budget is passed over, and the selection runs until the candidates are spent, so a near-double of a row in
  the block comes late and is the first the budget leaves out.
- **A release condition that comes true ends the memory.** Where the turn's words hold a row's
  `releaseCondition`, at least the share of its words the rules name, that situation has come true: the row is
  not handed out, and recall moves it to `invalidated` with the moment and the condition as its reason, in the
  same transaction as the rest of the recall, whether or not the row would have been carried. The answer names
  each row it released.
- **Only live rows, with their span.** A row goes out with the dates it holds for, since its `validFrom` and,
  where it has one, until its `validTo`; a row whose predecessor was superseded lately goes out with one line
  saying what the previous value was and until when, or that the previous value was retracted.
- **What this session already knows is left out.** A memory written after the session opened is not handed
  back to that session: the session saw the conversation it came from. Once the session is folded or handed
  off, the row may attach again.
- **A memory handed out once to a session is marked, not repeated,** while its version is unchanged: the
  brain keeps, per session key, which versions it has handed out, answers the mark in place of the text, and
  clears the marks when the caller reports the fold.
- **A `rumor` is handed out as heard,** marked so in the block. A `pending` row is never handed out.
- **Every hand-out is a use** of the rows chosen, which the brain records itself.

The caller that selects a turn's reference asks recall and decides what of it the turn is handed; the block
is the brain's offer, the caller's is the last word.

## Vectors

A memory is also searched by its meaning, as a vector its text is embedded into, beside its words.

- **The engine is the platform's embedding model** behind its LLM gateway, asked over the OpenAI-shaped
  embeddings route at the address the configuration names, with the bearer key its key file holds, under the
  model name it names (`[embedding]`). Where the configuration names none, nothing is embedded and every search
  is by words alone.
- **What is embedded:** a row's text as it stands, with its recall condition after it where it carries one;
  a query's words after the model's instruction for a search, `Instruct: <query_instruction>` and `Query:`, the
  instruction its training was given, which the configuration may leave empty.
- **Cut short of the engine's context:** a text is cut where a conservative count reaches `max_tokens`, held
  under the engine's context: a CJK or Hangul character counts as a token, any other three characters as one.
  The count is the brain's own and never asks the engine, so it holds whatever context the engine is started
  with.
- **A text the engine refuses does not hold the rest back.** Where the gateway refuses a batch as too large,
  its texts are embedded one by one; a text still refused is tried again cut to half as many tokens, down to
  64, and one refused even then is recorded as one the model does not embed, counted in the health answer and
  passed over until the model changes, where the gateway takes a text of two letters; where it refuses that too,
  the gateway refuses every ask, as while the model is out of its table, and nothing is recorded.
- **Every write is embedded after it:** a new row or version wakes the brain's embedder, which embeds the live
  rows lacking a vector of the current model in batches, the configuration's batch at a time, and keeps each
  vector beside its row. While the gateway refuses or does not answer, it tries again after a pause. A row with
  no vector yet is searched by its words alone. At start, once the vectors are read in, the embedder fills
  every live row lacking one; `bmo-brain embed` does the same once and ends.
- **The vectors are searched in the process:** the vectors of the current model are read into memory at start
  and kept as the embedder stores them; a query is compared with every vector of its brain by cosine, and a
  row found no longer live is let go of. Nearest-neighbour search inside the database may take its place.
- **A model is one space:** a vector is kept under the model that made it, and a search reads only the current
  model's. A change of model leaves the new model's vectors empty until the embedder has filled every live row,
  and the other models' vectors are deleted once it has.
- **A query is embedded once an ask:** a recall's turn words, a similar ask's text, a memory list's words, cut
  as a row's text is, the instruction counted in.
  Where the gateway does not answer, the ask is answered by words alone and counted as skipped; the health
  answer carries how many asks used vectors, how many words alone, and how many were skipped.

## What the brain counts

The brain counts how often a search by words finds nothing, so whether a search by meaning is worth adding is
judged on the brain's own record.

- **An ask by words is a similar ask** (the candidate's text) **or an explicit read by words** (a memory list
  by `q`, its first page, recording uses). Each one counts as an ask of its brain, and one that answers no row
  counts as a miss too; an ask with no words at all counts as neither.
- **A miss is kept as a row of its brain:** the moment, the words as asked, the stems they became
  (「Recall」), and which ask it was. A brain keeps its newest misses, at most `misses_kept` of them; the counts
  of asks and misses are kept whole.
- **The record is read, never acted on:** the misses are listed newest first, and the health answer carries
  each brain's asks and misses. Nothing the brain hands out changes by them.

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
  caller's rule id and its grantee among its subjects. Revoking a rule invalidates it. A rule reaches a turn
  when it is selected for it (「Recall」); none stands on every turn.
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

Nothing but its event window. Every member a caller reads or writes stands inside a brain: an episode is an
`episode` row; a reference piece is a memory of a reference type in a layer; a room's rule is a `rule`
memory; a wait or a reminder is the wait tool's, outside the brain (「What the brain acts on by itself」); a
session's loaded documents and its handoff note are the agency's session records. No row is held under a
caller's opaque scope, and no table stands beside the brains' own: a brain's count of its searches and the
misses it keeps (「What the brain counts」) are its own, let go with it.

## The screen

The brain's console bundle is the screen where a person sees one agent's long-term memory and sets it
right. The agent never uses it: the agent reads and writes through the brain's tools, and the screen shows
what came of that. What a person changes there takes the shape a tool's change takes, a new version or a
status move, and nothing is deleted. The screen is built from `client/` as a browser module on bmo-ui's
console bundle contract and served by the brain itself (`docs/boundary-api.md`, 「The console bundle」).

- **Who opens it.** An administrator, and no member. The brain does not know who sits at the screen: the
  console relays it with its own token and the reader's role, and the screen shows every brain the brain
  keeps. A write from the screen carries the screen's own `actorName`.
- **One title bar.** The screen's route asks for the console's one title bar (bmo-ui's
  `docs/console-bundles.md`, 「The title bar」). Under a console that offers it, the screen draws no bar of its
  own and lays its two actions on the screen as a whole, the reading language's toggle and teaching, in the
  place the bar keeps for them; standing alone, it draws its own bar with the screen's name and keeps the two
  at the end of the toolbar.
- **One brain at a time.** The toolbar's first control chooses the brain among those `GET /brains` answers,
  each by its name. Two brains share nothing, so no list ever mixes them.
- **Three panes.** The index on the left, the view in the middle and the memory open on the right. The right
  pane stands only while a memory is open; with none open the view takes its room. On a phone the view is the
  body, the index rises from the bottom edge and a memory stands over the view.
- **The memories stand as the console's shared table where their pane holds it** (bmo-ui's `DataTable`, the
  record browser's close grid): its column names held overhead while the rows scroll, a row pressed to open its
  memory, and every column's width moved by dragging the edge of its name. Its text column takes the room left,
  never less than 8rem; every other column — the type, the layer, the status, the importance and the start of
  validity — keeps one narrow width of its own down the whole table, a value too long for it cut with its whole
  value in the cut line's tooltip; no name ever stands over another. The table fills the view and scrolls its
  own rows; under a subject's sheet it takes its own height and the view scrolls both. Where the pane is
  narrower than 588 pixels, as it is beside an open memory, the rows stand stacked as on a phone, with no names
  over them.
- **One toolbar** under the title bar carries the brain, the view and every filter, and nothing else is a toolbar:
  a view's own controls stand at the end of the view's heading. Every select carries a visible label naming
  what it chooses (the brain, the view, the status, the type, the order), and its value reads as words a
  person understands, with its count: the live memories, a status, every type but episodes, a type by its
  name. On a phone the view is chosen by a labelled select rather than a row of marks, and the toolbar flows
  sideways.
- **The index** stands on the left in three tabs, 레이어, 대상 and 유형 in the Korean pack, each with one line
  under the tab head saying what it lists and that choosing one narrows the view. The layers, in position
  order, each with its description and its count of live memories, the unfiled last; the subjects, grouped by
  type, each called by its newest active `name` memory or else by its type and key; the types in use, the
  reserved ones first, each with its count of live memories. A chosen row stands highlighted and is scrolled
  into sight, the tab holding it is the one open, and choosing it again clears it.
- **Every choice narrows the view, or says why it cannot.** A layer, a subject or a type chosen in the index,
  and every toolbar filter, narrows whatever the view shows: the memories, the layer's items, the memories
  around the map's centre, the bubbles. Where a view cannot honour a choice (the version timeline shows one
  lineage, so it honours none; the layer view and the map keep their own order), that choice is disabled there,
  in the index and in the toolbar alike, with one line saying why, and it is never silently ignored. The view
  scrolls back to its start whenever the choice changes.
- **The heading** stands at the top of the middle pane and names exactly what is shown: the brain, the view,
  each choice standing (the layer, the subject, the type, the status, the words, the key, the label), each
  removable on its own, and the count of what the view holds under them. One press beside them clears every
  choice. The view's own controls stand at its end: the layer view's switch between its items and the prompt
  source and its export, the bubble chart's grouping and its table.
- **The memories** stand in the middle as a table on a wide screen and a list on a phone, narrowed by the
  filters, which are the memory list's query as it stands: status (live, that is `active` and `pending`, by
  default, or any one status, or all), type (every type but `episode` by default), layer, subject, key, label
  and words, each status and type with its count. They are ordered newest first by `validFrom`, or by
  importance or use; a layer's memories by their position. The filters, the view and the memory open stand
  in the address, so an address handed on opens the same row. The screen's lists are a person's view of the
  brain, not the agent's reading, so they record no use (「Use」).
- **Names in the reader's language.** A type, a subject's type and a status are shown by their names in the
  language pack, a type the pack does not name by its own word; no code, enum word or JSON stands in the
  screen.
- **The row view** stands on the right, or over the list on a phone, under a heading naming the memory (its
  type's name and its number) with a close control at its start and its own actions, editing and its version
  timeline, as labelled icon buttons at its end. It has no tabs: its sections stand one under another, the
  text, the moves, then the fields: the type, the key, the layer and position, the subjects as chips, the
  grounds (the `episode` subjects and a learning's source), the status with its span, the version history
  (which version it is, the versions before and after it, the row that superseded it), the use (how often, when
  last, the importance), the moment of its consolidation, its writer and its labels. A superseded
  version opens under a band leading to the live one.
- **Moves.** Only the moves the status table allows stand as buttons: an `active` row is done or
  invalidated, a `pending` row is approved to `active`, done or invalidated, a `done` or `invalidated`
  lineage is restored, a superseded row has none. A move down asks for its reason; a move to `done` may take
  the words it ended on, written as a version and moved in one call. Confirming a row records a
  confirmation.
- **Editing in place.** An edit of a live row's text, type, layer, position, subjects, importance or labels
  is saved as a new version. An edit is made over the version read: where another writer wrote a version in
  between, the brain refuses it with a conflict, and the screen shows the new version and keeps the words
  being written. A closed version, an episode's text, the fields the brain sets (ids, lineage, `validTo`,
  the use, the moments) and a subject's type and key and a layer's name are not editable. Layers
  are added and their descriptions and positions changed, and subjects' descriptions changed, in place.
- **Teaching.** A person teaches through the write judgement: the screen shows the live memories most like
  the words to teach, at most ten, the person judges them as a duplicate, a strengthening, a contradiction or
  new, and one learning with the source `owner` applies it. A piece placed at a position in a layer, a
  persona's, a lore's or a knowledge piece, is written as a memory into that layer.
- **The reading language.** The brain's memories and its persona are English, and the person at the screen may
  read another language. A toggle among the screen's actions chooses the reading language: the console's own locale by
  default, or English. In a language other than English, a memory's text wherever it stands (the list, the row
  view, the timeline, the layer view and the memories offered while teaching), a subject's summary and a persona
  layer's prompt source are shown translated through the translator the console names, page by page as they
  are shown; each translated text in the row view, the timeline, the summary and the prompt source says so and
  opens its English original with one tap, and the toggle turns every text back to English at once. The English stands while a
  translation is on its way, so a translation never holds the English view back, and where one fails, or the
  translator stays too busy to read a text and answers it untranslated, the English stays, without the
  translated label or the link to an original, with one sentence saying so. Where the console names no
  translator, the toggle does not stand and the screen reads English.
- **A subject's summary** (인물 요약 in the Korean pack) stands folded over the memories narrowed to that
  subject, headed by the subject's name, as the brain renders it from the facts about it (「A person is a
  subject」), and not at all where the subject has no fact.
- **Writing in another language.** The brain's rows stay English: no word of another script reaches a row. Words
  typed in the console's own language to teach, to edit a text, or as a move's reason or the words a row ended on
  are translated into English through the same translator before they are saved, and the screen shows the
  English to confirm, with a label saying it was translated; the English, which the person may still change, is
  what is saved. Where the translation fails, the words come back untranslated, or no translator is named, the
  screen says so in one sentence and saves nothing until the words are English.
- **The layer view** (레이어 in the Korean pack) reads the persona and the lore like a book: the layer chosen in
  the index, the first where none is, its items in position order, moved up and down as new versions, narrowed
  by the type, the subject, the status and the words. A persona layer shows its prompt source, the very bytes
  the brain hands a system prompt, placeholders left as they stand; the prompt source is the whole layer's
  and no filter narrows it. A change to a persona item says first that the prompts built on it start their
  cache afresh once. A layer is exported as a brain file from here.
- **A version timeline:** one lineage on a time axis, each version a point at its `validFrom` and a bar for
  the span it held, the point's colour its status and its mark its writer; the rows a merge folded into it
  reach it as lines, and the episodes a conclusion rests on stand under the axis at their moments. A point
  opens the version with the words it changed from the one before.
- **A bubble chart:** one bubble per type or per layer, its horizontal place the group's mean importance,
  its vertical place the mean use and its size its count of memories under the filters, so the important
  memories that are seldom used stand at the lower right. A bubble narrows the list to its group, and a table
  shows the same figures.
- **A mind map:** one subject at the centre, its memories under the filters around it coloured by layer, the most used
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
   the session records onto the agency's own; then the old routes and tables go (「What the brain holds
   besides brains」). Once the reference selection reads this structure, its training data is rebuilt on it
   and its decision model retrained, at the owner's word only, as that unit's own item.
4. The persona and lore of each agent injected from their content repositories' exports, and the agency's
   prompt composition onto the persona rendering.
5. The screen, the console's one view of a brain (「The screen」).
