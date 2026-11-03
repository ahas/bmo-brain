-- When a row stopped being current (docs/design.md, 「Versions」): the moment of the version, merge or move that
-- closed it. A row superseded before the column stood takes its successor's moment of writing; a row closed by
-- a move before then has none.

ALTER TABLE memories ADD COLUMN superseded_at BIGINT;

UPDATE memories AS replaced SET superseded_at = successor.created_at
FROM memories AS successor
WHERE replaced.superseded_by = successor.id AND replaced.status = 'superseded';
