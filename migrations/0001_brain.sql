-- The brain's tables (docs/boundary-api.md). Every time is epoch milliseconds.

CREATE TABLE memories (
  id BIGSERIAL PRIMARY KEY,
  scope TEXT NOT NULL,
  subject_key TEXT,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  state TEXT NOT NULL,
  conditions JSONB NOT NULL,
  labels JSONB NOT NULL,
  due_at BIGINT,
  match_count BIGINT NOT NULL DEFAULT 0,
  last_matched_at BIGINT,
  created_at BIGINT NOT NULL,
  changed_at BIGINT NOT NULL
);
CREATE INDEX memories_by_scope ON memories (scope, id);
CREATE INDEX memories_by_subject ON memories (subject_key, id);

CREATE TABLE memory_matches (
  id BIGSERIAL PRIMARY KEY,
  memory_ids BIGINT[] NOT NULL,
  actor_name TEXT NOT NULL,
  spent_ms BIGINT NOT NULL,
  token_count BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX memory_matches_by_row ON memory_matches USING GIN (memory_ids);

CREATE TABLE notes (
  id BIGSERIAL PRIMARY KEY,
  scope TEXT NOT NULL,
  subject_key TEXT,
  text TEXT NOT NULL,
  actor_name TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX notes_by_scope ON notes (scope, id);

-- `at` and `memory_id` are read off the condition when the watch is set, for the clock and the rows to find it.
CREATE TABLE watches (
  id BIGSERIAL PRIMARY KEY,
  scope TEXT NOT NULL,
  condition JSONB NOT NULL,
  state TEXT NOT NULL,
  setter_name TEXT NOT NULL,
  holder_name TEXT,
  labels JSONB NOT NULL,
  fired_at BIGINT,
  created_at BIGINT NOT NULL,
  changed_at BIGINT NOT NULL,
  at BIGINT,
  memory_id BIGINT
);
CREATE INDEX watches_by_scope ON watches (scope, id);
CREATE INDEX watches_waiting_on_time ON watches (at) WHERE state = 'armed' AND at IS NOT NULL;
CREATE INDEX watches_by_row ON watches (memory_id) WHERE memory_id IS NOT NULL;

CREATE TABLE reminders (
  id BIGSERIAL PRIMARY KEY,
  scope TEXT NOT NULL,
  due_at BIGINT NOT NULL,
  text TEXT NOT NULL,
  labels JSONB NOT NULL,
  setter_name TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  fired_at BIGINT
);
CREATE INDEX reminders_by_scope ON reminders (scope, id);
CREATE INDEX reminders_waiting ON reminders (due_at) WHERE fired_at IS NULL;

CREATE TABLE fragments (
  id BIGSERIAL PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  scope TEXT,
  subject_key TEXT,
  title TEXT,
  path TEXT,
  text TEXT NOT NULL,
  version TEXT NOT NULL,
  library_name TEXT,
  state TEXT NOT NULL,
  switched_off TEXT[] NOT NULL,
  labels JSONB NOT NULL,
  hit_count BIGINT NOT NULL DEFAULT 0,
  last_hit_at BIGINT,
  created_at BIGINT NOT NULL,
  changed_at BIGINT NOT NULL
);
CREATE INDEX fragments_by_scope ON fragments (scope, id);
CREATE INDEX fragments_by_library ON fragments (library_name, id);

CREATE TABLE fragment_hits (
  id BIGSERIAL PRIMARY KEY,
  fragment_key TEXT NOT NULL,
  actor_name TEXT NOT NULL,
  scope TEXT,
  created_at BIGINT NOT NULL
);
CREATE INDEX fragment_hits_by_scope ON fragment_hits (scope);

CREATE TABLE libraries (
  name TEXT PRIMARY KEY,
  documents JSONB NOT NULL,
  kinds JSONB NOT NULL,
  changed_at BIGINT NOT NULL
);

CREATE TABLE locks (
  id BIGSERIAL PRIMARY KEY,
  holder_name TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT
);

CREATE TABLE scopes (
  scope TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  changed_at BIGINT NOT NULL
);

CREATE TABLE events (
  id BIGSERIAL PRIMARY KEY,
  kind TEXT NOT NULL,
  scope TEXT,
  at BIGINT NOT NULL,
  data JSONB NOT NULL
);
CREATE INDEX events_by_time ON events (at);

-- The highest event number let go of: a reader resuming from before it is told the window has passed it.
CREATE TABLE event_floor (
  only_row BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (only_row),
  pruned_through BIGINT NOT NULL
);
INSERT INTO event_floor (pruned_through) VALUES (0);
