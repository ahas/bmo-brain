-- A row may be fetched by a key the system meets, the triggers a holder names (docs/design.md, 「What a holder
-- leaves」): a tool's name, a notice's kind, a room. A held row says whether it stands on every turn or is reached
-- only by its situation and its triggers. Every row that stands now carries no trigger and stands as it did.
ALTER TABLE public.memories ADD COLUMN triggers text[] NOT NULL DEFAULT '{}';
ALTER TABLE public.memories ADD COLUMN every_turn boolean NOT NULL DEFAULT true;

CREATE INDEX memories_by_trigger ON public.memories USING gin (triggers);

-- Korean words are indexed by their syllable pairs (docs/design.md, 「Recall」): every row holding Hangul is
-- indexed again.
UPDATE public.memories SET words = NULL WHERE text ~ '[가-힣]';
