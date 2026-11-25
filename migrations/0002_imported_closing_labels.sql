-- Rows imported from the memory store the brain replaced carried that store's record of their close as labels:
-- the words a row ended on, why it went down and when. The close is the brain's own columns, and no caller reads
-- or writes those labels, so they are removed from every row of every brain, closed rows included, and nothing
-- else is changed.
UPDATE memories
SET labels = labels - ARRAY['done_text', 'down_reason', 'down_at']
WHERE labels ?| ARRAY['done_text', 'down_reason', 'down_at'];
