-- A memory's two situations (docs/design.md, 「Recall」): the one that recalls it and the one that ends it, each
-- the writer's words and null where it carries none. Both change with a version, like the memory's text.

ALTER TABLE memories ADD COLUMN recall_condition TEXT;
ALTER TABLE memories ADD COLUMN release_condition TEXT;
