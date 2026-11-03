-- A memory's stems (docs/design.md, 「Recall」): the words of its text, lowercased, English stop words left
-- out, each reduced to its Snowball stem, each once. Null until the brain indexes the row, which it does for
-- every live row before a recall or a similar ask reads them.

ALTER TABLE memories ADD COLUMN words TEXT[];
CREATE INDEX memories_by_word ON memories USING GIN (words);
