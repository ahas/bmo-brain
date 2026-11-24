-- The brain's schema at 1.0.0, whole (docs/design.md, 「The schema」). Every time is epoch milliseconds.

CREATE TABLE public.brains (
    id bigint NOT NULL,
    account_key text NOT NULL,
    agent_key text NOT NULL,
    name text NOT NULL,
    created_at bigint NOT NULL,
    kind text DEFAULT 'agent'::text NOT NULL,
    CONSTRAINT brains_kind_check CHECK ((kind = ANY (ARRAY['agent'::text, 'org'::text])))
);

CREATE SEQUENCE public.brains_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.brains_id_seq OWNED BY public.brains.id;

CREATE TABLE public.event_floor (
    only_row boolean DEFAULT true NOT NULL,
    pruned_through bigint NOT NULL,
    CONSTRAINT event_floor_only_row_check CHECK (only_row)
);

CREATE TABLE public.events (
    id bigint NOT NULL,
    kind text NOT NULL,
    scope text,
    at bigint NOT NULL,
    data jsonb NOT NULL
);

CREATE SEQUENCE public.events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.events_id_seq OWNED BY public.events.id;

CREATE TABLE public.handouts (
    brain_id bigint NOT NULL,
    session_key text NOT NULL,
    memory_id bigint NOT NULL,
    at bigint NOT NULL
);

CREATE TABLE public.layers (
    id bigint NOT NULL,
    brain_id bigint NOT NULL,
    name text NOT NULL,
    description text NOT NULL,
    "position" bigint NOT NULL,
    created_at bigint NOT NULL
);

CREATE SEQUENCE public.layers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.layers_id_seq OWNED BY public.layers.id;

CREATE TABLE public.memories (
    id bigint NOT NULL,
    brain_id bigint NOT NULL,
    key text,
    type text NOT NULL,
    text text NOT NULL,
    layer_id bigint,
    "position" bigint DEFAULT 0 NOT NULL,
    status text NOT NULL,
    lineage_id bigint NOT NULL,
    superseded_by bigint,
    valid_from bigint NOT NULL,
    valid_to bigint,
    importance smallint NOT NULL,
    hits bigint DEFAULT 0 NOT NULL,
    hit_times bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    last_hit_at bigint,
    last_confirmed_at bigint,
    consolidated_at bigint,
    actor_name text NOT NULL,
    labels jsonb NOT NULL,
    created_at bigint NOT NULL,
    reason text,
    recall_condition text,
    release_condition text,
    words text[],
    superseded_at bigint,
    locked boolean DEFAULT false NOT NULL,
    internal_only boolean DEFAULT false NOT NULL,
    shared boolean DEFAULT false NOT NULL
);

CREATE SEQUENCE public.memories_id_seq1
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.memories_id_seq1 OWNED BY public.memories.id;

CREATE TABLE public.memory_subjects (
    memory_id bigint NOT NULL,
    subject_id bigint NOT NULL
);

CREATE TABLE public.memory_vector_rejections (
    memory_id bigint NOT NULL,
    model text NOT NULL,
    rejected_at bigint NOT NULL
);

CREATE TABLE public.memory_vectors (
    memory_id bigint NOT NULL,
    model text NOT NULL,
    dims integer NOT NULL,
    vector real[] NOT NULL,
    embedded_at bigint NOT NULL
);

CREATE TABLE public.search_counts (
    brain_id bigint NOT NULL,
    asks bigint DEFAULT 0 NOT NULL,
    misses bigint DEFAULT 0 NOT NULL
);

CREATE TABLE public.search_misses (
    id bigint NOT NULL,
    brain_id bigint NOT NULL,
    at bigint NOT NULL,
    route text NOT NULL,
    words text NOT NULL,
    stems text[] NOT NULL
);

CREATE SEQUENCE public.search_misses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.search_misses_id_seq OWNED BY public.search_misses.id;

CREATE TABLE public.subjects (
    id bigint NOT NULL,
    brain_id bigint NOT NULL,
    type text NOT NULL,
    key text NOT NULL,
    description text NOT NULL,
    created_at bigint NOT NULL
);

CREATE SEQUENCE public.subjects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.subjects_id_seq OWNED BY public.subjects.id;

ALTER TABLE ONLY public.brains ALTER COLUMN id SET DEFAULT nextval('public.brains_id_seq'::regclass);

ALTER TABLE ONLY public.events ALTER COLUMN id SET DEFAULT nextval('public.events_id_seq'::regclass);

ALTER TABLE ONLY public.layers ALTER COLUMN id SET DEFAULT nextval('public.layers_id_seq'::regclass);

ALTER TABLE ONLY public.memories ALTER COLUMN id SET DEFAULT nextval('public.memories_id_seq1'::regclass);

ALTER TABLE ONLY public.search_misses ALTER COLUMN id SET DEFAULT nextval('public.search_misses_id_seq'::regclass);

ALTER TABLE ONLY public.subjects ALTER COLUMN id SET DEFAULT nextval('public.subjects_id_seq'::regclass);

ALTER TABLE ONLY public.brains
    ADD CONSTRAINT brains_account_key_agent_key_key UNIQUE (account_key, agent_key);

ALTER TABLE ONLY public.brains
    ADD CONSTRAINT brains_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.event_floor
    ADD CONSTRAINT event_floor_pkey PRIMARY KEY (only_row);

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.handouts
    ADD CONSTRAINT handouts_pkey PRIMARY KEY (brain_id, session_key, memory_id);

ALTER TABLE ONLY public.layers
    ADD CONSTRAINT layers_brain_id_name_key UNIQUE (brain_id, name);

ALTER TABLE ONLY public.layers
    ADD CONSTRAINT layers_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.memories
    ADD CONSTRAINT memories_pkey1 PRIMARY KEY (id);

ALTER TABLE ONLY public.memory_subjects
    ADD CONSTRAINT memory_subjects_pkey PRIMARY KEY (memory_id, subject_id);

ALTER TABLE ONLY public.memory_vector_rejections
    ADD CONSTRAINT memory_vector_rejections_pkey PRIMARY KEY (memory_id, model);

ALTER TABLE ONLY public.memory_vectors
    ADD CONSTRAINT memory_vectors_pkey PRIMARY KEY (memory_id, model);

ALTER TABLE ONLY public.search_counts
    ADD CONSTRAINT search_counts_pkey PRIMARY KEY (brain_id);

ALTER TABLE ONLY public.search_misses
    ADD CONSTRAINT search_misses_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.subjects
    ADD CONSTRAINT subjects_brain_id_type_key_key UNIQUE (brain_id, type, key);

ALTER TABLE ONLY public.subjects
    ADD CONSTRAINT subjects_pkey PRIMARY KEY (id);

CREATE INDEX events_by_time ON public.events USING btree (at);

CREATE INDEX memories_by_brain ON public.memories USING btree (brain_id, id);

CREATE INDEX memories_by_layer ON public.memories USING btree (layer_id, "position", id) WHERE (layer_id IS NOT NULL);

CREATE INDEX memories_by_lineage ON public.memories USING btree (lineage_id, id);

CREATE INDEX memories_by_type ON public.memories USING btree (brain_id, type, id);

CREATE INDEX memories_by_word ON public.memories USING gin (words);

CREATE UNIQUE INDEX memories_live_key ON public.memories USING btree (brain_id, key) WHERE ((key IS NOT NULL) AND (status = ANY (ARRAY['active'::text, 'pending'::text])));

CREATE INDEX memory_subjects_by_subject ON public.memory_subjects USING btree (subject_id, memory_id);

CREATE INDEX memory_vectors_by_model ON public.memory_vectors USING btree (model, memory_id);

CREATE INDEX search_misses_by_brain ON public.search_misses USING btree (brain_id, id);

ALTER TABLE ONLY public.handouts
    ADD CONSTRAINT handouts_brain_id_fkey FOREIGN KEY (brain_id) REFERENCES public.brains(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.handouts
    ADD CONSTRAINT handouts_memory_id_fkey FOREIGN KEY (memory_id) REFERENCES public.memories(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.layers
    ADD CONSTRAINT layers_brain_id_fkey FOREIGN KEY (brain_id) REFERENCES public.brains(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.memories
    ADD CONSTRAINT memories_brain_id_fkey FOREIGN KEY (brain_id) REFERENCES public.brains(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.memories
    ADD CONSTRAINT memories_layer_id_fkey FOREIGN KEY (layer_id) REFERENCES public.layers(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.memory_subjects
    ADD CONSTRAINT memory_subjects_memory_id_fkey FOREIGN KEY (memory_id) REFERENCES public.memories(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.memory_subjects
    ADD CONSTRAINT memory_subjects_subject_id_fkey FOREIGN KEY (subject_id) REFERENCES public.subjects(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.memory_vector_rejections
    ADD CONSTRAINT memory_vector_rejections_memory_id_fkey FOREIGN KEY (memory_id) REFERENCES public.memories(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.memory_vectors
    ADD CONSTRAINT memory_vectors_memory_id_fkey FOREIGN KEY (memory_id) REFERENCES public.memories(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.search_counts
    ADD CONSTRAINT search_counts_brain_id_fkey FOREIGN KEY (brain_id) REFERENCES public.brains(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.search_misses
    ADD CONSTRAINT search_misses_brain_id_fkey FOREIGN KEY (brain_id) REFERENCES public.brains(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.subjects
    ADD CONSTRAINT subjects_brain_id_fkey FOREIGN KEY (brain_id) REFERENCES public.brains(id) ON DELETE CASCADE;


-- The one row of the event floor: nothing let go of yet.
INSERT INTO public.event_floor (pruned_through) VALUES (0);
