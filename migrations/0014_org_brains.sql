-- Org brains (docs/design.md, 「Org brains」): a brain's kind, an agent's or a unit's of an organisation, set
-- when it is made, and the two marks an org memory carries on each version, both off in an agent's brain.

ALTER TABLE brains ADD COLUMN kind TEXT NOT NULL DEFAULT 'agent' CHECK (kind IN ('agent', 'org'));

ALTER TABLE memories ADD COLUMN locked BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE memories ADD COLUMN internal_only BOOLEAN NOT NULL DEFAULT FALSE;
