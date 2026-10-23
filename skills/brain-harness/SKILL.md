---
name: brain-harness
description: Keep and read an agent's long-term memory with the bmo-brain MCP server. Use when a harness should find its brain, read what a turn should know (brain_recall), write what it learns as versioned memories about subjects (brain_memory_write, brain_memory_version), close a memory that stopped holding (brain_memory_move), tell the brain a session folded (brain_fold), or read the persona; and for rows still held under scopes, the scope tools below.
---

# brain-harness

The brain is an agent's long-term memory as a service. A **brain** belongs to an account and an agent; it
holds **subjects** (what a memory is about: a person, a room, a document, found by type and key), **layers**
(its table of contents) and **memories**, one row a version: a correction is a new version, a fact that
stopped holding is invalidated, nothing is deleted. The `bmo-brain` MCP server reaches one brain service and
offers a tool for each route a harness needs. The rows held under a **scope** (memory rows, notes, watches,
reminders, fragments) keep their own tools until their readers have moved.

## Starting the server

Register it with the harness as a stdio MCP server:

```
bmo-brain mcp --brain http://127.0.0.1:8080
```

The address is the service's base address, where its `/health` route stands; `BRAIN_URL` may give it instead.
The token goes in `BRAIN_TOKEN` or `--token`; a wrong one answers every tool with a 401 `unauthorized` error.

## Scopes, subjects and labels

- `scope` is your own string for what a member is held under: a project, a channel, a person you work for. The
  brain never reads it. Pick a scheme and keep to it, such as `project:atlas` and `channel:42`, since
  `brain_memories` can list by `scopePrefix`.
- `subjectKey` is your key for who or what a member is about. A sheet about a person is a fragment of kind
  `person` with that person's `subjectKey`.
- `labels` is your own metadata, strings to strings. `brain_memories` filters by `label` (`key`, or
  `key=value`).
- Every time is epoch milliseconds, and every list is a page: `items` and `more`; pass the last `id` as `after`
  for the next page.

## What to do first

1. `brain_knowledge` with `scopes` (comma-separated, most specific first): the active fragments held under
   those scopes or shared by all, and the standing memory rows. Narrow it with `slugs` (fragment keys), `kinds`
   or `subjectKey`.
2. `brain_memories` for rows by scope, kind, state or label; `dueBefore` for rows due by a moment.
3. `brain_notes` and `brain_fragments` for what was kept for later reading.

## Remembering

- `brain_memory_add`: one thing to remember. `kind` is your word for it (`promise`, `rule`, `state`);
  `conditions` are your own objects, kept as given; `dueAt` is when it is due. Pass `state: candidate` for a row
  that waits for someone's approval.
- `brain_memory_update`: change its `text`, `labels` (replaced whole) or `dueAt` (null clears it), or move its
  `state`. From `standing` a row moves to `done`, `retired`, `candidate` or `off`; `done` and `off` go back to
  `standing` or on to `retired`; `retired` only back to `standing`; `candidate` to `standing` or `retired`. Any
  other move is a 409 `conflict`.
- `brain_note_add`: something to keep for a later reading, with `actorName` saying who wrote it.
- `brain_fragment_put`: a piece of reference text under a `fragmentKey` you choose, written whole; leave `scope`
  out to share it with every scope.

## Being told later

- `brain_reminder_add`: `dueAt` and `text`; the service publishes `reminder.due` at that moment. It fires once.
  `brain_reminder_cancel` takes it back.
- `brain_watch_add`: a `condition` of kind `at` (`at`, a moment), `memory` (`memoryId` and the row `state` it
  waits for; it fires at once where the row is already there) or `external` (`key`, your own; you fire it with
  `brain_watch_update` and `state: fired`).
- A fired watch is claimed with `brain_watch_update`, `state: claimed` and your `holderName`; another holder's
  claim is a 409. End it with `state: ended`, or `cancelled` to give it up.
- Read the events on the service's `GET /events` (server-sent events, or `Accept: application/json` for a page),
  narrowed with `kinds=reminder.due,watch.fired` and `scope`, resuming with `after` or `Last-Event-ID`.

## Editing at length

`brain_lock` with your `holderName` and a `reason` before rewriting many rows or fragments, and `brain_unlock`
with its `id` after; `ttlMs` makes it run out on its own. Asking again as the same holder renews it; another
holder is answered 409 until you let go. The lock holds nothing back by itself: it works because every writer
asks for it.

## Failures

A failure comes back as a tool error whose text is JSON: `status`, `code` and `error`, the route's own.
`bad-request` names what is wrong with the arguments; `not-found` means no member stands at the id or key.

## A brain's memory

- **Find the brain first:** `brain_brains` with `accountKey` and `agentKey`, or `brain_brain_add` where none
  stands. Every other brain tool takes `brainId`.
- **Before answering, ask `brain_recall`** with the room (`type` `room`, its key), the speakers (`type`
  `person`, their keys), the turn's words, your `sessionKey` and when the session opened. The answer's `block`
  is what to read; a row marked as handed to this session already is one you have seen in this session.
- **Write what you learn with `brain_memory_write`:** `type` (`fact`, `rumor` for what you only heard,
  `situation` for a rule for a situation, `episode` for what happened, or your own word), `text`, the
  `subjects` it is about (each `type` and `key`, made where none stands), `importance` 1 to 10, and your
  `actorName`. Give a `key` to a memory you may rewrite later.
- **Correct with `brain_memory_version`,** never by writing a second row: the old version is superseded and
  kept. **Close with `brain_memory_move`:** `done` for a promise kept, `invalidated` for a fact that stopped
  holding; `active` approves a `pending` row.
- **Tell the brain when the session folds** with `brain_fold`, so the marks clear and what it knew may attach
  to the next session.
- **`brain_persona`** renders the agent's persona; `brain_types` lists the types in use, so one meaning keeps
  one spelling; `brain_subjects`, `brain_layers` and `brain_memories_list` read the tables.
