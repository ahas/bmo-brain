---
name: brain-harness
description: Keep and read an agent's long-term memory with the bmo-brain MCP server. Use when a harness should find its brain, read what a turn should know (brain_recall), write what it learns as versioned memories about subjects (brain_memory_write, brain_memory_version), close a memory that stopped holding (brain_memory_move), tell the brain a session folded (brain_fold), or read the persona.
---

# brain-harness

The brain is an agent's long-term memory as a service. A **brain** belongs to an account and an agent; it
holds **subjects** (what a memory is about: a person, a room, a document, found by type and key), **layers**
(its table of contents) and **memories**, one row a version: a correction is a new version, a fact that
stopped holding is invalidated, nothing is deleted. The `bmo-brain` MCP server reaches one brain service and
offers a tool for each route a harness needs.

## Starting the server

Register it with the harness as a stdio MCP server:

```
bmo-brain mcp --brain http://127.0.0.1:8080
```

The address is the service's base address, where its `/health` route stands; `BRAIN_URL` may give it instead.
The token goes in `BRAIN_TOKEN` or `--token`; a wrong one answers every tool with a 401 `unauthorized` error.

## Pages and times

Every time is epoch milliseconds, and every list is a page: `items` and `more`; pass the last `id` as `after`
for the next page. `labels` is your own metadata, strings to strings, which the brain never reads.

## Failures

A failure comes back as a tool error whose text is JSON: `status`, `code` and `error`, the route's own.
`bad-request` names what is wrong with the arguments; `not-found` means no member stands at the id or key.

## A brain's memory

The discipline of a turn, which a chat agent's system prompt carries, is `every-turn.md` beside this
skill: read the recall before answering, keep episodes as you read, judge every fact against its likes and
apply the judgement with one `brain_learn`, and consolidate when the brain says it is due.

- **Find the brain first:** `brain_brains` with `accountKey` and `agentKey`, or `brain_brain_add` where none
  stands. Every other brain tool takes `brainId`.
- **Before answering, ask `brain_recall`** with the room (`type` `room`, its key), the speakers (`type`
  `person`, their keys), the turn's words, your `sessionKey` and when the session opened. The answer's `block`
  is what to read; a row marked as handed to this session already is one you have seen in this session.
- **Write what you learn with `brain_memory_write`:** `type` (`fact`, `rumor` for what you only heard,
  `situation` for a rule for a situation, `episode` for what happened, or your own word), `text`, the
  `subjects` it is about (each `type` and `key`, made where none stands), `importance` 1 to 10, and your
  `actorName`. Give a `key` to a memory you may rewrite later.
- **Judge every fact before writing it:** `brain_similar` with the candidate's words and subjects answers the
  live memories like it; then `brain_learn` with your judgement (`confirms`, `strengthens`, `replaces`, or
  none for a new one) and the `source` (`own`, `owner`, `told`, `inferred`) applies it whole. A `told` or
  `inferred` learning is written as a rumor and closes nothing.
- **Correct with `brain_memory_version`,** never by writing a second row: the old version is superseded and
  kept. **Close with `brain_memory_move`:** `done` for a promise kept, `invalidated` for a fact that stopped
  holding; `active` approves a `pending` row.
- **Consolidate when the recall says it is due:** `brain_unconsolidated` reads the episodes most important
  first, `brain_learn` writes what they come to with the episodes under `grounds`, and `brain_consolidated`
  marks them.
- **Tell the brain when the session folds** with `brain_fold`, so the marks clear and what it knew may attach
  to the next session.
- **`brain_persona`** renders the agent's persona; `brain_types` lists the types in use, so one meaning keeps
  one spelling; `brain_subjects`, `brain_layers` and `brain_memories_list` read the tables.
