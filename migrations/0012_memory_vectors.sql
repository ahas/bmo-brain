-- A memory's vectors (docs/design.md, 「Vectors」): each row's text embedded by a model, kept under that model's
-- name. The nearest rows to a query are found in the process, over the vectors it reads in.

CREATE TABLE memory_vectors (
  memory_id BIGINT NOT NULL REFERENCES memories (id) ON DELETE CASCADE,
  model TEXT NOT NULL,
  dims INTEGER NOT NULL,
  vector REAL[] NOT NULL,
  embedded_at BIGINT NOT NULL,
  PRIMARY KEY (memory_id, model)
);
CREATE INDEX memory_vectors_by_model ON memory_vectors (model, memory_id);
