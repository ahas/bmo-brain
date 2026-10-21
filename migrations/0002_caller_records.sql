-- The caller's own records (docs/boundary-api.md, 「A caller's records」): a room's rules, a session's loaded
-- documents and a chat session's handoff note. Every time is epoch milliseconds, but a load's `loaded_at` is the
-- caller's `at`, in seconds.

CREATE TABLE room_rules (
  chat_id BIGINT NOT NULL,
  id TEXT NOT NULL,
  subject BIGINT NOT NULL,
  text TEXT NOT NULL,
  message_id BIGINT,
  created_at BIGINT NOT NULL,
  changed_at BIGINT NOT NULL,
  PRIMARY KEY (chat_id, id)
);
CREATE INDEX room_rules_by_room ON room_rules (chat_id, created_at, id);

CREATE TABLE session_loads (
  session TEXT NOT NULL,
  path TEXT NOT NULL,
  version TEXT NOT NULL,
  loaded_at BIGINT NOT NULL,
  PRIMARY KEY (session, path)
);
CREATE INDEX session_loads_by_age ON session_loads (loaded_at);

CREATE TABLE session_notes (
  agent TEXT PRIMARY KEY,
  note TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  changed_at BIGINT NOT NULL
);
