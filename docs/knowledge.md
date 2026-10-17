# The knowledge library

What a room has come to know, written as standing documents a turn is offered when its topic calls for them: a
fact that stands on its own, a way of working sorted by whose it is, a rule every room reads.

**What an agent is holding or is under is not here.** A promise, a check, a follow-up and a "when this, do
that" rule the owner set are memory rows, written in the room at the moment they are said (`memories.md`). A
note that sets what an agent does from now on is not the digest's to file.

**How a piece of this library reaches a turn** is the fragment table: each chunk is a row the digest writes with
it, and the gate asks its own models about the rows this room may be offered (`fragments.md`; `boundary-api.md`,
「Fragments」 and 「Brain documents」).

## Whose library it is

- **Each agent has its own knowledge library,** in its own brain folder (`agent-ownership.md`). This document
  says `<brain>` for that folder: the brain root itself for the chat agent, and the folder its registration
  names for another agent.
- **Notes are an agent's too.** A note row names the agent that wrote it, and the digest takes each agent's
  notes into that agent's library alone, writing from that agent's view.
- **A chunk any agent is better for knowing is marked for every agent,** and stays in its writer's library
  (`fragments.md`).

## From a note to a document

### The path

- **A room session writes a note when something worth keeping comes up:** a person's name or how they want to
  be addressed, a preference, a standing arrangement made there, a fact about someone that will matter next
  time. It is a write every room session has, whoever wrote the turn, through the note tool, which calls the
  note route (`boundary-api.md`, 「Notes」). The session says nothing about having done so; a note never
  replaces replying, and nothing whatsoever is sent to the room.
- **Notes are working material, not an archive:** a row is a note still waiting for the hourly digest, and the
  row is deleted once the digest has distilled it. A row existing means the note has not yet reached a
  document.
- **A note says which rooms it is for.** Its own room is the default. A way of working, a piece of know-how or
  a fact that holds wherever the work is done is kept for every room, and carries that mark. It still names the
  room it was said in: that room's batch files it, and the owner check below reads that room.
- **The hourly digest takes each room's pending notes, agent by agent,** and splits each agent's into two
  batches: notes that can be taken as the owner's, and all the others.
- **Each batch goes through these steps in order:**
  1. The digest editor run (work key `digest`) gets the notes and writes the documents.
  2. The documents pass the gate's document sanitiser, the sessions holding them are let go of them and told to
     read them again through the agency's document reload notice, and only the written places are committed.
     The brain has no remote, so nothing is pushed.
  3. The handed-over note rows are deleted.

### The editor decides where each note goes

- **Whose a note is, is the code's answer and is given to the editor,** and so is the mark of a note kept for
  every room, as its writer set it. Everything else is the editor's own reading of the note.
- **Where it may write:** that room's `<brain>/rooms/<room>/rules/`, `<brain>/knowledge/`, and the batch
  authors' `<brain>/people/<userId>/`.
- The destinations are below, under 「Where a note goes」.

### What the editor is handed

- **Whole, the pieces the table picks for the notes:** those tied to an item a note names or carrying a keyword
  a note uses, and the authors' Working requests and Told to remember pieces.
- **The fragment tool, over what it may touch:** the room's rules, every knowledge chunk the room may see, and
  the common books of the notes' authors. It searches those rows, and it is the only way the run writes, merges,
  splits or removes a piece's row (`fragments.md`, 「An editor writes the row through the fragment tool」).
- **Anything else it opens itself,** only where a row the tool found shows the piece holds the matter a note is
  about. A note on a matter a piece already holds goes into that piece; a new chunk is for a matter no row
  covers, and the tool refuses a new chunk that repeats one.
- **The index files are drawn from the rows** once the batch stands and are never written by the run.

### Splitting by author guards approval

- Only the owner's notes may make or change a common rule (a knowledge row marked shared), which every room
  reads, or remove a candidate awaiting approval.
- In a mixed batch, a document alone cannot show whose note a wrongly written common rule came from.
- So the two batches get separate editor runs. A batch with no note of the owner's that made or changed a
  common rule, or removed a candidate, is a failed run.

### Whether an author can be taken as the owner

- Only when the receiver's owner check says the author is the owner and that the owner actually sent a message
  in that room within the hour before the note was written (`boundary-api.md`, 「The receiver」, owner spoke).
- The room session fills in the author, so the value alone is no proof the words are the owner's.
- The code decides this and passes it on; the editor is not asked to judge it.

### When a run fails

- **Failure causes:**
  - the editor run did not report success;
  - a batch without the owner's notes did the owner-only work above;
  - the editor run's own file tools wrote outside its places;
  - the editor run left a user id or room title the boundary forbids in a common person book;
  - the editor run left a chunk or piece it wrote without its row written through the fragment tool, or a write
    that names no piece of its own.
- **What happens:**
  - Everything the run wrote is reverted.
  - No note row is deleted, and the notes are retried the next hour.
  - The owner is told which room's digest failed and why, through the receiver's owner notice.
- **What is compared and reverted:** the documents of the brain (`<brain>/rooms/`, `<brain>/people/`,
  `<brain>/knowledge/`), and of them only the run's own: everything changed inside its places and what its own
  file tools wrote outside them. Another writer's change outside its places neither fails the run nor is put
  back (`people-book.md`, 「The writer writes only its places」).
- Notes piling up while the digest is stopped surface through the pause notice (「The digest run」).

## The digest run

- **Only the owner writes an agent's rules, and the digest is that work on a schedule.**
- **Once an hour the brain's clock asks for the digest run.** The run is launched apart through the agency's
  unit launch, under the fixed unit name `bmo-digest`, so a run still going makes the next launch answer
  `running` and the following hour retry; the work never runs inside a request of the service
  (`boundary-api.md`, 「The digests」).
- **The run ends at once when no notes are pending or a rest is on,** the rest as the receiver's rest state
  says.
- **While the digests are paused it starts nothing,** and the first hourly launch that finds the pause holding
  while notes wait, or while a room holds at least the people digest's minimum of unread messages, tells the
  owner through the receiver's owner notice, once for that pause and never in a room, how many notes and which
  rooms' person books are waiting behind it and how to lift it. Whether the pause has been told is the
  `digest_pause_noticed` key.
- **The editor run:** each batch is one recollector session of the `digest` work key, started through the
  agency, on the model and reasoning that key resolves to for the agent (`agent-ownership.md`, 「The model each
  recollector runs on」), with the file tools only (read, write, edit, glob, grep), no shell, no web, no
  subagents, its task the digest prompt this repository keeps.
- **New notes merge into what exists** rather than appending duplicates, and a note contradicting a rule
  replaces the old text outright.
- **Each batch is a run of the agency's,** opened when the session starts and closed however it ends with what
  it cost and the size it ended carrying, so a digest at work stands in the live picture as an editor.
- **When the run reports success, the digest itself deletes exactly the notes it handed over.** The sessions
  concerned are then told to read what changed again, and the digest commits into the brain what the run wrote
  (the room's rules directory, the knowledge library and the common person books of the notes' authors, nothing
  beside them), since the editor run has no shell to do it.
- **A failure is loud** (「When a run fails」): the owner is told which room's digest failed and why, as a code
  with its parameters through the receiver's owner notice. Only when that notice cannot be sent does the failure
  land in `digest-failures.log` under the data root instead, one line carrying the error, the room and the note
  ids left pending, so nothing disappears silently.
- **A usage limit reached mid-run is not told as a failure:** the agency has rested its login and told the owner
  (`boundary-api.md`, 「The agency」). The run's writes are put back and the digest stops there, its notes left
  pending for a later hour.
- **The lock wait is twenty minutes** (「Boundaries and locking」): a room whose batch cannot have the editing
  lock in that time, because a cut or a rewrite is holding it, is not a failure; its notes wait for the next
  hour.
- **A room session can run the digest at once** instead of waiting for the hour: when the owner asks for the
  records to be organised now, the session calls the digest-now route through its tool, which launches that one
  room's digest apart under the unit name `bmo-digest-<chatId>` and answers at once, so the reply is never held
  and a restart of the session cannot end the run.
- **The per-room unit name doubles as a mutex:** a digest already running for that room makes the route answer
  `running` instead of starting a second one.
- **Triggering it is the owner's alone,** and that is held by the session's judgement rather than by the tools,
  which every session has the same; anyone else's request to remember something still becomes a note and waits
  for the hourly digest.

## Where a note goes

| What the note records | Author is the owner | Anyone else |
|---|---|---|
| A way of working for every room | Common rule chunk `<brain>/knowledge/<slug>.md`, shared | Candidate in `<brain>/knowledge/candidates.md` |
| A way of working for that one person | The author's common person book, piece `c<author>-requests` | Same |
| An arrangement for that room alone | The room's rule chunk `<brain>/rooms/<room>/rules/<slug>.md` | Same |
| Anything that is not a way of working | Below | Below |

- **A note kept for every room never lands in one room.** It becomes no room rule chunk and no knowledge chunk
  scoped to its room. From an author taken as the owner it is a common rule chunk, or a shared knowledge chunk
  where it records a fact. From anyone else it is a candidate, a fact as much as a way of working, and the
  owner's approval makes it the same.
- **A way of working that holds wherever the work is done is never a room rule,** marked or not: know-how about
  a kind of work, a tool or a method is the first row of the table, whichever room it was learned in.
- **When in doubt, the narrower one.** This is for a note whose reach its words leave open. Between every room
  and one person, the person; between the person and that room, the room. A wrong common rule lets one person's
  taste change an agent in every room.
- **No judgement record.** A judgement hangs on one message of a room, and a note is not a message. What was
  decided shows in the documents the editor run writes.

### Common rules

- Written in the rule-chunk shape (Rules, Judgements), with the row written as shared.
- Every room is offered the shared rows, so the rule attaches in any room when a conversation touches it
  (`fragments.md`, 「What a room may be offered」).

### Candidates

- **Anyone but the owner:** a way of working for every room, or a fact kept for every room, is not applied at
  once; it is written only as a candidate awaiting approval. Otherwise anyone in a room could change an agent in
  every room on one editor run's reading alone.
- **The candidates document is no piece of the library,** so it has no fragment row and is offered to no
  session.
- One item is one request: the requester's id, the room it came from, the date and the request.
- A batch that wrote a new candidate tells the owner through the receiver's owner notice, one line per
  candidate.
- **Approval:** when the owner says in any room to apply a candidate, that room's session writes a note with
  the owner as author. It is read as the owner's way of working for every room and becomes a common rule chunk,
  or a shared knowledge chunk for a fact. The same batch's editor run removes the candidate the chunk accepted.
- **Rejection:** also a note with the owner as author. The editor run removes the candidate it names without
  making a rule.
- The editor run reads the candidates document every batch, so only a note by an author taken as the owner
  removes a candidate.

### Personal ways of working

- Go into the author's common person book as a Working requests item, and the author's common sheet is written
  again.
- They attach wherever that person talks, whenever an agent deals with them.
- A note with no author cannot reach a person book, so one about a person without an author is handled as the
  last row of the table instead.

### Room arrangements

- Become a rule chunk of that room and are read only there.
- The owner may also read and set a room's rules directly through the room-rule routes, which take a rule only
  from a message the receiver confirms the owner sent (`boundary-api.md`, 「Brain documents」).

### Anything that is not a way of working

The editor run decides:

- **Knowledge:** a fact that stands on its own becomes a knowledge chunk scoped to its room. Only a fact kept for
  every room by an author taken as the owner, or one the note itself quotes the owner as allowing across rooms,
  is shared.
- **Told to remember:** a fact the author asked to be remembered about themselves, not tied to the room's
  affairs, goes into the author's common person book.
- **Dropped:** a note with nothing worth keeping.
- When in doubt, the room-bound choice.

## Facts of the machines

- **The machines' present state is a knowledge chunk and nothing else:** which card sits in which machine, its
  link and memory, which service runs or stands stopped on which node, a node's role. It is one chunk shared
  across rooms, and it reaches a turn, a job, a search and a browse as any shared knowledge chunk does.
- **Whoever changes the machines writes the chunk then and there:** the job that moved a card, placed a service
  or changed a role, in the same job; the agent, where the change was its own. The chunk is edited as any chunk
  is by hand: the body written in the chunk shape below with each changed item's bracket dated that day, its row
  written again through the fragment write route where its title or keywords no longer say what it holds, the
  brain committed, and the sessions told to read it again.
- **An editor running beside such an edit never takes it back:** a run puts back only what it wrote, so a hand
  edit outside its places stands through its failure.

## What a chunk looks like

- **Every knowledge chunk, room rule chunk, fragment row and candidate is written in English,** under the
  document-writing rule the editor run reads ahead of its task. Names, forms of address, speech examples, short
  quotes and a room's own words keep their original language.
- **Room rule and common rule chunk:** a heading of search nouns, then `## Rules` and `## Judgements` lists.
- **Knowledge chunk:** a heading, a `Source room:` line, then `## Facts`, `## Views` and `## Judgements` lists.
- **Item bracket:** `[date D · confirmed D]`, or `[basis D · confirmed D]` on a judgement. A day is
  `YYYY-MM-DD` in Korean time, a range, or `unknown`.
- **Fragment row:** written through the fragment tool with the chunk's path, a title saying what kind of
  document it is and when a turn needs it (never a list of nouns), the keywords the room uses for the matter,
  the items it is tied to, and on a knowledge chunk whether it is shared. The row is what a gate is offered; the
  scope tag (`[chat:<room>]` or `[shared]`) is the row's, and the drawn index carries it (`fragments.md`).
- **Candidates document:** one heading, `# Common rule candidates awaiting approval`, and one item per
  candidate.
- **A chunk still written in another language, or in paragraphs,** is carried over whole into this shape the
  next time an editor run touches it.

## Boundaries and locking

- **Two libraries, kept apart.** A room's rules live under its own `<brain>/rooms/<chatId>/rules/` and are only
  ever read for that room. The knowledge library is shared on disk but scoped per chunk: every knowledge chunk
  states at its top which room it came from, and its row carries `[chat:<id>]`, or `[shared]` only when the
  owner said explicitly, in the note itself, that the fact may cross rooms.
- **The offer and read routes answer a room's own and the shared chunks alone,** and the read route refuses a
  slug outside that set `out-of-room`, so one room's knowledge cannot surface in another.
- **The person book's parts follow the same line** (`people-book.md`): the room-only part sits under the room's
  own `<brain>/rooms/<chatId>/people/` and is only ever read for that room; the common part sits under
  `<brain>/people/<userId>/` and is read only in rooms that person has spoken in, as the receiver says. Each part
  has two layers, a sheet that attaches whenever that person speaks and the rest cut into pieces, room pieces
  slugged `p<userId>-<topic>` and common pieces `c<userId>-<topic>`.
- **The person-book read takes no slug outside those shapes,** looks for a `p` slug nowhere but this room's own
  pieces, and reads a `c` slug only for a person who has spoken in this room, so a piece of another room has no
  path to reach a prompt and a person this room has never seen is not even named in what it is offered.
- **Room titles:** the editor run's prompt carries, letter for letter, the room titles a common document must not
  contain, as the receiver's room titles give them. Some titles are everyday words, and this keeps a common
  person book from failing every run over one.
- **Person-book rules:** the shape of a person-book piece, the common/room-only boundary, and the check that a
  common piece carries no other person's id or room title are as `people-book.md` sets them.
- **Lock:**
  - A room's batch takes its agent's editing lock, the one that agent's person-book work takes (`people-book.md`,
    「One lock per agent's folder」), before starting the editor run, and releases it after the documents are
    written and committed.
  - The run's writes are found by comparing the agent's whole document tree before and after, and everything
    changed inside its places counts as its own, so no person-book batch of that agent may be writing beside
    it; another agent's may, in its own folder.
  - The lock is released after each batch of person-book and cutting work, and handed over in the order the
    waiters began waiting, so the wait is only as long as the runs already waiting.
  - A room that cannot get the lock in the set time is not a failure. Its notes stay for the next hour; only a
    long job such as a cut, which holds the lock for its whole run, is in the way.
- **A batch that wrote to a common person book writes again the common sheet** of every author whose pieces
  changed.
