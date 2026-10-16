# What each agent owns in the brain

Every agent, the chat agent and every companion, keeps its own state. This document states the brain's part of
that split: the brain's rows, its long-term memory folders and its recollector runs. Everything else an agent
owns (its sessions, its gate settings, its voice, its usage) is another unit's, and what the brain needs of an
agent is read from the agency's registration lookups (`boundary-api.md`, 「The agency」).

## The principle

- **One owner column, never one table per agent.** Every table of the brain that holds several agents' rows keeps
  them in the one table, with an `agent` column naming the agent each row belongs to, by its registration name.
- **A value that differs per agent is the agent's own,** its registration's, never a table of agents kept here
  and never a branch on an agent's name or kind.
- **Nothing counts the agents.** No code, schema or test of the brain assumes a number of agents or names one of
  them: every route takes the agent a request is for and reads that agent's rows, so an agent added to the
  registrations has every part below on its own from its first turn.
- **The proof is a third agent.** The tests stand a fake agent beside two others and check that it gets its own
  memories, long-term memory, reminders, watches, notes, rules and session loads, and reads nothing of the
  others'.

## What the brain reads of an agent

| Registration field | What the brain does with it |
|---|---|
| `name` | the value of every `agent` column, and the name every route takes |
| `label` | the name a recollector's task calls the agent by, and a room title that is an agent's own name |
| `brainFolder` | where its long-term memory lives, relative to the brain root: the brain root itself for the chat agent, a folder of its own for a companion |
| `recollectorRegistration` | the registration its recollectors run on, or null for each recollector's own work key |
| `reasoning` | how its recollector runs think where they run on its registration |

The chat agent is the registration whose key is the service's configured chat agent's key.

## The stores split by an owner column

| Store | Its owner column | Who reads it |
|---|---|---|
| memories, situation rules and state rows | `agent` | the agent's turns through the gate, its tools, the console's memories screen |
| reminders | `agent` | the brain's clock, which wakes the owner agent when one comes due |
| watches | `agent` | the brain's clock, which wakes the owner agent when its check ends |
| room notes | `agent` | the agent's own digest, which files them into its long-term memory |
| fragments (knowledge, room rules, person-book pieces, system documents, lore) | `agent`, with the mark for every agent | the gate, for the agent's turns |
| session loads | the session, which is one agent's | the gate, for that session |

- **Every write names the agent it is for.** A tool writes the agent of the turn it runs in (`boundary-api.md`,
  「The turn a request is for」); a write with no agent is refused `no-agent` rather than filed under a default,
  and the `agent` column has no default.
- **A reminder, a watch and a memory event wake their own agent:** their quiet instruction goes through the
  agency for the agent the row is, and only that agent's turns take it, in the row's room (`memories.md`).
  Nothing of another agent's ever reaches an agent's turns.
- **Every read names the agent too.** A turn is offered its own agent's rows; the gate's matcher is offered only
  its own agent's condition rows; a reminder or watch wakes its own agent; a room's notes go to its own agent's
  digest. Every room several agents share holds rows of each, and each agent is answered only its own.

## Long-term memory

- **Each agent's long-term memory is a folder of the brain:** its `brainFolder`, holding the same three parts,
  `people/`, `rooms/` and `knowledge/`, with the same shapes (`people-book.md`, `knowledge.md`). The chat agent's
  folder is the brain root.
- **A fragment row names the agent whose folder its body is in,** and the reconcile reads the agent from the
  path's folder, so a body and its row never disagree about whose it is (`fragments.md`, 「The reconcile」).
- **The shipped libraries are the chat agent's.** The system documents and the lore describe the system the chat
  agent runs and its persona, so their rows carry the chat agent; another agent is offered one only where it is
  marked for every agent.
- **Shared between agents is a mark, not a place.** A fragment's mark for every agent says every agent's turns
  may be offered it; the body stays in its owner's folder. It is off unless a recollector or a person sets it,
  and nothing sets it by default.
- **Three axes, kept apart:**
  - the room scope: every room or one room (`[shared]` or `[chat:<room>]`, and the row's room);
  - the person a person-book piece is about: the row's owner;
  - the agent whose memory it is: the row's agent and its mark for every agent.
- **A turn's candidates are its own and the shared.** The offer route answers only the rows of the turn's agent
  and the rows marked for every agent, before any narrowing the gate does.
- **An agent's folder goes with it.** A companion's reset takes its rows and its folder out of the brain, and its
  removal takes its editing lock (`boundary-api.md`, 「Agents forgetting」; `people-book.md`, 「One lock per
  agent's folder」).

## The recollectors

- **The recollectors decide what becomes an agent's long-term memory:** the digest and the person-book writer,
  each a recollector session the brain starts through the agency, and the sheet judgement the brain asks of the
  gate. The session summariser that writes a chat agent's handoff note is not the brain's; the brain keeps the
  note it hands in (`boundary-api.md`, 「Chat-session summaries」).
- **Each run is for one agent.** It reads the words of the rooms that agent answers, writes into that agent's
  folder and rows, and writes from that agent's own first-person view, under the recollection rule this
  repository keeps. A room several agents answer is read once per agent, and each run writes its own.
- **A room is read for an agent where the agent answers it,** as the receiver's room answerers say; a room with
  no answerers is the chat agent's alone. The digest takes each agent's own notes, and the person-book writer
  reads past each agent's own read marks (`people-book.md`, 「Each agent's run」).
- **Every recollector is told whose memory it writes.** Its task names the agent by its registration's label, and
  an editor run stands in that agent's brain folder (`fragments.md`).
- **A companion hands its own session over,** writing its state on its own model, and no session summariser runs
  for it; that is the companion's and the agency's, and the brain is not asked.

### The model each recollector runs on

- **Each recollector's own work key, by default:** the person-book writer on `people-digest`, the sheet judgement
  on `people-standing`, the digest on `digest`, each resolved by the agency and the gate as they hold the key's
  road. The chat agent's recollectors always run so.
- **A companion may name its own:** where its registration names a recollector registration, every recollector
  run for that companion (a batch, a cut, a sort, a reform, a sheet, a digest of its notes) is asked on that
  registration, and the key's road is not read. An editor run thinks as the companion's registration says its
  turns think; a sheet keeps the sheet judgement's own thinking. The run is the same run: its prompt, its tools,
  its desk and its checks are the recollector's, and only the model answering it is the companion's.
  - **A sheet so asked** is the sheet judgement sent with that registration named (`boundary-api.md`, 「The
    gate」).
  - **No other road stands behind it:** a registration that does not answer fails the run as a chosen road that
    did not answer does, and nothing turns to the key's road in its place.
- **Why the companion's:** what a companion remembers is read back into its own turns, so it is written by the
  model that speaks as it, from its view, and no other model's judgement of what is fit to say reaches its turns
  through its memory.
- **Each run sorts what it keeps:** a fact every agent is better for knowing (about the owner or another person,
  about the system, the machines or their running) is written with the mark for every agent; a memory of the
  agent's own (what it said, promised, felt, how it plays or works) is written as its own.
