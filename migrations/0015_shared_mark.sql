-- The third mark of an org memory (docs/design.md, 「Org brains」): whether it is a memory about a person that may
-- be known inside the organisation, off in an agent's brain and wherever the writer does not say.

ALTER TABLE memories ADD COLUMN shared BOOLEAN NOT NULL DEFAULT FALSE;
