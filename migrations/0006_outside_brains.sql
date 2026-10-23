-- Nothing stands outside a brain (docs/design.md, 「What the brain holds besides brains」): the memory rows held
-- under a caller's scope, the fragments and their kinds go, and with them the events only they published.

DROP TABLE scoped_memories;
DROP TABLE fragments;
DROP TABLE fragment_kinds;

DELETE FROM events WHERE kind IN ('memory.changed', 'fragment.changed');
