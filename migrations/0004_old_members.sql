-- The members of the old shape that nothing reads go (docs/design.md, 「What the brain holds besides brains」).
-- The memory rows and the fragments stay for the console's screens; the fragment kinds keep the descriptions
-- the libraries gave them, the last library put naming a kind giving its words.

CREATE TABLE fragment_kinds (
  name TEXT PRIMARY KEY,
  description TEXT NOT NULL
);

INSERT INTO fragment_kinds (name, description)
SELECT DISTINCT ON (named.kind ->> 'name') named.kind ->> 'name', COALESCE(named.kind ->> 'description', '')
FROM libraries, jsonb_array_elements(libraries.kinds) WITH ORDINALITY AS named (kind, position)
WHERE named.kind ->> 'name' IS NOT NULL
ORDER BY named.kind ->> 'name', libraries.changed_at DESC, libraries.name DESC, named.position DESC;

DROP TABLE notes;
DROP TABLE watches;
DROP TABLE reminders;
DROP TABLE locks;
DROP TABLE libraries;
DROP TABLE room_rules;
DROP TABLE session_loads;
DROP TABLE session_notes;
DROP TABLE memory_matches;
DROP TABLE scopes;
DROP TABLE fragment_hits;
