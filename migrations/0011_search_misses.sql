-- What a brain counts of its asks by words (docs/design.md, 「What the brain counts」): each brain's count of
-- its asks and its misses, and the misses it keeps, each with the words as asked and the stems they became.

CREATE TABLE search_counts (
  brain_id BIGINT PRIMARY KEY REFERENCES brains (id) ON DELETE CASCADE,
  asks BIGINT NOT NULL DEFAULT 0,
  misses BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE search_misses (
  id BIGSERIAL PRIMARY KEY,
  brain_id BIGINT NOT NULL REFERENCES brains (id) ON DELETE CASCADE,
  at BIGINT NOT NULL,
  route TEXT NOT NULL,
  words TEXT NOT NULL,
  stems TEXT[] NOT NULL
);
CREATE INDEX search_misses_by_brain ON search_misses (brain_id, id);
