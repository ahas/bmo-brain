-- A memory carries no strength (docs/design.md, 「Use」): it records its use and ends only by an event, so the
-- storage strength confirmations raised is let go of. Its use, the hits, their moments and the latest of them,
-- stays as it was.

ALTER TABLE memories DROP COLUMN strength;
