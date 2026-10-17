# Person book

## What it is

- **Per-person knowledge an agent keeps** from what it sees in conversation. Every agent has a person book of its
  own, written from its own view by its recollectors and read by its own turns alone (`agent-ownership.md`).
  This document describes one agent's book; every agent's has the same shape in its own brain folder.
- **A fact about a person any agent is better for knowing** is marked for every agent on its piece and offered to
  every agent's turns; a view or a judgement of the agent's own is not.
- **Not a persona's cast list:** a persona's cast list is basic information the owner wrote by hand and keeps
  with the persona, and the person book never touches it; the person book is what the agent fills in from
  conversation.
- **Who gets a book:** every sender in a room except the agent itself, the owner and other bots included.
- **What a book never holds:** it never copies out a message, never names or numbers another room, and never
  records credentials, phone numbers, addresses or medical details.
- **Privacy still binds:** the book is the agent's working knowledge of how to talk with each person, never
  material for answering a question about anyone's personal life.

### Two parts

- **Common:** one book per user id.
  - The same id is the same person, so what is the same in every room (personality, values, speech, habits,
    requested ways of working) is written here once.
  - Every room the person speaks in shares it.
- **Room-only:** one book per room of several.
  - What would be sensitive if said outside the room (forms of address there, relations with that room's
    people, topics raised and promises made there) stays here.
  - It is read only in that room.
- **A one-to-one keeps no room-only part.** A one-to-one, as the receiver's roads say one is, is the person
  talking with the agent alone: what the room is busy with is in the TODO and in what runs, and who the person is
  stands in the common part. So no run writes, cuts, sorts or trims a book there, and the read route hands no
  room sheet, room piece or room index there; the person's common sheet and pieces are handed as in any room.
  What an older run left under such a room's `people/` stays on disk as it stands, read by nothing.

### Two layers in each part

- **The standing layer:** a sheet attached every time the person speaks.
- **Pieces:** full texts, of which only those a turn needs are picked and attached.
- **What one turn can carry for a person:** the common sheet, that room's sheet, and pieces picked from both
  parts. The gate does the picking and the attaching; the brain offers the rows and answers the read route
  (`boundary-api.md`, 「Fragments」 and 「Brain documents」).
- **Beside them, not part of the book:** the room's standing memories, the agent's own list of what it holds
  there (`memories.md`). The room-only item Ongoing and promises is the digest's account of a person; a memory is
  the agent's working list.
- **Once per session:** each of them goes to a session in full once per version and as a one-line mark after
  that (「Loaded once per session」).

## Language

- **The whole book is written in English,** both parts and both layers.
  - Each speaker's book is loaded into the session and read again as that session's context every turn, so a
    language that carries the same meaning in fewer tokens is the right one.
  - The instructions the writing runs receive, and what they return, are English too.
- **The rules:** the runs that write the book and the sheet judgement that writes a sheet all receive the
  document-writing rule and after it the plain-writing rule this repository keeps, ahead of their own
  instructions.

### The room's own language stays as written

Translating these would take away the agent's grounds for addressing the person and answering in keeping with
their way of talking:

- forms of address, names and nicknames, room names (only in the room-only part; the common part names no room);
- examples of speech: sentence endings, verbal habits, favourite expressions;
- a short phrase quoting what the person said; copying a conversation wholesale is still not done;
- words whose meaning blurs in English: the levels of politeness, kinship forms of address, words that only make
  sense in that room.

**Measured on English:** every character-count target and ceiling below is measured on text written this way.

## Items

- **Fixed items:** the book is written in fixed items. An item is both a piece's topic and a sheet's section
  heading.
- **Beyond the fixed items:** the writer is free to add topics of its own, and a sheet may add sections after the
  fixed ones.

### Common items

| Name | Topic | Sheet | Holds |
|---|---|---|---|
| Personality | `personality` | basic, brief | who the person is |
| Values | `values` | basic, brief | what they hold important |
| Speech | `speech` | basic, brief | how they speak, and the tone they want to be spoken to in |
| Habits | `habits` | basic, brief | recurring behaviour and verbal habits |
| Working requests | `requests` | basic, brief | "do it this way from now on": how the agent works with this person |
| Told to remember | `told` | not loaded | "remember this": facts the person told about themselves |
| Interests | `interests` | not loaded | favourite topics and areas of expertise |
| Dislikes and cautions | `dislikes` | brief | tones that upset them, and how to treat them |
| Humor | `humor` | brief | how they give and take jokes, and where the line is |

### Room-only items

| Name | Topic | Sheet | Holds |
|---|---|---|---|
| Address | `address` | basic, brief | the name used in this room, and how the agent addresses them |
| Relations | `relations` | loaded | whom in this room they are close to, and how they treat each other |
| Ongoing and promises | `ongoing` | loaded | the person's own matters still open in this room: commitments, plans, agreements with others |

### Rules for items

- **The name is what the book writes.** Piece titles naming an item, and sheet section headings, use that name.
- **Basic items are brief.**
  - In a sheet, a basic item's section is one or two items of a line or two, three at most.
  - In pieces, a basic item's piece stays short too; the 1,500-character target below does not apply to it.
  - Only what needs explanation to make sense (relations, ongoing matters and promises, topics of the writer's
    own) is written at length.
- **Requests split in two.**
  - A request to change behaviour must be kept every turn, so it stands in the sheet as a working request.
  - A fact to remember is needed only when relevant, so it stands only in pieces, as told to remember.
- **Ongoing and promises holds only the person's own matters still open.**
  - What it holds: a commitment, a personal plan, an agreement with somebody, a promise not yet kept, each one
    item of a line or two, the detail in the piece of the matter itself.
  - **No work state:** a TODO item, a job, a deployment and how far it got are never written there, nor anywhere
    in the book. The TODO and what runs are their originals, and a copy in the book is stale within hours.
  - A batch that shows a matter in it finished moves that matter's items into the matter's own piece, found
    through the fragment tool or made new, so the fixed heading never carries what is done.
- **Interests and told to remember are not loaded into sheets.** They are needed only when the topic comes up,
  so they are picked as pieces.
- **Address is room-only.**
  - The same person is called differently in different rooms, and a room's nickname is that room's business.
  - The only things the same in every room are the display name and the user id, which stand in the common
    index's section heading.
  - The display name is the name the person's latest message went under, as the receiver's display names say. A
    room where the owner goes by a name of that room alone is read last, so such a name stands only for somebody
    who has no other.

## The boundary between common and room-only

- **The writer decides** which part a fact stands in.
- **When in doubt, room-only.**
  - Room talk wrongly put in common leaks into other rooms; a common fact left in room-only is merely unknown
    outside that room.
  - The two mistakes cost differently, so the tilt is set one way.

### What goes where

- **Common:** who the person is in any room, and how to work with them.
- **Room-only:** what hangs on that room's people, topics and circumstances.
  - A fact that bears a common item's name but only makes sense with that room's circumstances (an interest
    arising from that room's work, a topic not to raise in that room) stands in a room-only piece of its own
    topic.
- **The common part names no one else.**
  - No other member is named, by name or by user id.
  - No room name or chat id is written.
  - Everything between people is room-only relations.

### Enforced by code as well

- **The check:** if text written to the common part contains any of these, the run that wrote it fails and what
  it wrote is reverted:
  - the user id of anyone other than the person (an id that has stood as a sender, the agent's own included);
  - a chat id the receiver knows;
  - a room title.
  The ids and titles are the receiver's room titles and their senders (`boundary-api.md`, 「The receiver」).
- **A room title that is an agent's own label is that agent's name, not a room's,** and is not forbidden: a room
  made for a companion carries its name, and a common document names the companion as it names anyone, which
  tells nothing of the room. Which titles those are is read off the agency's registration lookups.
- **Only literal identifiers are caught.** A sentence carrying a room's circumstances without an id or room name
  passes the check; there, the writer's tilt toward room-only is what stops it.
- **Writers of the common part receive the forbidden room titles verbatim.**
  - Some room titles are everyday words, so a sentence using the word with no relation to the room is caught
    too.
  - Loosening the check would leak room names, so the check stays.
  - Instead, the prompts of runs writing the common part (the person-book writer's batches, the sort, the digest)
    carry every room title the receiver knows, with the instruction not to write those words in the common part
    but to rephrase.
  - Without that, the same sentence would repeat the same failure, and that room's backlog would stall.

## Where it lives

`<brain>` below is the agent's brain folder: the brain root itself for the chat agent, and the folder its
registration names for another agent (`agent-ownership.md`).

| Part | Sheet | Pieces | Index |
|---|---|---|---|
| Common | `<brain>/people/<userId>/sheet.md` | `<brain>/people/<userId>/chunks/c<userId>-<topic>.md` | `<brain>/people/<userId>/index.md` |
| Room-only | `<brain>/rooms/<chatId>/people/<userId>.md` | `<brain>/rooms/<chatId>/people/chunks/p<userId>-<topic>.md` | `<brain>/rooms/<chatId>/people/index.md` |

- **The brain:** the brain root is the service's configured brain folder, the library the agents' recollectors
  write: person books, the knowledge library, room rules. Procedure documents people write are in a repository's
  `docs/`, not here.
- **Its own git repository:** the brain is a git repository apart from any code's.
  - An editor commits what its batch wrote there when the batch ends; a job or a person that edits a brain
    document commits it there too.
  - It has no remote, so nothing is pushed. Its copy away from the machine is the runtime-data archive the
    deployment takes.
  - Nothing in a code checkout is the brain's, so a checkout's judgements never meet a brain document.
- **Its paths:** the code keeps a brain path relative to `<brain>`. An editor run works inside `<brain>`, so its
  prompt names the same relative paths; an answer of the service names a path relative to the brain root.

### The brain's code

- **One owner:** the brain's location and its three parts (`rooms/`, `people/`, `knowledge/`) are defined in one
  module of this repository; everything that reads or writes the brain asks it.
- **Where it resolves:** at the configured brain folder, so a test or a bench that names another folder sees the
  brain there.
- **Made on first use, except the live one:** a brain folder that holds no repository gets an empty one the first
  time an editor needs one, for a test, a bench or a fresh copy.
  - The live brain is the exception. It is never made again from nothing: an editor that finds no repository
    there refuses `no-brain` and says so, since an empty brain committed over a lost one is a history nobody can
    tell from the real one.
  - That brain comes back from the runtime-data archive; a machine that truly has none gets one from a person
    running `git init` there.
  - A brain that is no repository is not an empty one either: the fragment reconcile passes over its three
    libraries and leaves their rows as they stand (`fragments.md`, 「The reconcile」).

### Slugs

- **The first letter names the part:** `c` for common pieces, `p` for room pieces.
- **The rest:** the user id, a hyphen, and a topic of lowercase Latin letters, digits and hyphens.
- **Fixed items** use the topic from the tables above (`c1001-speech`, `p1002-address`).
- **Pieces of the writer's own topics** use the subject a seeker would name as the topic.
- **Room pieces made by cutting** carry a span number before the topic (「Cutting a book into pieces」).

### Indexes

- **The rows are the index.** Every piece is a fragment row, written by the run that wrote the piece through the
  fragment tool (`fragments.md`). The row carries the title, the keywords, the items, the dates and the status a
  search and a writer look the piece up by.
- **The index files are drawn from the rows** after every batch, for a person and a session to open:
  - **Common index:** one per person, with a single section, `## <userId> — <display name>`.
  - **Room index:** one per room, with one section per person, `## <userId> — <display name>`.
  - **Piece lines,** in both: `- <slug> — <title> · <period>`, the period being the months the piece's item
    dates span, read off the body; a piece with no dated item has none.
  - Nothing reads them back, and an edit made to one by hand is overwritten at the next drawing.
- **The title:** one or two sentences saying which piece of whose book this is, or what matter it holds, and the
  turns that need it: English, with the room's own words kept in the original, and never a list of nouns or a
  line hung on the words every turn carries (`fragments.md`, 「The shape a title must have」).
- **The keywords:** the words particular to the piece that somebody asks for it by: above all the words the room
  itself uses for the matter, and the names, products and identifiers it is about.
- **No row, no piece:** a piece with no row can never be offered, so a run that leaves a piece it wrote without
  its row fails.

## Item shape: facts, views, judgements

### A list, not paragraphs

- **Every part and layer is a list.**
- **One item, one thing:** each item holds one fact, one view or one judgement in a sentence or two. Two facts
  are never joined in one item.
- **No transcripts:** conversation is never copied verbatim.

### The piece shape

A piece is, in order:

1. a title line: `# ` then the display name; in brackets, for a room piece that room's forms of address and other
   names, then the user id; then a dash and the item names or the topic nouns a seeker would use;
2. `## Facts`, its items each opening `[date D · confirmed D]`: what happened as seen;
3. `## Views`, its items each opening `[date D · confirmed D]`: a thought, preference or outlook the person
   stated, as theirs;
4. `## Judgements`, its items each opening `[basis D · confirmed D]`: a tendency the agent read, or how to treat
   them.

- **Terms used below:** "facts", "views" and "judgements" mean those three lists; "date", "confirmed" and "basis"
  mean those slots of the bracket.
- **Common piece titles carry no form of address;** address belongs to rooms.
  - The name in a common piece's title and in the common index's section heading is the person's display name,
    which the code reads from the receiver's messages and hands to the writer.
  - The writer does not carry over the name a room piece uses there.
  - When touching a common piece with another name, the writer writes the title again with the display name.
- **Fixed order:** sections come in fixed order, and a section with no items is not written.
- **Nothing else:** nothing stands outside the title and the three lists.

### Opinions split by whose they are

- **Two kinds:** there are prospects or preferences the person stated, and judgements the agent drew from reading
  the conversation.
- **Why they are kept apart:**
  - Putting the first among facts would read the person's prospect as confirmed.
  - Putting it among judgements would read the agent's thought as the person's words.
- **The three lists:**
  - **Fact:** what happened as seen: what was done, decided, owned, promised.
  - **View:** what the person said they think, like, or expect to happen.
  - **Judgement:** what the agent read from facts and views.

### The date bracket opens every item

Two slots, both always written, each under its name. The dates of a piece's row are read off the named slots
alone, so the fragment tool refuses a piece whose bracket leaves the names out (`fragments.md`).

- **Date:** the day it happened or was said.
  - In a judgement this slot is the basis: the period the dates of the items it rests on span.
- **Confirmed:** the latest day that supported this item.
  - When the same thing is seen again later, confirmed moves forward and the date stays.
  - When new evidence contradicts an item, the item is replaced whole with new dates, and the old form is not
    recorded.
- **Format:**
  - Dates are `YYYY-MM-DD` in Korean time.
  - A recurring thing is `YYYY-MM-DD~YYYY-MM-DD`.
  - Unknown is `unknown`.
- **A writer never writes a date not in its input.** Dates may come only from message times, dates the text
  itself states, dates of items already standing, and the record date the code hands over.
- **If none applies,** the date is `unknown`.

### Carrying old pieces over

- **Prose pieces are converted whole.** Touching a prose piece converts the whole piece into items, so paragraphs
  and items never mix in one piece. No sentence is dropped in the conversion.
- **Pieces in another language are translated whole.** Touching a piece written in another language carries it
  into English by the same rule.
- **Missing dates:** an item from a sentence with no date has `unknown` for both date and confirmed.

### Piece size

- **Target:** a non-basic piece aims at 1,500 to 3,000 characters.
- **Past that, split:** a piece that grows beyond is two subjects, so it is split in two.
- **Brackets count:** date brackets count toward piece and sheet character counts.
- **Fixed headings have a ceiling of 6,000 characters.** Every batch is handed each of its people's
  fixed-heading pieces whole (「The writer's prompt carries what it reads」), so a fixed heading is paid for in
  every batch of its room, and one that only grows soon outweighs the batch itself. The writer aims a fixed
  heading at the same target as any piece, a basic one shorter; what stands past the ceiling is trimmed by the
  sort (「Trimming a fixed heading」).

## The standing layer: sheets

### Sections

- **A sheet's sections are its items.**
- **Order:**
  - Common: `## Personality`, `## Values`, `## Speech`, `## Habits`, `## Working requests`,
    `## Dislikes and cautions`, `## Humor`.
  - Room: `## Address`, `## Relations`, `## Ongoing and promises`.
  - A section with nothing to write is left out.
- **Extra sections:** the sheet judgement may add sections after the fixed ones.
- **Inside a section:** a list of English items.

### Items

- **The kind comes first.** Each item marks, at the very front of its bracket, whether it is a fact, a view or a
  judgement: `fact`, `view`, `judgement`. A piece carries the kind as the heading of its list and its bracket
  holds dates alone; a sheet's sections are its subjects, so the kind moves into the bracket.
- **Which dates:**
  - Ongoing matters and promises carry the day it happened too, as `[fact · date D · confirmed D]`.
  - Other sections carry only the confirmed day, as `[fact · confirmed D]`.
  - A judgement has `basis` in the date slot.
- **Order within a section:** facts, views, judgements.

### Where its content comes from

- **A sheet is the gate's sheet judgement,** asked by the brain with that part's pieces and the part's ceiling
  (`boundary-api.md`, 「The gate」). It is a tool-less request that writes a sheet from scratch.
- **Every date in a sheet is copied from pieces.**
  - The judgement receives only that part's pieces, so the only dates it can know are the ones written there.
  - If its answer contains any date found in none of the pieces it received, the answer is not used and the
    existing sheet stands.
- **Only what is still going on.** A piece whose matter is done (every item it is tied to finished,
  `fragments.md`) is not read into a sheet. A sheet stands in front of every turn, and a finished matter is found
  by the search on a turn that asks for it; it also keeps a large book's sheet to a few requests rather than
  dozens.
- **Only its own part's pieces.**
  - The common sheet receives only the person's common pieces; the room sheet only that person's pieces in that
    room.
  - So there is no way for what is in room pieces to move into the common sheet.

### Length

- **Targets:** common 3,000 to 5,000 characters, room 1,600 to 4,000.
- **No padding:** a person with little to keep gets a short sheet; padding to reach a number is something the
  judgement is set not to do.
- **Ceilings:** common 8,000, room 6,000. A sheet over its ceiling is not fixed, and not used.

### What the read route hands

- **Sheets for the people who spoke:** the read route takes the turn's senders and answers, per sender, the
  common sheet and that room's sheet, the room's only where it keeps a room-only part.
- **Only when the file is under the ceiling.** An uncut full document may still lie in the room-only place, so a
  file longer than the ceiling is taken as no sheet and left out (「People whose book is not yet cut」).
- **When and how a sheet is attached is the gate's:** the gate attaches a speaker's sheets on every turn, even
  where its own picking answered nothing.

## Pieces: what a turn may be offered

- **The pieces a room may be offered:** that room's room part, where it keeps one, and the common part of the
  people who have spoken in that room, as the receiver says.
- **Who has spoken** is decided by whether the person has stood as a sender among that room's messages. So a
  person who has never appeared in a room does not even show up by name in what that room is offered.
- **The narrowing and the picking are the gate's.** The brain answers the rows with their titles, keywords and
  items; a row's words decide whether a piece can be found: its keywords and items whether the narrowing keeps
  it, its title whether a pick takes it.

### The read route checks the boundary again

- **Room pieces:** a slug starting with `p` answers only to a row of the room asked for, whose piece stands under
  that room's `people/chunks/`.
- **Common pieces:** a slug starting with `c` answers only to a common row of a person who has spoken in that
  room, whose piece stands under `<brain>/people/<userId>/chunks/`.
- **Anything else:** a slug of any other shape is refused `out-of-room`.
- **No road, not a filter:** another room's pieces are not attached because no row of theirs is ever offered to
  this room, not because they are filtered out.

## Loaded once per session

- **Every document of the book a session is handed** (sheets, pieces, and indexes handed in place of pieces)
  goes to a session in full once per version and as a one-line mark after that. The brain keeps what each
  session holds through the session-load routes; the gate asks them and decides the form (`boundary-api.md`,
  「Session document loads」).
- **What the book adds to the mark:** the user id whose document it is. An index names nobody.
- **Room-only documents stand only on that room's turns,** full or mark, even when every room shares one
  session; a common document is one version in every room the person spoke in, so a common sheet loaded in one
  room stands as a mark on the same person's turn in another.

## Who writes what

- **One writer per document, and every run for one agent.** A run reads the rooms that agent answers and writes
  only into that agent's book, from its view; a room several agents answer is read once for each of them
  (「Each agent's run」).
  - **The person-book writer** writes both parts' pieces and their rows. It is a recollector session with file
    tools, on the `people-digest` work key.
  - **The sheet judgement** writes both parts' sheets, on the `people-standing` work key, looking only at that
    person's just-written pieces of that part.
  - **The digest** writes working requests and told-to-remember facts from notes into the person's common
    pieces (`knowledge.md`). So that two runs never write the same piece at once, the digest takes the same
    agent's editing lock as the person book (「One lock per agent's folder」).

### Each agent's run

- **A run names its agent.** Every form of the people route takes `agent`, and works in that agent's brain
  folder alone: the writer stands in the folder and sees every path relative to it (`fragments.md`), and the run
  reads and writes only that agent's rows.
- **The rooms it reads are the rooms the agent answers,** by the receiver's room answerers; a room with no
  answerers is the chat agent's. A one-to-one is never among them (「Two parts」): the launcher passes it by, a
  room's own run is refused `one-to-one`, and a history walk reaching the top of one starts nothing.
- **Its own words are not a person.** The sender the agent's own words stand under in the room, as the
  receiver's own number answers it, is left out of the book, and none is left out where it answers none; any
  other speaker is written as the room shows them.
- **What it has read is its own.** Each agent's read marks are the receiver's, one per agent and message, so a
  message one agent has read stays unread for another until that agent's run reads it (`boundary-api.md`, 「The
  receiver」, unread messages and read marks).
- **One launcher, a worker per agent and room.** The launcher goes through every agent's rooms in turn. A worker
  is the unit `bmo-people-digest-<agent>-<chatId>`, the mutex for one agent's book of one room, and the sort
  runs as `bmo-people-sort-<agent>`, one per agent. Units are launched through the agency's unit launch.

### The launcher and the worker

- **The work is the people route,** the launcher when it names no room and the worker for one room when it does.
- **What starts it:**
  - the brain's clock launches the launcher every three hours as `bmo-people-digest`;
  - a room whose session is about to fold launches its own worker first through the before-fold route, so what
    the session is about to forget is written down before it goes;
  - a finished history walk of a room asks for that room's run through the people route;
  - a rest, as the receiver's rest state says, stops either launch before any session is started.
- **The launcher takes every room holding at least ten unread messages** for the agent and runs their workers
  one after another, waiting for each, so two writer sessions never run at once.
- **Smallest backlog first:** every room's book is current soon, and the room with years of history takes its
  long turn last.
- **Catch-up (`all`):** a room with a whole batch or more waiting (a backfilled history, a busy week) is worked
  batch after batch from the oldest forward until the backlog it started with is folded, pausing only for a rest
  or for a pause the owner set, so the documents are built from the deepest messages up to the newest in order.
- **Batches:** the worker takes the oldest unread messages in batches of at most 400 and hands each batch to one
  recollector session of the `people-digest` work key, file tools only, its task the people prompt this
  repository keeps.
- **A run plans its batches as it starts.** The worker counts the room's unread messages once, before its first
  batch, and works that many batches of 400 and no more: at most ten, or as many as the backlog makes in
  catch-up.
  - Messages that arrive while it runs ride along only where a planned batch still has room; the rest wait for
    the room's next run, so a room whose conversation keeps going is followed batch by batch and then let go,
    never chased.
  - Each batch's run carries which batch it is of how many, which the agency's live picture shows.
- **What the clock covers:** one writer run, one batch or one group of a batch, carries the editor's cap the
  agency's work key gives, thirty minutes or the registration's request wait where that is longer, and the
  agency's overrun ask is made before it is cut there. A room's run as a whole carries no clock of its own: it
  is bounded by the batches it planned, each under its cap.
- **Which messages are folded in is marked** through the receiver's read marks, so a message is read into the
  book once and no run reads again what an earlier one handled.
- **The owner's pause:** the pause route pauses both digests for a length, `on` until told otherwise, `off`
  resumes, `status` shows it. The batch in a run finishes and is marked as usual, no new batch starts while the
  pause holds, and the next run continues from the first unread message, so a pause costs nothing but time.
- **A companion's reset holds its writer off** (「One lock per agent's folder」; `boundary-api.md`, 「Agents
  forgetting」): it stops that companion's open runs, puts back what they left unwritten in its folder, and
  holds the companion's editing lock until its rows and files are gone; a run of that companion waiting on the
  lock goes on once it is let go, and another agent's runs neither wait on the reset nor hold it up.
- **When the third run has failed too** (「A failed batch is tried again before the room stops」), the messages
  are left for a later run and the owner is told which room's person book failed and why through the receiver's
  owner notice, with `people-digest-failures.log` under the data root as the last resort when even that notice
  cannot be sent.
- **After a successful batch** the digest commits that room's people directory and its senders' common books,
  nothing beside them, and the sessions concerned are told to read what changed again (「Who reads a changed
  document again」).

### The writer's prompt carries what it reads

- **What the code loads once into the prompt:** whole, the pieces of the batch's people the table picks for this
  batch: every fixed-heading piece, and every piece tied to an item the batch's messages name or carrying a
  keyword they use.
- **One person's pieces stay under the run's ceiling:** the picked pieces of one person are handed while they
  fit the documents ceiling of a run, the fixed-heading pieces first and then the ones the batch's words picked,
  each in the order their rows stand; a piece that does not fit what is left is not handed whole and is reached
  like any piece not picked.
  - The prompt goes to the agency as one request, which has a size cap, so a batch handed a person's whole book
    would never become a turn and its messages would never be read.
  - The pieces left out are named in the run's log, by path and size.
- **What the writer asks for itself:** every other piece of those people, both parts, through the fragment tool's
  search: slug, path, title, keywords, items, period, status. No index is written into the prompt: a book of
  hundreds of pieces would be paid for in every call of every run.
- **No rereading:** the writer does not read what it was handed again with file tools. A piece that was not
  handed over whole is opened only where its row shows it holds the matter the batch is about.
- **One matter, one piece:** evidence for a matter a piece already holds goes into that piece and its row; a new
  piece is for a matter no row covers, and the tool refuses a new piece that repeats one (`fragments.md`,
  「Finding a piece before writing one」).
- **Edits:** a piece to change is changed in one call, and that call is the smallest correct one. If the piece
  still stands, only the changed items are replaced; only a new piece, or one split wholesale, is written whole.
- **Why so frugal:** a run with file tools reads its whole context again on every tool call, and the text sent
  in a tool call is text the run wrote. So a run's cost is its context size times its number of tool calls, plus
  the size of what it wrote, and reads, piecemeal edits and wholesale rewrites all inflate it.
- **Splitting a batch:**
  - When the documents to load exceed a run's ceiling in characters, the batch splits into groups of people.
  - Each group gets a run carrying the same full messages plus only that group's documents.
  - A group's run may write only that group's documents.

### A batch writes both parts together

- **Merging by the boundary:** the writer given a room's batch searches both parts and sees the pieces handed
  over whole, and merges new evidence into the two parts by the boundary.
- **Nothing copied down:** what it read in common pieces is not copied into that room's pieces; what is in common
  is already visible in every room.
- **Touched room pieces get sorted:** a room piece the batch touched is sorted whole by the boundary, and becomes
  a sorted piece (「Sorting pieces」).
- **While the book is being cut:**
  - If the person's book in that room is still being cut, the writer does not sort the person's room pieces; it
    only merges into them.
  - It does not overwrite the file in the room sheet place either.
  - Cutting judges itself by whether its pieces are unchanged, and that file is still the original to cut.

### Whether a book is being cut is told by the disk

- **Being cut** means all three hold:
  1. the file in the room-only sheet place exceeds the room sheet ceiling;
  2. some of the person's room pieces carry a span number;
  3. splitting that file into spans leaves a span with no piece.
- **New pieces do not end it:** a batch writing unnumbered pieces meanwhile does not end the cutting.
- **Cut at once:** a person with pieces but no numbered ones at all was cut in one go.
- **Not size alone:** a sheet slightly over the ceiling may stand on a book already fully cut. That file splits
  into one span whose first span has pieces, so it is not being cut.

### The writer writes only its places

- **The allowed places:** a batch may write only that room's `people/` and the batch people's
  `<brain>/people/<userId>/`.
- **When a batch fails:** after the run, its own file tools wrote a document outside its places, or the common
  part holds an id or room title the boundary forbids. Then the batch fails, what the run wrote is reverted, and
  the messages are not marked as read.
- **What is compared:** the brain's documents (`<brain>/rooms/`, `<brain>/people/`, `<brain>/knowledge/`) as they
  stood when the run began, against how they stand after it.
- **What is the run's:**
  - **Inside its places, everything that changed.** The places are reserved for the run while it runs, and what
    changed there is its model's writing and its code's alike: the sheets, the sorted list.
  - **Outside them, only a document its own file tools named.** The agency's wait answers the files the
    session's file-writing calls named, and only such a document counts as the run's writing out of place.
  - **Anything else changed outside its places is another writer's:** an agent editing the knowledge or a room's
    rules by hand, a job's commit. It neither fails the batch nor is put back; the run's log names it and the
    batch leaves it as it stands, and does not commit it.
  - A write made through a shell, or on a road whose stream names no tool call, names no file, so outside the
    places it is counted as another writer's.
- **What is reverted:** the run's documents alone, back to how they stood when the run began.
- **Who else stands in a run's places:**
  - Within one agent's folder the two editors never run at once: both hold that folder's editing lock for as
    long as a batch runs. Another agent's batch may run beside it, writing only in its own folder, which the
    comparison never reads.
  - A job never writes beside a batch either (「A job editing the brain holds the editors off」).
  - An agent's own hand edit is held off by neither, so one made inside a running batch's places is counted as
    the run's and goes back with it on a failure; outside those places it is never touched.

### A failed batch is tried again before the room stops

- **Three tries:** a batch that fails (no success marker, a write out of place, a run that errored) is put back
  and the same messages are handed to a fresh run, up to three runs in all, a minute apart.
- **Only the last failure stops the room** and tells the owner, with the reason and the number of tries. A
  catch-up therefore keeps going past one bad run instead of ending and waiting for the next scheduled launch.
- **A missing marker is not trusted.** An answer that summarises its work but lacks the marker is a failure like
  any other: a run cut off halfway lacks the marker too, and nothing on disk tells the two apart, so what it
  wrote is put back and the batch runs again.
- **What stops without a retry:** a usage limit (the rest begins), a rest or a digest pause found before the next
  try, and a failure after the batch was marked (cleaning or committing what it wrote), since its messages are no
  longer the ones to read again.

### A job editing the brain holds the editors off

- **The hold:** the first time a job writes into the brain, the job's own guard sets a brain hold for its run
  through the hold route (`boundary-api.md`, 「Brain holds」). The brain keeps it as one settings key per run.
- **What it holds:** while any hold stands, no brain editor starts a batch: the notes digest, a room's
  person-book batch, the sort, a cut, a sheet rewrite, a reform. Each waits, looking again every half minute, and
  begins as soon as the hold is gone; it takes no lock while it waits.
- **When it ends:** a hold lasts as long as its job's run is open, as the agency's run open says, and is cleared
  by the first editor that finds the run finished, so the editors resume on their own when the job ends or is
  stopped.
- **The job's own runs:** a brain editor started by the holding job itself, a cut the job runs by hand, names
  that run and is not held by its hold.
- **A batch already running:** setting a hold also tries every agent's editing lock without waiting and answers
  the ones taken. Where one is, the job is refused its edit by its own guard and waits through the wait route,
  which answers once every lock is free (at most nine minutes a call). The hold is already set, so no new batch
  starts behind the running one.
- **Apart from the owner's pause:** the hold is its own record. Lifting the owner's pause does not lift a hold, a
  hold ending does not lift the pause, and the hold says nothing to the owner.
- **Reading is free:** reading the brain sets no hold.
- The hold route's read answers which runs hold the brain and which editing locks are taken.

### Writer and sheet judgement run in turn within a batch

- **Order:** after the writer folds pieces, the sheet judgement is asked once per part for each person the batch
  touched: the people one after another, and each person's room sheet and common sheet side by side.
- **One commit:** both parts go out in one commit.
- **No history:** sheets are written again each time, so they have none of their own.

#### One lock per agent's folder

- **Every writer of one agent's brain folder shares one editing lock,** so two rooms' batches of that agent never
  write one person's common part at once: its person-book batches, its sort, a cut, a sheet rewrite, a reform,
  its notes digest, and a companion's reset.
- **The file is named after the folder,** under the data root: the chat agent's, at the brain root, is
  `people-digest.lock`, and another agent's is `people-digest.<folder>.lock`, the folder percent-encoded as a URI
  component (`people-digest.agents%2Fagent-b.lock`).
- **Two folders' writers never wait on each other,** since nothing one compares or puts back is the other's:
  - every agent's folder stands apart from the others' library directories, a companion's under its own folder
    and the chat agent's at the root;
  - a batch compares and puts back only its own folder's documents (「The writer writes only its places」), and
    commits only the paths it wrote;
  - as it ends it reads again only its own agent's rows and draws only its own agent's index files
    (`fragments.md`, 「The reconcile」);
  - so a companion's batch or reset goes on while the chat agent's runs queue, and the other way round.
- **Git's index lock is the one thing they still share:** a commit that meets another folder's commit tries
  again after a pause.
- **A fenced folder is not written by what came before the fence:** a companion's reset puts a fence on its
  folder as it begins and again as it ends, the moment of it kept in a file beside the lock, and a writer whose
  process began before that moment is refused the folder's lock from then on and stops, so nothing it read before
  the reset is written into the folder after it.
- **A removed companion's lock goes with it:** the lock file, its queue and its fence are removed through the
  agent-removed route once the removal has taken the companion's folder and every writer of it.
- **How long each holds it:**
  - A batch folding messages takes and releases the lock per batch. Catching up can take days, and the digest
    must be able to write notes into common pieces in between.
  - Cutting, writing sheets again and reforming hold the lock for the whole run.

#### The lock passes in the order waiting began

- **Tickets:** each waiting run sets a ticket in the queue beside the lock, and takes the lock only after every
  earlier ticket is gone.
- **Taking it again goes to the back:** a run that takes the lock again right after releasing it, for the next
  batch or group, sets a new ticket at the end of the queue. So it cannot jump ahead of a digest or another
  room's batch that was waiting first.
- **Why tickets:** with a bare lock, who takes it at the moment of release is luck, and a catch-up running batch
  after batch without pause could keep other runs out until their wait ran out.
- **Stale tickets:** the ticket of a run that died holding one is cleared by the next run that looks at the
  queue.
- **A companion's reset goes first:** its ticket is a front ticket, ahead of every waiting run, and the run still
  holding the lock is stopped rather than waited for.
- **A wait follows a keep:** each ticket carries the name of its run. A waiting run whose wait runs out while the
  run ahead of it was kept past its editor's clock by the overrun ask goes on waiting, keeping its place, until
  the keep ends and a slack after it. Behind a cut, a sheet rewrite or a reform, which hold the lock from outside
  the queue, the wait ends at its own length.

### Records

- **The sheet judgement concerns no message,** so it records neither judgements nor run counts; what it judged
  is in the written sheet itself.
- **Models:** the writer runs on the `people-digest` work key and the sheet judgement on `people-standing`, each
  resolved by the agency. A companion that names its recollectors' model has its writer and sheet runs go out on
  that registration instead, whatever the two keys say (`agent-ownership.md`, 「The model each recollector runs
  on」).

## Who reads a changed document again

- **Room-only documents:** when one changes, that room's sessions are told.
- **Common documents:** when one changes (anything under `<brain>/people/<userId>/`, the index excepted), the
  sessions of the rooms the person has spoken in are told.
- **Beside the book, the same rule:** a room's rule chunk tells that room's sessions; a knowledge chunk tells the
  sessions of the room its row is scoped to, or of every room the receiver knows where its row is shared; the
  knowledge index and a document no row stands for (the candidates) tell nobody.
- **A notice names the path and carries no text,** and before it goes out the path is forgotten in every session
  holding it (`boundary-api.md`, 「Session document loads」). The notice is the agency's document reload notice,
  given the agent, the paths and the rooms; the agency reaches each session once, however many of the rooms
  share it.
- **Sanitising:** before entering a session, both parts' documents pass the gate's document sanitiser.

## People whose book is not yet cut

- **A person with no pieces has no fragment row.** Their whole document stands at the room sheet place, and
  nothing but its size decides whether a turn reads it.
- **Room-only only:** this place exists only in the room-only part. The common part always stands as pieces and a
  sheet only.
- **The file's size decides where it stands:** under the room sheet ceiling it is a sheet-sized text in the sheet
  place, so it stands as the room sheet; over the ceiling it is not handed, and until it is cut that person's
  room-only knowledge never reaches a turn.
- **Half-cut people** are measured by the same rule: the full text is still in the sheet place, so that file is
  judged by its size, and pieces with rows can be offered.

## Cutting a book into pieces

A verb of its own, the people route with `split` for a room, optionally one person and a bound on spans: cutting
a room-only book already written as one document into pieces.

### One run cuts one span of one person

- **Spans:** the document is split at paragraph and sentence boundaries into spans of about 8,000 characters.
- **One span per prompt:** one span's text goes whole into the prompt, and only that span is written as pieces.
- **Long paragraphs:** a paragraph longer than a span is cut at its sentence boundaries.
- **The original stays closed:** the writer does not open the original document; the span it received is all
  that run knows.

### Cutting writes room-only pieces and does not sort

- **All room pieces:** pieces from a span are all that room's pieces, not yet sorted.
- **Sorting comes later** (「Sorting pieces」).
- **Why:** a cutting run seeing only one span's sentences would judge commonness without knowing the person's
  other spans or other rooms.

### Spans have no dates, so cut items are dated `unknown`

- **Date:** only dates the span's text states itself become dates.
- **Confirmed:** the day the document was last committed. If git does not know it, confirmed is `unknown` too.

### The span number goes into the piece name

- **The slug:** a piece from the nth span is `p<userId>-<two-digit number>-<topic>`.
- **Progress lives on disk:** the pieces there say how far cutting got; the first span with no piece is the next
  span to cut.
- **Resuming:** an interrupted run continues when called again.
- **Batching:** `spans` bounds how many spans one run takes.
- **Skipped:** a person who already has unnumbered pieces was cut in one go, and is skipped.

### Fidelity is measured per span, against that span

- **The floor:** if one span's pieces come to under 85% of that span, only that span's pieces are deleted and
  cutting stops at that person.
- **Hangul counts double:** in measuring, one Hangul syllable counts as two characters. The span is a document in
  the room's language and the pieces are English; the same words run to a bit over twice as many characters in
  English, so a raw count would let a piece that dropped half pass the floor.
- **Only its own pieces:** a run can touch no piece other than those it wrote. If an earlier span's pieces have
  changed, that span's result is discarded.
- **The original:** no span touches the original document.
- **A span whose pieces have no rows is not finished.**

### Finishing the book

- **When the last span stands,** the sheet judgement writes the room sheet over that document's place.
- **Too many pieces for one pass:** the pieces are split into groups, one sheet is asked per group, and those
  sheets are handed again to make one.
  - The groups are asked side by side, at most four at once for one sheet and so eight for a person's two, so a
    large book's sheet takes about as long as its slowest group and the fold. Each request takes a minute or
    more, and the batch holds its folder's lock throughout, so requests asked one after another would hold every
    other writer of that folder off for the sum of them.
  - A group that produces no sheet is asked once more.
  - If it still fails, no new sheet is written and the existing sheet stays. Writing from the other groups only
    would silently drop that group's pieces from the sheet.
- **`sheets`** writes both parts' sheets again for every person of that room without cutting.

### It commits what stands

- **When the run ends,** what stands of its writing is committed into the brain repository in one commit:
  - a cut: the room's `people/`, under "Room <chatId>'s person book … is cut into pieces";
  - a sheet rewrite: the room's `people/` and the common books of that room's people, under "Room <chatId>'s
    standing sheets are written again from the pieces".
- **What failed is not in it:** a span whose pieces were dropped was put back before the commit, so the commit
  holds only the spans that stand. A run that left nothing standing makes no commit.
- **Reviewing a result** is reading that commit, and undoing it is reverting it in the brain repository.
- **Inside sorting:** when the sort cuts a book first, the sort's own finish commits, so the cut makes no commit
  of its own there.

### Not held by the digest pause

- **This verb ignores the digest pause.** It neither reads messages nor marks them, so it moves no backlog at
  all.
- **A job's brain hold holds it** like every editor.

## Sorting pieces

A verb of its own, the people route with `sort` for one person, for `all`, or for `status`, optionally bounded by
`groups`: sorting unsorted room pieces into common and room-only.

### Which room pieces are sorted: `people/sorted.txt`

- **The file:** each room's `people/sorted.txt`, one slug per line, written only by the code.
- **What goes on it:** room pieces a batch touched, room pieces this verb sorted, and room pieces it newly wrote
  while sorting.
- **Anything else** is an unsorted piece.
- **Unsorted pieces stand as room-only** until sorted. Nothing leaks outside the room; the person's common
  knowledge simply has not reached other rooms yet. So the book is in a correct state wherever sorting stops.

### One run sorts one group of one person's pieces in one room

- **Grouping:** that person's unsorted pieces in one room are grouped in the order their rows stand, about 8,000
  characters each, and one group's piece texts go whole into the prompt.
- **Sorting:** the writer finds the person's common pieces through the fragment tool, and for each sentence of
  the group either moves it to a common piece or leaves it in the room piece by the boundary.
- **Prose is converted:** prose pieces are converted whole into items at this point.
- **What becomes of room pieces:** a room piece is written again with the items that remain; a room piece with
  nothing left is removed through the fragment tool, which deletes its file and takes its row down.
- **Duplicates fold:** an item equal to one already standing in a common piece folds into that item, and
  confirmed moves forward.
- **Stay inside the group:** the writer does not touch room pieces outside the group; if it does, that group
  fails.

### A person's room is sorted after its book is fully cut

- **Cut first:** if the person's book in that room is still being cut, or stands as a sheet with no pieces, this
  verb first cuts it span by span as 「Cutting a book into pieces」 does, and sorts only once it is fully cut.
- **Why:** cutting judges its result by whether earlier spans' pieces are unchanged, and reads the first span
  without pieces as the next span. Sorting pieces mid-cut would make it cut spans already cut.
- **On a cutting failure,** it stops at that person's room.

### Fidelity is measured per group, against that group

- **What is counted:** the characters of items and sentences, excluding heading lines, blank lines, list marks
  and date brackets, Hangul counting double as in cutting. An emptied room piece losing its title, or a sentence
  gaining a bracket as it becomes an item, is not something said; neither may pass as a kept sentence or hide a
  dropped one.
- **The 80% floor:** against the group's count, the sum of the characters of the room pieces written again, the
  characters the common pieces grew by, and the characters of common items that already stood and in this run
  changed only their date bracket with their text unchanged. Below 80%, the group's result is reverted and
  sorting stops at that person.
- **Why lower than cutting's 85%:** sorting converts paragraph-written pieces into items, and even converting
  without dropping a sentence loses the connecting words between sentences, which measures around 85%. A summary
  drops to about half that, so 80% still tells them apart.
- **Only what can be measured counts:** a run's word that it folded something is not counted, so a run that
  dropped sentences and answered that it folded them cannot pass the floor. A fold whose bracket stayed unchanged
  because no confirmed date came along is not counted either, so the count can only err toward failure.
- **Other reverts:** the group is reverted in the same way if the common part holds an id or room title the
  boundary forbids, anything was written outside that room's `people/` and the person's common part, or a piece
  it wrote is left without its row.

### A group that stands is finished on the spot

- **The finish:** its pieces go onto `sorted.txt`; both of the person's sheets are written again; the written
  documents pass the sanitiser and the sessions holding them are told; the result is committed into the brain
  repository per group.
- **No pieces left:** a person with no room pieces left has their room sheet deleted too. A sheet standing with no
  pieces would read as a sheet-only person and be cut again.
- **Resuming:** an interrupted run, called again, continues from the unsorted pieces.

### A group that fails twice is skipped until its pieces change

- **The fingerprint:** a failed group is recorded in the brain's settings as a failure count under a fingerprint
  made from its piece slugs and text.
- **First and second failure:** a first failure is tried again on the next run; when a group with the same
  fingerprint fails a second time, that person's room is skipped.
- **Telling the owner:** a single notice goes to the owner through the receiver's owner notice, never to a room,
  with the person, the room and the reason.
- **Changed pieces:** if the pieces change, the fingerprint changes, and the group is tried again as a new group.
  When a group stands, that fingerprint's failure record is deleted.
- **Cutting too:** cutting a book that must be cut first follows the same rule, with a fingerprint made from that
  place's file and the piece list.
- **Why skip:** otherwise the sort that starts every three hours would spend an editing run on the same failure
  each time, leaving only a log line.
- **Reporting:** `status` names the skipped people's rooms separately, and the sort spends no run on a person
  with only skipped work left.

### Oversized room sheets are written again before sorting

- **The problem:** a room sheet longer than its ceiling is not handed, so a person who already has pieces and an
  oversized sheet speaks without a sheet on every turn until sorting ends.
- **So first:** before running any group, the sort writes again both sheets of people in that room who have
  pieces, are not being cut, and have a file in the room sheet place over the ceiling.
- **Light work:** only the sheet judgement is asked. The lock is taken and released per sheet; written sheets go
  through the sanitiser and the reload notice, and are committed.
- **Books being cut are left alone:** their file is the original to cut.
- **Failures:** the judgement failing to produce a sheet within the ceiling counts as a failure too. A second
  failure on the same file and pieces stops writing that sheet until the pieces or the file change, with one
  owner notice.
- **The sort starts here:** it does this first every time it starts. Even with no pieces to sort, if such sheets
  remain, it has work.
- **Reporting:** `status` also reports oversized sheets per room.

### Sorting runs as its own unit

- **Launching:** when the brain's clock starts the launcher every three hours, it also starts the sort with `all`
  for every agent as its `bmo-people-sort-<agent>` unit, unless that unit is already running.
- **Why not the launcher:** during catch-up the launcher can run for days, waiting on room workers one by one,
  and meanwhile no new launcher can start, so sorting stopped by a rest or a lock wait could not restart either.
- **Nothing to do:** if there is no fixed heading to trim, no unsorted room piece, no book to cut first and no
  oversized sheet, the unit ends at once with no run.
- **The order of its work:** the fixed headings to trim first, since every batch carries them (「Trimming a fixed
  heading」); then the oversized sheets; then the spans and groups.
- **`all`:** processes stretches, spans and groups per person, and per room within a person. It takes and
  releases the lock per stretch, span or group, so batches can slip in between.
- **A rest:** when one begins, no next group is started, and it stops.
- **Bounding a run:** where the caller must finish before a run would, `groups` bounds how many stretches, spans
  and groups one run takes.
- **Progress:** `status` reports, per person and per room, the fixed headings being trimmed or over their
  ceiling, the remaining pieces and the books to cut first. Each room's `sorted.txt` and the commit history say
  the same.
- **Not held by the digest pause;** held by a job's brain hold.

## Trimming a fixed heading

- **The work of the sort, done before anything else it does:** a fixed-heading piece past its 6,000-character
  ceiling, common or room-only, is brought back under it.
- **What stays in a fixed heading** is what every batch needs whole:
  - Ongoing and promises: the person's own matters still open, one item of a line or two each.
  - Every other heading: the person's lasting pattern, in a few items.
  - Everything else moves into the piece of its own matter or topic in the same part: a finished matter, an
    episode, the evidence behind a pattern.
  - A work state belongs to no piece of the book (「Rules for items」) and is dropped. What an item carries
    beyond it that lasts, a view the person voiced or how they want work done, moves into the piece for it first.

### The heading is set aside first

- **The holding file:** the piece's whole text moves into `trimming/<slug>.md` beside the part's `chunks/`
  (`<brain>/rooms/<chatId>/people/trimming/`, `<brain>/people/<userId>/trimming/`), its sections joined under
  their names, and the piece keeps only its heading line and its row.
- **So every batch is short at once:** from that moment a batch is handed the heading as the trim leaves it, never
  the text set aside.
- **The holding file is no piece.** It has no row; no search, batch or sheet judgement reads it. What stands in it
  reaches a turn again as its stretch is walked, and meanwhile the sheet written before stands until a batch
  writes it again.
- **The disk is the state:** a holding file on disk is a trim under way, and a run called again carries it on
  from its first item.
- **One heading at a time:** a person's heading set aside is walked to its end before the next is set aside, one
  already set aside goes first, and a stretch that fails stops that person's trim until the next run. What a
  holding file holds is read by nothing until it is walked, so a failure never leaves a second heading set aside
  behind it.

### One run walks one stretch

- **The stretch:** the holding file's first items, about 16,000 characters, each under its section.
- **What the run is handed:** the stretch, the heading's piece as it stands, today's date, and how each item
  number the stretch names stands, as the agency's item states say. It reads no messages.
- **What it does with each item:** keeps it in the heading's piece, moves it into the piece of its own matter in
  the same part, found through the fragment tool or made new at the ordinary piece size, or, for a work state,
  drops it. An item equal to one already standing folds into it, its confirmed moved forward. Every piece it
  writes gets its row.
- **What it drops it lists:** each dropped item, word for word, in `trimming/<slug>.dropped.md` beside the holding
  file. The list is committed with the stretch, so the brain's history shows every item a trim dropped, and taken
  away before the next stretch.
- **Only its own person's part:** a room heading's run writes that person's room pieces in that room alone, a
  common heading's run that person's common pieces alone, under the common boundary, and the dropped list; no
  sheet, no index, no other person's piece and not the holding file.

### A stretch is measured as a sorted group is

- **The floor:** what that person's pieces in that part gained, with the items whose bracket moved and the items
  the dropped list names, must come to 80% of the stretch's statements, counted as 「Sorting pieces」 counts them;
  a run that summarises rather than moves falls under it.
- **The dropped list holds the stretch's own items only:** a line that is not one of them word for word puts the
  stretch back, so nothing reaches the list but whole items, and nothing is dropped unlisted.
- **Put back as a group is,** and for the same causes: a write outside the part or into another person's piece, a
  common piece naming an id or a room title, a piece left without its row, and the holding file changed.
- **A stretch that stands** is taken off the holding file; where the heading is a room piece on the room's sorted
  list, the room pieces it made go on the list too, since what they hold was sorted already. Each stretch is
  committed into the brain on its own.
- **A stretch that fails** is tried again by the next run. Failing a second time as it stands, it goes back into
  the heading's piece unwalked, the owner is told once through the owner notice, and the walk goes on with the
  next stretch.

### The end of a walk

- **The holding file emptied** is removed, and a heading's piece left with no item is removed with its row:
  nothing in it was open.
- **A heading still past its ceiling** at the end is counted as a failure of the text it holds. The next run sets
  it aside and walks it again; past its ceiling a second time on the same text, it is left as it stands and the
  owner is told once.

## Reforming a book

A verb of its own, the people route with `reform` for a room and a number of `batches`: writing one room's
room-only book again from scratch from conversation already folded.

### It reads only messages already folded

- **Reading:** messages already marked as read are read oldest first, through the receiver's read messages, in
  batch-sized portions.
- **No marks:** nothing is marked.
- **The lock:** it takes the same lock as other writers.
- **Not held by the digest pause;** held by a job's brain hold.

### It writes only the room-only part

- **A fresh place:** the writer writes new room pieces into the empty `people-reform/` beside the room's
  `people/`, and their rows through the fragment tool under the paths they will stand at once the new book takes
  the old one's place.
- **Common is read-only:** the common part is shared by every room, so one room's reform cannot replace it.
- **Unsorted:** the new book's pieces stand as unsorted, and 「Sorting pieces」 sorts them after the swap.
- **Sheets last:** after the last batch, the sheet judgement writes each person's room sheet in that place.

### Only a finished book swaps with the original

- **The swap happens only when all three hold:**
  1. every piece has its row;
  2. every person who had a document in the original book and has folded messages has pieces;
  3. every person with pieces has a room sheet.
- **The swap:** the room's index, pieces and sheets are replaced by the new ones, and `sorted.txt` is emptied.
- **After the swap:** changed documents pass the sanitiser, and the sessions holding them are told.
- **On failure:** if any batch fails or a check catches it, the place being written is deleted and the original
  book stands.
- **Too few batches means no start:** if that room's folded messages do not fit in the batches it was given, the
  route refuses `too-few-batches` without running at all.

### It commits what took the original's place

- **After the swap,** the room's `people/` is committed into the brain repository in one commit, under "Room
  <chatId>'s person book is written again from <n> folded messages".
- **On failure** the original stands untouched and nothing is committed.
- **Reviewing a result** is reading that commit, and undoing it is reverting it in the brain repository.

## The day reading starts

- **A start day per agent and room:** each agent's book of a room can have a day from which it starts reading
  conversation.
- **What it does:** messages sent before that day (00:00 Korean time) are neither counted nor read by catch-up,
  so even an old room's book stands only on conversation from that day.
- **Where it lives:** it is the owner's value, one key per agent and room in the brain's settings, read and
  changed through the people route's `since`: a day sets it, `none` removes it, and leaving it out answers the
  current value. A start day set for one agent leaves every other agent's book of the room as it was.

## Writing every book again from scratch

Writing every person book again from conversation is done with these steps in order:

1. **Pause.** Stop the digests through the pause route with `on`, and wait for the running batch to finish and
   release the lock. The sort and runs waiting for the lock stop when they are not holding it.
2. **Back up.** Back up `<brain>/people/` and every room's `<brain>/rooms/<chatId>/people/` into one archive file
   outside the brain, run from inside `<brain>`:
   `tar --zstd -cf <data root>/backups/people-books-<time>.tar.zst people rooms/*/people`.
   To restore, unpack it inside `<brain>` with `tar --zstd -xf <that file>`. The brain's own history is the second
   way back.
3. **Clear.** Empty those directories and commit in the brain repository.
4. **Forget.** Clear every room's read marks for the agent through the people route's `forgetRead` with `all`, or
   with one room's `chatId` for that room alone, naming the agent whose book it is.
5. **Resume.** Release the pause with `off`. The launcher starts catch-up per room, and batches write both parts
   in the present shape from the start; sorting has no room pieces to sort and does nothing.

While the books are empty, turns have no sheets to attach and no pieces to be offered, so rooms are answered
without person books.
