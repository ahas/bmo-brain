-- A holder's reach (docs/design.md, 「What a holder leaves」): the brains its standing set stands in, kept with its
-- key so a brain made later is given only the rows of the holders whose reach takes it in. A holder with no row
-- here reaches every agent's brain, which is every holder that stands now.
CREATE TABLE public.holder_reaches (
    held_by text PRIMARY KEY,
    kind text NOT NULL,
    account_key text NOT NULL,
    agent_key text,
    declared_at bigint NOT NULL
);
