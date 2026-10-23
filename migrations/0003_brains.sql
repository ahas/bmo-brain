-- The brains and their three tables (docs/design.md): subjects, layers and versioned memories, the links
-- between memories and subjects, and the marks of what a session was handed. Every time is epoch
-- milliseconds. The rows held under a caller's scope keep answering their routes from scoped_memories
-- until their readers have moved.

ALTER TABLE memories RENAME TO scoped_memories;

CREATE TABLE brains (
  id BIGSERIAL PRIMARY KEY,
  account_key TEXT NOT NULL,
  agent_key TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE (account_key, agent_key)
);

CREATE TABLE subjects (
  id BIGSERIAL PRIMARY KEY,
  brain_id BIGINT NOT NULL REFERENCES brains (id) ON DELETE CASCADE,
  type TEXT NOT NULL,
  key TEXT NOT NULL,
  description TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE (brain_id, type, key)
);

CREATE TABLE layers (
  id BIGSERIAL PRIMARY KEY,
  brain_id BIGINT NOT NULL REFERENCES brains (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  position BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE (brain_id, name)
);

-- One row is one memory in one version. A first version's lineage_id is its own id; `hit_times` keeps the
-- last hits, newest last, which the retrieval strength is computed from.
CREATE TABLE memories (
  id BIGSERIAL PRIMARY KEY,
  brain_id BIGINT NOT NULL REFERENCES brains (id) ON DELETE CASCADE,
  key TEXT,
  type TEXT NOT NULL,
  text TEXT NOT NULL,
  layer_id BIGINT REFERENCES layers (id) ON DELETE SET NULL,
  position BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  lineage_id BIGINT NOT NULL,
  superseded_by BIGINT,
  valid_from BIGINT NOT NULL,
  valid_to BIGINT,
  importance SMALLINT NOT NULL,
  hits BIGINT NOT NULL DEFAULT 0,
  hit_times BIGINT[] NOT NULL DEFAULT '{}',
  last_hit_at BIGINT,
  last_confirmed_at BIGINT,
  strength DOUBLE PRECISION NOT NULL DEFAULT 1,
  consolidated_at BIGINT,
  actor_name TEXT NOT NULL,
  labels JSONB NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX memories_by_brain ON memories (brain_id, id);
CREATE INDEX memories_by_lineage ON memories (lineage_id, id);
CREATE INDEX memories_by_layer ON memories (layer_id, position, id) WHERE layer_id IS NOT NULL;
CREATE INDEX memories_by_type ON memories (brain_id, type, id);
CREATE UNIQUE INDEX memories_live_key ON memories (brain_id, key)
  WHERE key IS NOT NULL AND status IN ('active', 'pending');

CREATE TABLE memory_subjects (
  memory_id BIGINT NOT NULL REFERENCES memories (id) ON DELETE CASCADE,
  subject_id BIGINT NOT NULL REFERENCES subjects (id) ON DELETE CASCADE,
  PRIMARY KEY (memory_id, subject_id)
);
CREATE INDEX memory_subjects_by_subject ON memory_subjects (subject_id, memory_id);

-- Which versions a session was handed, so a recall marks them instead of repeating them.
CREATE TABLE handouts (
  brain_id BIGINT NOT NULL REFERENCES brains (id) ON DELETE CASCADE,
  session_key TEXT NOT NULL,
  memory_id BIGINT NOT NULL REFERENCES memories (id) ON DELETE CASCADE,
  at BIGINT NOT NULL,
  PRIMARY KEY (brain_id, session_key, memory_id)
);
