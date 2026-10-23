-- Why a memory's status was moved, as the mover said (docs/boundary-api.md, 「Memories」); null where nothing
-- was said.

ALTER TABLE memories ADD COLUMN reason TEXT;
