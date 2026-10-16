# The agents' memories

What an agent is holding: a promise it made, something it said it would check, a follow-up handed to it, a
rule the owner set for a situation ("from now on, when this happens, do that"), and the state of something
under way in a room. One table, `memories`, holds all of them; a row's `type` says which it is and its `agent`
says whose it is. The brain keeps the rows and answers for them; the gate decides what a turn is shown of them
and the matcher that judges a condition is the gate's (`boundary-api.md`, 「Memory rows」).

## Whose a row is

- **Every row is one agent's,** named in its `agent` column by the agent's registration name. An insert names
  its agent; the column has no default (`agent-ownership.md`).
- **An agent reads, matches, wakes on and writes only its own rows.** Every read route takes the agent, and a
  room several agents answer holds rows of each that reach their own agent alone.
- **Reminders and watches are an agent's the same way.** A reminder or a watch names its agent, and its moment
  wakes that agent alone in its room, through the agency's quiet instruction.

## What a memory is

- **One row is one thing an agent remembers,** of one of five types:
  - `promise`: something the agent said it would do.
  - `check`: something it said it would look at or confirm later.
  - `followup`: a follow-up someone in the room, the owner included, handed to it.
  - `situation`: what the owner set it to do from a situation on.
  - `state`: something under way in the room for now and where it has got to, such as an improvised game with
    its players, whose turn it is and the last move, or any other work in progress. It holds until the thing
    ends or stops (「A state row」 below).
- **The type says what the row is; the condition or the event says when it attaches.** A row carrying neither
  stands in front of every turn of its room; a row carrying a condition waits for the matcher to say the
  situation fits; a row carrying an event waits for that event. Any type may carry either, and a row carries
  one or the other, never both.
- **Written and checked off by the agent itself,** from the room's session, through the memory tools, in every
  room and whoever wrote the turn.
- **One row may be written for the agent rather than by it:** a strand of a finished TODO item that the job
  marked as the agent's own is filed as a `followup` carrying the item, through the same add route.
- **A row outlives its session, its digest and its deploy.**

## What it is not

- **Not the TODO,** which is the job workers' queue and the agency's. A memory is what the agent must
  remember; something that becomes a job stays here until the agent has done what it promised about it.
- **Not a watch.** A watch wakes the agent that set it; the memories are what that agent was holding when it
  wakes (「Watches」 below).
- **Not the person book.** The room-only item "Ongoing and promises" is the digest's account of a person,
  written from the room's messages on the digest's schedule (`people-book.md`). A memory is written the moment
  the thing arises and read on the very next turn.
- **Not a note.** A note is material the digest distils into the library and then removes (`knowledge.md`). A
  memory is never handed to a digest.
- **Not the knowledge library or a room rule.** The library holds what a room has come to know, offered when a
  turn's topic calls for it; a room rule is always in force in its room. A memory with a condition is in force
  only where that condition holds.

## The row

One row per memory, holding:

- its slug, the name the matcher answers under and the console lists it by;
- its type and the agent it belongs to;
- the room it belongs to, or nothing at all where it is global;
- the condition and the action, the condition empty where the row waits for nothing;
- the event it waits on, empty where it waits on none, and when that event first came;
- whom it is for, where it is for one person;
- whether it is active;
- when it last matched, and how many times it has matched;
- who set it and when, the message it was set from, and the TODO item where one asked for it;
- for a state, when it last moved;
- when it was checked off, and a line saying how it ended;
- why it came down and when, where it has;
- whether it stands as a candidate for retirement, and why;
- for a global row, which kinds of room it reaches.

**Every row has a slug.** A writer with a name of its own gives one; a writer with none leaves it out and the
row is given `memory-<id>`, unique by construction. Every route takes a row by its number or its slug.

## The three roads a row reaches a turn by

Which road a row takes is its condition's and its event's answer and nothing else's. The brain answers each
road on its own route; the gate puts what they answer into the turn.

### A row with no condition

- **Stands in front of every turn of its room.** The standing route answers the room's open rows that wait on
  nothing, with no model asked: the room is a fact the caller already holds.
- **An errand whose event has come stands here too,** from the turn its event came until it is checked off
  (「A row with an event」 below).
- **There is no count limit.** However many rows a room holds, every one is answered.
- **Each row is answered whole,** with its number, its type, when it was written, when a state last moved,
  whom it is for, the TODO item its strand came out of, and its text.
- **Only that room's rows, in every room.** A room's read is by its own number, so a row of another room never
  comes back from it.

### A row with a condition

- **The conditions go up, the actions stay down.** The conditions route answers every open row of the room
  that carries a condition, and the global rows that reach the room, for the matcher to judge.
- **Only a matched row's action attaches,** and that is the gate's to do. The gate hands back which rows
  matched and which it read as spent; the brain writes each matched row's count and last match, and takes a
  spent row down on the spot with the matcher as its reason.
- **No count limit here either.**

### A row with an event

- **An event is a fact the machine records, not a situation a model judges:** a deployment carrying an item
  went live (`deployed`), a job of an item ended (`job-ended`), the queue changed (`queue-changed`). The first
  two may name an item: `deployed:773` waits for the deployment carrying #773, `deployed` for any.
- **No model is asked.** The events route answers the rows an event wakes, by the event alone.
- **An errand waits once.** A promise, a check or a follow-up waiting on an event is woken on the turn that
  carries it, the moment it first came is written on the row, and from then on it stands in front of every
  turn of its room like a row with no condition until it is checked off. An event passing in another room's
  turn still meets it.
- **A situation row waits every time.** It is woken on each turn that carries its event and on no other, and it
  is never met for good.
- **A global row with an event** is woken on the turns its event passes in the rooms it reaches, and stands
  nowhere afterwards.
- **It is never withered.** Withering is a condition's clock; an event row's waiting is the event not having
  happened yet.

### What bounds them

- **A write cap per row:** a condition of at most 500 characters, an action of at most 2,000. A row is bounded
  where it is written, so nothing a turn attaches is ever a sentence cut in half.
- **What a turn carries altogether is the gate's,** which may leave memories last of all when a turn is
  crowded.

## When a row is written

- **An errand, in the same run the thing arises:** the agent says it will do something, let someone know or
  come back to something; says it will check or confirm something later; or is handed a follow-up.
- **A rule, at the turn the owner sets one.** What anybody else asks for in a room is that turn's request.
- **An errand that waits for something to happen is written with what wakes the agent when it does.** Nothing
  wakes an agent by itself, and a condition is only judged on a turn something else opened:
  - an event the machine records is given as the row's event (`on`);
  - anything else is given as a check a watch runs (`until`), set in the same call as the row and serving it;
    the two stand together or not at all.
- **A row is never written from a note,** never from a digest, and never by a gate on its own judgement.
- **Whether a turn should have filed a row** (a promise to act once something happens, a rule the owner set)
  is checked by whoever reviews the turn's answer, which reads the turn's own calls; the brain answers only
  for the rows.

## A state row

- **What it holds:** what is under way in the room and where it has got to now: for a game, who is playing,
  whose turn it is and the last move; for any other work, the step it stands at and what comes next.
- **When it is written:** in the run the thing starts, and not for anything a single answer finishes.
- **Where it stands:** in front of every turn of its room while it is open, on the road of a row with no
  condition. It belongs to one room, so it is never global, and it waits on nothing, so it carries no
  condition, no event and no watch.
- **It moves as the thing moves.** Each step rewrites the row's text in place with where it now stands
  (`memory_update`), and the moment it last moved is written with it.
- **It is released, never deleted.** A released state is checked off: it stops standing and stays in the table
  with when and how it ended. Three things release it:
  - someone ends it in words, or the thing reaches its end, and the agent checks it off in that run;
  - the agent judges the room has plainly moved on, and checks it off saying it stopped half-way;
  - it has not moved for twelve hours: the standing route releases it on its room's next read, before it
    answers, with a line saying it stopped for want of movement. A quiet room releases nothing until it is
    read again.
- **Taking it back up** after it was released is a new state; the released row is not reopened.

## The memory routes

- **The memory tools are a separate repository** that calls the brain's memory routes, one route per tool:
  `memory_add`, `memory_update`, `memory_done`, `memory_retire`, `memory_revive`, `memory_candidate`,
  `memory_list`, with `note_add` and `watch_set` beside them (`boundary-api.md`, 「Memory rows」). Each carries
  the turn it runs in, and acts in the turn's own room unless `chat` names another or `global` names none.
- **What each does:**
  - **add** files a row of a type with its action, and optionally a condition or an event, a slug, whom it is
    for, who set it, the message, the TODO item, and the reach of a global row. It answers the new row's number
    and the name it stands under.
  - **add with `replaces`** files the new row and takes the named one down in the same call, with the new row's
    slug as its reason.
  - **add with `until`** files the row and a watch serving it in one call, the watch taking `name`, `every`
    and `within` as the watch route does and the row's action as its note. A watch that cannot be set takes
    the new row down again with the reason, so a row is never left with nothing to wake it.
  - **update** rewrites an open state's text with where it now stands and writes when it moved.
  - **done** checks an errand off, with an optional line of how it ended, and releases a state.
  - **retire** takes a row down with a reason; **revive** puts one back; **candidate** raises one as no longer
    useful with a reason and changes nothing else.
  - **done**, **retire** and **add with `replaces`** cancel every standing watch serving the row they close,
    and name each in their answer.
  - **list** answers the open rows, or with `all` the finished and the down as well, narrowed by `type`.
- **A slug given is lower-case letters, digits and hyphens,** and names one row across the whole table.
- **Refusals** write nothing and carry a code and one sentence saying why (`boundary-api.md`, 「Memory
  refusals」):
  - no agent, no room, both scopes at once or neither;
  - an unknown type, a missing action, a slug of another shape or one already taken;
  - a condition over 500 characters, an action over 2,000;
  - a `situation` row with neither a condition nor an event, which would attach every turn instead of waiting;
  - a `state` row with a condition, an event or `until`, or on a global row;
  - update on a row that is not a state, one already released or down, or one of another room;
  - a condition and an event on the same row, and an event outside the vocabulary;
  - `until` beside a condition or an event, or on a global row, which no watch can come back to;
  - an errand whose action waits on something happening ("once it is out", "after it ends", "after the
    deploy"), as the receiver's reading of promises says, carrying neither `on` nor `until`;
  - `for` on a global row, `reach` on a room's own row;
  - a reason left out of retire or candidate;
  - a row of another scope or another agent, one already down or already up, a `replaces` naming no standing
    row;
  - a row that does not exist, one of another room, one already checked off;
  - a `situation` row put to done, which is never finished and is taken down instead;
  - a global row whose text names a room, a person or an id (below).
- **Nothing is sent to any room.** The routes only read and write the table.

## The two closings

Being finished and being taken down are two different things, and the table keeps them apart.

### Checked off: the errand is finished

- **When:** the promise is kept, the check is done, or the follow-up is carried out; or the person it was for
  drops it, or the owner does. A state is checked off when it is released.
- **What it writes:** when it ended and a line saying in a few words how.
- **The row stays active,** because nobody retired it. Checking off never deletes.
- **A `situation` row is never checked off.** A rule waits for its condition and fires again every time it
  comes; taking one out of the matcher's candidates is retire, which writes the reason on the row.

### Taken down: the row is no longer useful

There are three roads down, and none of them deletes.

- **Taken back in words.** The owner says a rule is no longer needed and the agent retires it. A new rule that
  contradicts a standing one replaces it, the old one going inactive naming the new one as its reason. The
  matcher reading a row as spent is the other half of this road.
- **Withering.** A row whose condition has not matched for sixty days goes inactive and leaves the conditions
  the matcher is offered. One that has never matched counts from the day it was set. The sweep runs inside the
  conditions route, at most once an hour for a room, so a quiet room withers nothing. **Only a row with a
  condition is ever swept**: an errand nobody has done yet is still waiting, and a state that stops moving is
  released rather than withered. The hour a room was last swept is held in the process's memory and let go
  when the room is forgotten.
- **Overtaken.** Where the action has since been written into code, a room's rules or a rule of the system,
  the row is raised as a **candidate** and nothing else happens to it. It stays active, it still matches, and a
  person settles it on the console.

## Nothing is deleted

- **Coming down sets a flag and writes a reason.** The row stays, with its condition, its action, its count,
  its last match and the account of why it stopped.
- **The table has no delete path,** in the routes, on the console or in the brain's own code, but one: a room
  removed takes its rows with it (`boundary-api.md`, 「Rooms leaving」).
- **Why:** a row is the record of what the agent held or what the owner instructed, and why the agent behaved
  as it did. One taken down by mistake can be found again.
- **There is no retention sweep and no row bound.**

## Global rows and room isolation

- **A row not tied to one room may be global.** It has no room of its own and is offered in every room its
  reach allows.
- **A global row's text may name no room and no person.** The condition and the action are both checked as the
  row is written, and one naming a room by its title, a person by name, a user or chat id the receiver has
  seen, or a bare `id <number>` is refused with the offending phrase named. The titles and ids come from the
  receiver's room titles. The owner's own name is not among them: the owner is in every room.
- **A global row may not be for one person,** since it is read in rooms that person was never in.
- **Which rooms it reaches is the row's own field:** the owner's private room (`owner`), group rooms (`group`),
  or every room (`all`). **The default is the owner's private room alone,** so a row set in private never
  begins governing a room full of other people without that being chosen.
- **A room is offered two lists and they are read apart.** Its own rows are found by its own number; the global
  ones are read separately and held to their reach by the room's kind.
- **Which kind of room it is** is asked of the receiver's roads, because a one-to-one with the owner is the
  owner's private chat wherever it is and whatever it is numbered.
- **A row that has come down is offered nowhere,** its own room included.

## Silence

- **Memories are quiet.** The agent does not recite them, announce writing or checking one, or bring a promise
  up unless someone asks about it, in every room and with no exception. This is a rule of the agent's prompt,
  not of the brain: the brain sends nothing to any room.
- **Acting on a row is not reciting it.** Keeping a promise, such as reporting the promised result, is the
  point of holding it.

## Watches

- **The watch is the only thing that brings an agent back.** When the agent tells anyone it will let them know
  once something finishes, look at a result once it comes, or check back on something, it sets a watch in that
  same run.
- **Setting one is the owner's alone,** as with jobs: the check is a shell command run as the machine's user,
  and the agent's prompt, not the route, holds anyone else's request to that.
- **What one is:** a short, read-only shell check the agent writes for the occasion, and a note in its own
  words of what it is waiting for and what it promised, to whom (`watch_set`).
- **The note is the whole of that promise** and comes back when the watch ends. `name` is the few words a line
  shows the watch by; a watch set without one is shown by the head of its note.
- **The route refuses before anything exists** a check bash cannot parse, a missing setter or note, a name
  over 40 characters, an interval under 15 seconds or over 30 minutes, a wait over 24 hours, an eleventh
  standing watch for one room, and a check already standing for the same room, naming the watch that stands.
- **What passes is a row in the `watches` table and a run of its own.** The brain asks the agency's unit
  launch to start the run in a unit named after the watch, outside the brain's own process, and answers at
  once.
- **The run checks with bash,** input closed, every interval (a minute by default), cutting a check off after
  30 seconds along with everything it started; a check that exits 0 is the thing having happened.
- **The check is handed the user's service manager,** so `systemctl --user` answers from inside it; a check
  whose output says it could not reach that manager's bus has judged nothing, so it is never taken as the thing
  having happened: the first such check comes back at once as one that cannot run, and a later one is waited
  past like a no.
- **Then the note goes to the room that set the watch** as a quiet instruction for the watch's agent, through
  the agency, framed as the agent's own note with what the check printed marked as data.
- **When the wait (two hours by default) runs out first,** the note comes back saying the check never
  succeeded and how the last one ended; a check whose first run cannot execute at all (exit 126 or 127) comes
  back at once saying so.
- **A timer** (`at`, Asia/Seoul, at most a week out) is the exception, for a moment no check can express: it is
  a row alone, with no run, and the brain's own clock fires it.
- **A watch outlives the brain's process.** Every 30 seconds the brain's clock ends the watches of closed
  memories, fires due timers (one that came due while the brain was down included), and launches a new run
  for any standing watch whose run nobody holds, which is how a reboot is survived. A run claims its watch as
  it starts; a re-launched run takes the watch over and the one it replaced stops when it next claims. A watch
  ends exactly once, so its note is queued exactly once.
- **Listing and cancelling:** the watch list route lists a room's watches, and cancel ends one without a note;
  only the room that set it or the owner's room may cancel it.
- **Stopping a watch's run ends the watch itself,** and not the run alone: the row is ended before the unit
  goes, so the clock has nothing standing to launch again, and the room that set it gets the agent's own note
  saying the watch was stopped and that nothing more is coming.
- **A watch and a reminder:** a watch wakes the agent on a condition, or at a moment at most a week out, in
  the room that set it; a reminder wakes it in its room at a moment of the room's own calendar, however far
  ahead and however often.

### A watch and the memory it serves

- **Two roles.** A watch wakes the agent when something happens or a time comes; a memory is what the agent
  was holding. A promise that needs waking gets both, each the same agent's.
- **A watch is one agent's,** named in its `agent` column, and serves only a row of its own agent's.
- **Linking them.** The watch route takes an optional `item`. It refuses a row that does not exist, is already
  checked off, is down, or belongs to a room other than the watch's.
- **A watch stands only while the row it serves is open.** Once that row is checked off or taken down, by
  whatever road (done, retire, a new row filed with `replaces`, the console's switch, withering, the matcher,
  a state's release), every standing watch serving it is cancelled with it, and no note goes out, since the
  agent closing the row already knows. The routes that close a row end its watches on the spot and name them;
  the clock ends any it finds on its next pass. The run holding a check stops when it next claims its row.
- **A watch set without `item` serves no row** and stands until it fires, lapses or is cancelled.
- **The note carries the row.** A fired, lapsed, broken or timer note names the linked row by number and quotes
  its current text, and says to check it off once the promise is kept. When the row was closed before the
  watch ended, the note says so instead.
- **A watch never checks a row off.** Whether the promise is kept is the agent's to judge and record.
- **The watch listing** names the row a watch serves.

## The console

- **The console reads the rows** through the memories route, every agent's or one agent's, newest first, and
  draws them itself. It reads watches, room names and messages from their own routes.
- **The active switch is the only write it makes.** Putting a row back is one call; taking one down carries a
  reason, since the reason stays with the row for good. A candidate is settled by the same switch.
- **Every row stands in one of three states:** open (active and not checked off), finished (checked off with
  its flag still on) and down (its flag off, whether or not it was checked off first). Open is what the brain
  answers as standing and what the list route answers without `all`.
- **Checking an errand off is not the console's.** That is the room's, where the errand was made.
