-- A holder leaves rows that stand in every agent's brain until it withdraws them, so each row records whose it is
-- and the rows of one key are read, applied and closed together (docs/design.md, 「What a holder leaves」). The
-- mark is null on every row a brain's own writers wrote, which is every row that stands now.
ALTER TABLE public.memories ADD COLUMN held_by text;

CREATE INDEX memories_by_holder ON public.memories USING btree (held_by, brain_id, id) WHERE (held_by IS NOT NULL);
