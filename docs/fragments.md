# Reference fragments

Every piece a turn may be offered from the agents' libraries stands as a row in the brain's `fragments` table.
The row is the piece's index entry and everything known about it: who and what it is about, the items it is
tied to, the words it is asked for by, when it holds and whether its matter is finished. The text it names
stays a file and is never copied into the table. The brain answers the rows; the gate asks its own models which
of them a turn needs (`boundary-api.md`, 「Fragments」).

## What a fragment is

- **One row is one piece a gate can be asked about:** a system document, a lore piece, a room rule, a knowledge
  chunk, a person-book piece.
- **The row holds the entry, not the text.** Its slug is the name a gate answers under, its title is the
  one-line summary a gate picks by, and its path says where the body is read from.
- **The body stays where it was written.** A system document and a lore piece live with the unit that ships
  them; the rest are documents of the brain, written there by the editors that own them.
- **A fragment nothing offers is still a row.** Taking a fragment down leaves its row standing with its counts,
  the way a memory that has come down keeps its own (`memories.md`).

## The kinds, and where each row comes from

A row's kind is the library it belongs to. Each library the brain writes is a member of one library trait, the
extension point a new brain library is added at: it says what its index is, which of its entries become rows,
where a slug's body sits, and whether its pieces settle with their items. A shipped library answers the same
questions through what its owner hands in, and its pieces never settle. Nothing central reads a library's files
on its behalf, and nothing branches on a kind's name.

| Kind | Where its bodies live | Where its rows come from |
|---|---|---|
| a shipped kind, such as the system documents or the lore | with the unit that ships them | that unit's entries, handed in through the reconcile route |
| `room-rule` | the room's rules in the brain | the digest's writes through the fragment tool, and the room-rule route |
| `library-knowledge` | the knowledge library in the brain | the digest's writes through the fragment tool |
| `person-book` | the room part and the common part of the person book in the brain | the person-book writer's and the digest's writes through the fragment tool |

- **The brain's three libraries are the table.** Their rows are the one index they have; an editor writes a row
  when it writes the piece, and the index files beside the pieces are drawn out of the rows afterwards (「The
  brain's libraries」 below).
- **The shipped libraries follow their owners.** A shipped piece is another unit's code, edited with its index
  and deployed with it; that unit hands its entries in and the rows are brought to them (「The shipped
  libraries」 below). The library trait's members are the brain's three libraries; a shipped library is no
  member of it, and the brain names no shipped kind.
- **Each kind carries the word a screen calls it by,** as its own answer, so a kind added without a name is
  refused before it can be shown as a slug.

## What one row holds

- **Kind:** which library it is a piece of.
- **Room:** the room it belongs to, or nothing at all where it belongs to every room. A room rule, a room-only
  person-book piece and a room-scoped knowledge chunk name their room; a system document, a lore piece, a
  shared knowledge chunk and a common person-book piece name none.
- **Owner:** the person the piece is about, on a person-book piece, and nothing on every other kind. It never
  says whose memory the piece is; the agent does.
- **Agent:** the agent whose long-term memory the piece is: for a piece of the brain, the agent whose folder its
  body lies in (`agent-ownership.md`), and for a shipped piece, the chat agent. Only that agent's turns are
  offered it, unless it is marked for every agent.
- **For every agent:** the mark that every agent's turns may be offered the piece, a fact any of them is better
  for knowing. Off unless the writing recollector or a person sets it; never a default. It is a third axis
  beside the room (the scope tag) and the person (the owner).
- **Slug:** the name the gate answers under, unique within its kind, its agent, its room and its owner.
- **Title:** one or two sentences on what kind of document the piece is and the questions or situations a turn
  needs it for, in English with the domain terms the document-writing rule keeps. It is what a gate picks by
  and what a drawn index line says. It is never a list of nouns: a line of nouns strung with commas, or one
  hung on the words nearly every turn carries, is matched by every turn and the piece is attached where it has
  no business. The agent's own name as its rooms call it never stands in a title, since every turn that wakes
  it carries that word. The situation it names is the particular question the piece settles, never the
  everyday work of the rooms it reaches.
- **Keywords:** the words somebody asks for the piece by that are particular to it: the words the room itself
  uses for the matter, names, product and system names, identifiers, and none of the common words. They are
  the writer's alone. The common words are the ones nearly every turn of the rooms carries: the policy's list,
  the chat agent's key and its label as its registration names it, and the owner's names the configuration
  holds, all compared in lower case. A person's address piece is the exception: its keywords are the names
  that person is called by.
- **Items:** the TODO items the piece is tied to: the ones its writer named, and every `#<n>` its body carries.
- **Written and confirmed:** the earliest day and the latest confirmed day its item brackets carry
  (`[date D · confirmed D]`), or nothing where it carries none. The period a drawn index line shows is read
  from them.
- **Status:** `open` or `done`. A piece is done when it is tied to items and every one of them is finished, as
  the agency's item states say; a piece tied to none is open. A library whose pieces never finish (a room rule,
  a system document, a lore piece) keeps every piece open, and each library says for itself whether its pieces
  settle with their items.
- **Body hash:** a digest of the body as the table last read it, which is how a body edited outside the
  editors' road is told apart.
- **Path:** where the body is read from: relative to the brain folder for the three brain libraries, and as its
  owner hands it in for a shipped piece. Never an absolute path.
- **Scope tag:** the bracket the knowledge library's entries carry: `[shared]`, or `[chat:<room>]` for a chunk
  of one room. Only that kind has one.
- **Position:** where the entry stands in its index. The order candidates are offered in is part of the prompt,
  so it is kept; a new piece stands after every piece already in its index.
- **Active:** whether the fragment is offered at all.
- **Last hit and hits:** when a turn last attached it, and how many turns have.
- **Set by and set at:** the person who set it where a person did, and when the row first stood. A row an
  editor or a reconcile wrote was set by nobody, and says so.
- **Set by hand:** the standing a person gave the row themselves, and when. Null on a row no person has
  touched.

**Uniqueness.** One row per kind, agent, room, owner and slug together, with an absent room and an absent
owner normalised to a fixed value so the uniqueness holds across the rows that name neither.

## What a room may be offered

- **The offer route answers by kind, room and agent:** the turn's agent's own rows and those marked for every
  agent, of the kinds asked, in position order. The gate narrows and picks from them; the brain picks nothing.
- **A room's pieces and the shared ones:** room rules of that room; knowledge chunks of that room and the
  shared ones; the room part of the person book of that room, where it keeps one; the common part of the people
  who have spoken in that room, as the receiver says; every shipped piece.
- **Where several indexes stand together,** the common books follow the room's part, by the person's id.
- **The bodies are read through the document routes,** which refuse a slug outside what the room may be
  offered (`knowledge.md`, `people-book.md`).
- **What a turn leaves on a row:** the gate hands back the pieces it attached, and the brain writes each one's
  count and last hit. A piece a gate asked for that the turn then dropped did not attach and does not count.

## The brain's libraries

### An editor writes the row through the fragment tool

- **The body is written with file tools; the row through the fragment tool.** An editor (the digest and the
  person-book writer, in all of its runs) writes and edits piece files with its file tools, and every piece it
  creates, changes, merges, splits or removes has its row written through the fragment tool, a separate
  repository that calls the brain's desk routes (`boundary-api.md`, 「Fragments」). The rule it reads is one of
  the recollectors' rules this repository keeps.
- **What the tool answers:**
  - `fragment_search`: the rows the run may reach, narrowed by words, an item, a person or a status, each as
    the line a writer is shown, with the writes this run has made laid over them.
  - `fragment_write`: one piece's row: its title, its keywords, its items, on a knowledge chunk whether it is
    shared across rooms, and on a knowledge chunk or a person-book piece whether it is for every agent. It
    answers with the row as it will stand.
  - `fragment_remove`: a piece taken away: its file deleted and its row marked to go down. The file tools
    cannot delete a file, so this is the one way an editor removes a piece.
  - `fragment_merge`: pieces folded into one: the kept piece's row written, every other piece removed.
  - `fragment_split`: one piece cut into several: a row for each piece that comes out of it, and the original
    removed unless it is one of them.
  - `fragment_owed`: the pieces this run has created or changed on disk and written no row for yet, which is
    what fails a run that ends with them.
- **Everything else on the row is the code's.** The kind, the agent, the room, the owner and the slug are read
  off the path by the library the path belongs to; the dates, the body hash and the body's own item numbers
  are read off the body; the status is settled against the item states; a new piece's position is the end of
  its index.
- **Every call is checked as it is made,** and a refused call writes nothing and says why, so the writer puts
  it right inside the same run (`boundary-api.md`, 「Fragment refusals」):
  - a path that is no piece of a brain library, or a piece outside the folder of the agent the run remembers
    for;
  - a write for a piece whose body is not on disk;
  - a piece with an item whose day bracket leaves its slot names out, since the dates are read off the named
    slots alone and such a row would carry none;
  - a title the shape check refuses (below);
  - a new piece whose matter a piece the run may reach already holds (「Finding a piece before writing one」).
- **A run stands in the folder of the agent it remembers for.** Its working directory is that agent's brain
  folder, and every path the tool takes from it or shows it is relative to that folder; the desk turns them
  into the brain's own paths before it reads or writes a row, and offers the run only that agent's rows. For
  the chat agent the folder is the brain root.
- **The writes wait on a desk until the run stands.** The tool writes nothing to the table. The batch opens a
  desk for each run, under the data root, naming the run's room, where it may write, the rows it may reach, the
  writes earlier runs of the same batch made, and the names the checks read (the agent's name and key, and the
  words every turn carries); the desk keeps the run's writes. A run's rows share the run's fate: a run that
  fails has its files put back, and its writes, never having reached the table, go with it.
- **What the batch holds the run to once it has answered,** with everything the run wrote put back on any of
  them:
  - a piece it created or changed with no write of its own;
  - a write naming a path that is no piece of a brain library, or a piece outside where the run may write;
  - a write for a piece that is not on disk and is not removed;
  - a piece whose day brackets leave their slot names out;
  - a title the shape check refuses.
- **Only a batch that stood whole writes its desks' rows to the table.** A run is checked for where it wrote
  and what a common piece names first (`people-book.md`, `knowledge.md`); only then are its writes held
  against what it wrote.
- **A piece whose file the run deleted goes down** whether or not a write says so, keeping its counts, and a
  write naming it again later raises the row it had.
- **The desk is the session's.** The agency starts every recollector session with the same tool configuration,
  so the tools a run is shown are the same for every run; the session's record names its desk, and a call
  naming no open desk is refused.
- **The index files are drawn from the rows.** Once a batch's rows are written, every library draws its index
  files out of its active rows, in position order: the room part of a person book with one section per person,
  a person's common index, a room's rules index, the knowledge index. Nothing reads them back: they exist for a
  person and a session to open, and an edit made to one by hand is overwritten at the next drawing.

### The shape a title must have

- **Checked the moment it is written.** A title shorter than 40 characters or longer than 320, a title that is
  four or more short pieces strung together with commas or middle dots, a title more than two in five of whose
  words are the words every turn carries, and a title that names no situation (when, if, only, or their
  equivalents in the room's language) are refused. The fragment tool refuses the write that carries one, and
  the batch refuses a run that ends with one all the same.
- **Every writer is held to it:** the digest, the person-book writer in all of its runs, and a person or a job
  writing a piece's row by hand through the write route.
- **A row already standing is held to it only when it is written again.** Nothing takes down a standing row for
  its title; the drift check counts such rows instead.

### Finding a piece before writing one

- **What a writer may reach is the batch's to say.** The person-book writer reaches every piece of the people
  it writes, in both parts of their book; the digest every room rule of the room, every knowledge chunk the
  room may see and the common books of the notes' authors. That set is written on the run's desk, and the tool
  answers over it and nothing else.
- **The bodies the table picks for the batch are handed over whole.** The editor's own narrowing runs over
  those rows with the batch's own text: a piece tied to an item the batch names, or carrying a keyword it uses,
  is handed over whole, and so is every fixed-heading piece of the people being written. The narrowing is the
  brain's own copy of the rule the gate narrows a turn's candidates by: a turn's lines and a row's keywords
  and items.
- **Everything else is found through the tool, and no index is handed over.** A book of hundreds of pieces
  written out ahead of every run would be paid for in every call that run makes.
- **A piece not handed over whole is opened only when its row shows it holds the same matter.** Where a piece
  already holds the matter, the writer edits that piece and writes its row; a new piece is for a matter no row
  covers.
- **A new piece that repeats one is refused.** A write for a path with no row yet is held against every row the
  run may reach in the same library, the writes of this batch included: one sharing an item with it, or
  carrying two of its keywords among its own keywords or in its slug and title, may already hold its matter,
  and the write is refused naming them. The writer then edits that piece, merges the two, or, where the matter
  really is another, writes again naming each of them as distinct. A merge's kept piece and a split's pieces
  come out of pieces already standing, and are held against nothing.

### The reconcile

The brain's rows are the source, so their reconcile repairs what can be read off the bodies and nothing else.

- **A body changed on disk** (its hash no longer the row's) has its dates, its body items and its hash read
  again. The title, the keywords and the mark for every agent stay: they are the editor's words and only a
  write changes them.
- **The agent is read off the path:** the agent whose folder the body lies in, so a row and its body never
  disagree about whose memory it is. The folders are the agency's registration lookups.
- **A row whose body is gone goes down.** Only a write raises a row again: a piece file standing with no row,
  or with a row that is down, is somebody's leftover until its writer names it.
- **A brain that is no git repository is passed over,** its libraries' rows left exactly as they stand and the
  reason said as the library's trouble; its index files are not drawn either. Reading its bodies as gone would
  take every row down on the next start.
- **The status is settled** against the item states on every reconcile.
- **The index files are drawn again** from what the rows now say by whoever holds the brain: an editor's batch
  as it ends, or a person asking for the sync. The service's own start reconciles the rows and writes no file,
  since a batch may be running beside it.
- **A batch as it ends reaches its own agent alone:** it re-reads the bodies and settles the status of its own
  agent's rows and draws that agent's index files, and leaves every other agent's as they stand, since that
  agent's batch may be writing beside it under a lock of its own (`people-book.md`, 「One lock per agent's
  folder」).

## The shipped libraries

- **Their owners hand them in.** The unit that ships the system documents or the lore reads its own index and
  bodies and hands the brain the whole list of its entries: slug, title, path, position, the scope tag its index
  line carries, the items its title and slug name, and the dates and hash read off each body, with the word a
  screen calls its kind by. A shipped row carries no keywords, since nobody but a writer gives them.
- **The reconcile brings the table to the list:** a slug not yet in the table becomes a row, a changed title,
  path, position, date or hash updates its row, a slug that has left the list takes its row down, and a slug
  that comes back raises the row that was already there. A row taken down and raised again keeps its counts and
  who set it.
- **An empty list is a library with nothing in it** and takes its rows down. An owner that cannot read its
  index sends nothing, and the rows stand as they are.
- **The shipped rows are the chat agent's.** They describe the system the chat agent runs and its persona;
  another agent is offered one only where it is marked for every agent.
- **A library the brain writes is never handed in:** the route refuses the brain's own kinds.

## When the table is brought up to date

- When the service starts, at the end of every batch an editor commits to the brain, when a person asks for the
  sync, and for a shipped kind whenever its owner hands its entries in.
- The brain's index files are drawn at the end of an editor's batch and on a sync, not at the start.

## A standing set by hand

A person switches a fragment off or on through the console. That switch is a fact about the row rather than
about its library, so it is kept on the row and neither a reconcile nor an editor's write moves it.

- **The hand outranks the library while the two disagree.** A row switched off stays off while its library
  still offers it, and a row switched on stays on after its library has stopped offering it. Everything else
  about the row is written as always.
- **The hand is let go the moment the library comes round to it.** A row switched off whose piece is then
  removed, or whose shipped entry is dropped, and a row switched on that its library offers again, are rows the
  two agree about; the mark is cleared there and the row follows its library from then on.
- **Only a person's switch writes it.**

## The drift check

- **What it compares:** for the brain's libraries, the piece files on disk against the rows; for a shipped
  library, the entries last handed in against the rows.
- **What it reports:** a piece file the table does not offer, a row the table offers that no file stands behind,
  a row whose body has changed since the table last read it, and a row that is offered while the file it points
  at is gone.
- **A standing a person set is not drift.**
- **Titles out of shape are counted beside the drift.** Every active row of the brain's three libraries whose
  title the shape check refuses is counted per kind and listed with the reason, narrowed to one kind where the
  check names it.
- **It reports and does not repair.** The reconcile repairs. A piece file with no row is the one thing no
  reconcile can repair, since only its writer knows its title; the next run that touches it writes its row.

## Writing a row by hand

- **The write route** is how a person or a job writes a piece's row after writing its body by hand: the same
  write an editor makes through the fragment tool, held to the same checks, the index files drawn after it.
  The remove route takes down a piece whose file is gone. Editing an index file by hand writes nothing: the
  next drawing puts back what the rows say.
- **The list route** answers the rows, narrowed by a kind, a room, a person, an item or a word, and with the
  rows that are down where they are asked for, each with its keywords, items, period and status, and the
  standing a person set where one did.
