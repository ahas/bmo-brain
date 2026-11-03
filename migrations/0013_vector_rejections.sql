-- The rows a model refused to embed even cut short (docs/design.md, 「Vectors」), each under the model that
-- refused it, so the embedder passes them over until the model changes.

CREATE TABLE memory_vector_rejections (
  memory_id BIGINT NOT NULL REFERENCES memories (id) ON DELETE CASCADE,
  model TEXT NOT NULL,
  rejected_at BIGINT NOT NULL,
  PRIMARY KEY (memory_id, model)
);
