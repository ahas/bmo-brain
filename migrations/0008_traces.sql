-- A memory's traces and its brain's psychological clock (docs/design.md, 「Strength and decay」). Every time
-- is epoch milliseconds; a psychological moment is milliseconds on the brain's own clock.
--
-- The hits a row kept become traces: each hit a `handed` trace, and the row's own writing its `written`
-- trace, whose salience is its importance over five. Their psychological moments are read off a clock run
-- once over every moment the brain's rows hold, in order, each gap counting whole up to an hour and 0.031
-- of itself past it: the default rules' same_session_ms and between_sessions, which this migration
-- computes with since it reads no configuration. Every converted trace takes the default decay intercept,
-- 0.167, as its decay. The brain's clock is left at its latest moment.

ALTER TABLE brains ADD COLUMN clock_real_at BIGINT;
ALTER TABLE brains ADD COLUMN clock_psych DOUBLE PRECISION NOT NULL DEFAULT 0;

CREATE TABLE memory_traces (
  id BIGSERIAL PRIMARY KEY,
  memory_id BIGINT NOT NULL REFERENCES memories (id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  real_at BIGINT NOT NULL,
  psych_at DOUBLE PRECISION NOT NULL,
  decay DOUBLE PRECISION NOT NULL,
  salience DOUBLE PRECISION NOT NULL,
  session_key TEXT
);
CREATE INDEX memory_traces_by_memory ON memory_traces (memory_id, psych_at, id);

CREATE TEMPORARY TABLE clocked ON COMMIT DROP AS
WITH moments AS (
  SELECT brain_id, created_at AS real_at FROM memories
  UNION
  SELECT brain_id, hit FROM memories, UNNEST(hit_times) AS hit
), gaps AS (
  SELECT brain_id, real_at, real_at - LAG(real_at) OVER (PARTITION BY brain_id ORDER BY real_at) AS gap
  FROM moments
)
SELECT brain_id, real_at,
  COALESCE(SUM(CASE WHEN gap <= 3600000 THEN gap::DOUBLE PRECISION ELSE 0.031 * gap END)
    OVER (PARTITION BY brain_id ORDER BY real_at), 0) AS psych_at
FROM gaps;

INSERT INTO memory_traces (memory_id, kind, real_at, psych_at, decay, salience)
SELECT m.id, 'written', m.created_at, c.psych_at, 0.167, m.importance / 5.0
FROM memories m JOIN clocked c ON c.brain_id = m.brain_id AND c.real_at = m.created_at
ORDER BY m.id;

INSERT INTO memory_traces (memory_id, kind, real_at, psych_at, decay, salience)
SELECT m.id, 'handed', hit.at, c.psych_at, 0.167, 1
FROM memories m CROSS JOIN LATERAL UNNEST(m.hit_times) WITH ORDINALITY AS hit (at, n)
JOIN clocked c ON c.brain_id = m.brain_id AND c.real_at = hit.at
ORDER BY m.id, hit.n;

UPDATE brains b SET clock_real_at = latest.real_at, clock_psych = latest.psych_at
FROM (SELECT DISTINCT ON (brain_id) brain_id, real_at, psych_at FROM clocked ORDER BY brain_id, real_at DESC)
  latest
WHERE latest.brain_id = b.id;

-- The traces stand for both: the hits a row kept and the storage strength confirmations raised.
ALTER TABLE memories DROP COLUMN hit_times;
ALTER TABLE memories DROP COLUMN strength;
