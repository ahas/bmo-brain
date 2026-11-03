//! A memory searched by its meaning (docs/design.md, 「Vectors」): the embedding model the configuration names,
//! asked over the gateway's embeddings route; each live row's vector of the current model kept beside it and
//! read into the process; the embedder that fills the rows lacking one; and the nearest rows to a query by
//! cosine.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use bmo_brain_client::{EmbeddingHealth, HttpClient};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use super::rules::tokens;
use super::{Held, Store, now};

/// How long the embedder waits before it asks the gateway again after an ask failed.
const RETRY_PAUSE: Duration = Duration::from_secs(30);

/// The most tokens of a text that are embedded.
const TOKENS_EMBEDDED: i64 = 512;

/// The embedding model's settings (docs/boundary-api.md, 「Configuration」).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingConfig {
  /// The LLM gateway's base address, whose `/v1/embeddings` is asked.
  pub address: String,
  /// The file holding the gateway's bearer key, read at each ask.
  pub key_file: PathBuf,
  /// The model name; `embedding` where none is given.
  #[serde(default = "default_model")]
  pub model: String,
  /// The vectors' length.
  #[serde(default = "default_dims")]
  pub dims: usize,
  /// The most texts one ask carries.
  #[serde(default = "default_batch")]
  pub batch: usize,
}

fn default_model() -> String {
  "embedding".to_owned()
}

fn default_dims() -> usize {
  1024
}

fn default_batch() -> usize {
  32
}

/// One row's vector as the index holds it: its brain, and the vector scaled to length one.
struct Entry {
  brain_id: i64,
  unit: Vec<f32>,
}

/// The vectors of the current model, the embedder's wake, and what the asks came to.
pub struct Vectors {
  config: Option<EmbeddingConfig>,
  http: Option<HttpClient>,
  index: RwLock<HashMap<i64, Entry>>,
  wake: Notify,
  embedded: watch::Sender<u64>,
  vector_asks: AtomicI64,
  word_asks: AtomicI64,
  skipped: AtomicI64,
  last_error: Mutex<Option<String>>,
}

impl std::fmt::Debug for Vectors {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    out.debug_struct("Vectors").field("config", &self.config).finish_non_exhaustive()
  }
}

impl Vectors {
  /// The vectors of the model the configuration names, or none where it names none.
  pub fn new(config: Option<EmbeddingConfig>) -> Self {
    let http = config.as_ref().and_then(|_| HttpClient::builder().no_proxy().build().ok());
    Self {
      config,
      http,
      index: RwLock::new(HashMap::new()),
      wake: Notify::new(),
      embedded: watch::Sender::new(0),
      vector_asks: AtomicI64::new(0),
      word_asks: AtomicI64::new(0),
      skipped: AtomicI64::new(0),
      last_error: Mutex::new(None),
    }
  }

  /// Whether an embedding model is named.
  pub fn configured(&self) -> bool {
    self.config.is_some()
  }

  /// The embedder woken: a row may lack a vector.
  pub fn wake(&self) {
    if self.config.is_some() {
      self.wake.notify_one();
    }
  }

  /// A signal raised after every batch the embedder stores.
  pub fn embedded(&self) -> watch::Receiver<u64> {
    self.embedded.subscribe()
  }

  fn failed(&self, why: &str) {
    if let Ok(mut last) = self.last_error.lock() {
      *last = Some(why.to_owned());
    }
  }

  fn answered(&self) {
    if let Ok(mut last) = self.last_error.lock() {
      *last = None;
    }
  }

  /// The texts embedded by the gateway, in their order, or why not.
  async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    let (Some(config), Some(http)) = (&self.config, &self.http) else {
      return Err("no embedding model is named".to_owned());
    };
    let key = std::fs::read_to_string(&config.key_file)
      .map_err(|why| format!("{}: {why}", config.key_file.display()))?;
    let address = format!("{}/v1/embeddings", config.address.trim_end_matches('/'));
    let answer = http
      .post(&address)
      .bearer_auth(key.trim())
      .json(&EmbeddingAsk {
        model: &config.model,
        input: texts,
      })
      .send()
      .await
      .map_err(|why| format!("{address}: {why}"))?;
    let status = answer.status();
    if !status.is_success() {
      return Err(format!("{address} answered {status}"));
    }
    let mut answered: EmbeddingAnswer = answer.json().await.map_err(|why| format!("{address}: {why}"))?;
    answered.data.sort_by_key(|datum| datum.index);
    if answered.data.len() != texts.len() {
      return Err(format!("{address} answered {} vectors for {} texts", answered.data.len(), texts.len()));
    }
    let vectors: Vec<Vec<f32>> = answered.data.into_iter().map(|datum| datum.embedding).collect();
    if let Some(wrong) = vectors.iter().find(|vector| vector.len() != config.dims) {
      return Err(format!(
        "{address} answered {} dimensions; the configuration names {}",
        wrong.len(),
        config.dims
      ));
    }
    Ok(vectors)
  }
}

#[derive(Serialize)]
struct EmbeddingAsk<'a> {
  model: &'a str,
  input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingAnswer {
  data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
  embedding: Vec<f32>,
  #[serde(default)]
  index: usize,
}

/// A vector scaled to length one; none where it has no length.
fn unit(vector: &[f32]) -> Option<Vec<f32>> {
  let length = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
  (length > 0.0).then(|| vector.iter().map(|x| x / length).collect())
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
  a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// What of a row is embedded: its text, with its recall condition after it, cut to the tokens embedded.
pub(crate) fn embedded_text(text: &str, recall_condition: Option<&str>) -> String {
  let whole = match recall_condition {
    Some(condition) => format!("{text}\n{condition}"),
    None => text.to_owned(),
  };
  let most = usize::try_from(TOKENS_EMBEDDED * 3).unwrap_or(usize::MAX);
  if tokens(&whole) <= TOKENS_EMBEDDED {
    whole
  } else {
    whole.chars().take(most).collect()
  }
}

/// A query embedded for one ask: its unit vector, where the model answered.
pub(crate) struct Query {
  pub unit: Vec<f32>,
}

impl Store {
  /// The vectors of the current model of every live row, read into the process.
  pub async fn load_vectors(&self) -> Held<usize> {
    let Some(config) = &self.vectors.config else {
      return Ok(0);
    };
    let rows: Vec<(i64, i64, Vec<f32>)> = sqlx::query_as(
      "SELECT v.memory_id, m.brain_id, v.vector FROM memory_vectors v JOIN memories m ON m.id = v.memory_id \
       WHERE v.model = $1 AND m.status IN ('active', 'pending')",
    )
    .bind(&config.model)
    .fetch_all(&self.pool)
    .await?;
    let mut index = self.vectors.index.write().unwrap_or_else(|poisoned| poisoned.into_inner());
    index.clear();
    for (memory_id, brain_id, vector) in rows {
      if let Some(unit) = unit(&vector) {
        index.insert(memory_id, Entry { brain_id, unit });
      }
    }
    Ok(index.len())
  }

  /// Every live row lacking a vector of the current model embedded and kept, a batch at a time; once none
  /// lacks one, the other models' vectors deleted. How many rows it embedded, or why the gateway did not.
  pub async fn embed_backlog(&self) -> Result<i64, String> {
    let Some(config) = self.vectors.config.clone() else {
      return Ok(0);
    };
    let mut embedded = 0;
    loop {
      let lacking: Vec<(i64, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT m.id, m.brain_id, m.text, m.recall_condition FROM memories m WHERE m.status IN ('active', 'pending') \
         AND NOT EXISTS (SELECT 1 FROM memory_vectors v WHERE v.memory_id = m.id AND v.model = $1) \
         ORDER BY m.id LIMIT $2",
      )
      .bind(&config.model)
      .bind(i64::try_from(config.batch.max(1)).unwrap_or(32))
      .fetch_all(&self.pool)
      .await
      .map_err(|why| why.to_string())?;
      if lacking.is_empty() {
        sqlx::query("DELETE FROM memory_vectors WHERE model <> $1")
          .bind(&config.model)
          .execute(&self.pool)
          .await
          .map_err(|why| why.to_string())?;
        return Ok(embedded);
      }
      let texts: Vec<String> = lacking
        .iter()
        .map(|(_, _, text, condition)| embedded_text(text, condition.as_deref()))
        .collect();
      let vectors = match self.vectors.embed(&texts).await {
        Ok(vectors) => vectors,
        Err(why) => {
          self.vectors.failed(&why);
          return Err(why);
        }
      };
      self.vectors.answered();
      let at = now();
      for ((memory_id, brain_id, _, _), vector) in lacking.iter().zip(&vectors) {
        sqlx::query(
          "INSERT INTO memory_vectors (memory_id, model, dims, vector, embedded_at) VALUES ($1, $2, $3, $4, $5) \
           ON CONFLICT (memory_id, model) DO UPDATE SET dims = $3, vector = $4, embedded_at = $5",
        )
        .bind(memory_id)
        .bind(&config.model)
        .bind(i32::try_from(vector.len()).unwrap_or(i32::MAX))
        .bind(vector)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(|why| why.to_string())?;
        if let Some(unit) = unit(vector) {
          let mut index = self.vectors.index.write().unwrap_or_else(|poisoned| poisoned.into_inner());
          index.insert(
            *memory_id,
            Entry {
              brain_id: *brain_id,
              unit,
            },
          );
        }
        embedded += 1;
      }
      self.vectors.embedded.send_modify(|count| *count = count.wrapping_add(1));
    }
  }

  /// A query's words embedded for one ask, counted as an ask with vectors, by words alone, or skipped where the
  /// gateway did not answer; none where it was not embedded.
  pub(crate) async fn query_vector(&self, words: &str) -> Option<Query> {
    if words.trim().is_empty() {
      return None;
    }
    let vectors = &self.vectors;
    if !vectors.configured() {
      vectors.word_asks.fetch_add(1, Ordering::Relaxed);
      return None;
    }
    match vectors.embed(&[embedded_text(words, None)]).await {
      Ok(mut answered) => {
        vectors.answered();
        match answered.pop().as_deref().and_then(unit) {
          Some(unit) => {
            vectors.vector_asks.fetch_add(1, Ordering::Relaxed);
            Some(Query { unit })
          }
          None => {
            vectors.word_asks.fetch_add(1, Ordering::Relaxed);
            None
          }
        }
      }
      Err(why) => {
        vectors.failed(&why);
        vectors.skipped.fetch_add(1, Ordering::Relaxed);
        vectors.word_asks.fetch_add(1, Ordering::Relaxed);
        None
      }
    }
  }

  /// The rows of a brain nearest a query by cosine, nearest first, each with its cosine.
  pub(crate) fn nearest(&self, brain_id: i64, query: &Query) -> Vec<(i64, f64)> {
    let index = self.vectors.index.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut near: Vec<(i64, f64)> = index
      .iter()
      .filter(|(_, entry)| entry.brain_id == brain_id)
      .map(|(id, entry)| (*id, f64::from(dot(&entry.unit, &query.unit))))
      .collect();
    near.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    near
  }

  /// These rows let go of the index: they are no longer live.
  pub(crate) fn forget_vectors(&self, ids: &[i64]) {
    if ids.is_empty() {
      return;
    }
    let mut index = self.vectors.index.write().unwrap_or_else(|poisoned| poisoned.into_inner());
    for id in ids {
      index.remove(id);
    }
  }

  /// The embedding model's settings and what its vectors and asks came to, for the health answer.
  pub async fn embedding_health(&self) -> Option<EmbeddingHealth> {
    let config = self.vectors.config.as_ref()?;
    let counts: Result<(i64, i64), sqlx::Error> = sqlx::query_as(
      "SELECT COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM memory_vectors v WHERE v.memory_id = m.id \
       AND v.model = $1)), COUNT(*) FILTER (WHERE NOT EXISTS (SELECT 1 FROM memory_vectors v \
       WHERE v.memory_id = m.id AND v.model = $1)) FROM memories m WHERE m.status IN ('active', 'pending')",
    )
    .bind(&config.model)
    .fetch_one(&self.pool)
    .await;
    let (vectorised, lacking) = counts.unwrap_or_default();
    let vectors = &self.vectors;
    Some(EmbeddingHealth {
      address: config.address.clone(),
      key_file: config.key_file.display().to_string(),
      model: config.model.clone(),
      dims: i64::try_from(config.dims).unwrap_or(i64::MAX),
      batch: i64::try_from(config.batch).unwrap_or(i64::MAX),
      vectorised,
      lacking,
      vector_asks: vectors.vector_asks.load(Ordering::Relaxed),
      word_asks: vectors.word_asks.load(Ordering::Relaxed),
      vector_skipped: vectors.skipped.load(Ordering::Relaxed),
      last_error: vectors.last_error.lock().ok().and_then(|last| last.clone()),
    })
  }
}

/// The embedder, running until it is aborted: it fills the rows lacking a vector whenever a write wakes it, and
/// after a pause where the gateway did not answer.
pub fn stand(store: Store) -> JoinHandle<()> {
  tokio::spawn(async move {
    if !store.vectors.configured() {
      return;
    }
    loop {
      match store.embed_backlog().await {
        Ok(_) => store.vectors.wake.notified().await,
        Err(why) => {
          eprintln!("bmo-brain: the embedder could not embed: {why}");
          tokio::select! {
            () = tokio::time::sleep(RETRY_PAUSE) => {}
            () = store.vectors.wake.notified() => {}
          }
        }
      }
    }
  })
}

/// How many of the nearest rows a search reads past the ones it keeps, for the rows its own filters leave out.
const SCANNED_PER_KEPT: usize = 10;

/// Reciprocal Rank Fusion of ranked lists of ids, each id with the score it is ranked by, highest first: each
/// id's sum over the lists it stands in of 1 / (k + its rank there), a rank counted from one and shared by the
/// ids of equal score, so rows a list cannot tell apart stay equal.
pub(crate) fn fuse(lists: &[&[(i64, f64)]], k: f64) -> HashMap<i64, f64> {
  let mut fused: HashMap<i64, f64> = HashMap::new();
  for list in lists {
    let mut rank = 0;
    for (place, (id, score)) in list.iter().enumerate() {
      if place == 0 || list[place - 1].1 != *score {
        rank = place + 1;
      }
      *fused.entry(*id).or_default() += 1.0 / (k + rank as f64);
    }
  }
  fused
}

impl Store {
  /// The live rows of a brain nearest a query, nearest first with their cosine, as many as a search keeping
  /// `kept` of them reads; rows found no longer live let go of the index.
  pub(crate) async fn nearest_live(
    &self,
    conn: &mut sqlx::PgConnection,
    brain_id: i64,
    query: &Query,
    kept: usize,
  ) -> Held<Vec<(i64, f64)>> {
    let mut near = self.nearest(brain_id, query);
    near.truncate(kept.saturating_mul(SCANNED_PER_KEPT));
    let ids: Vec<i64> = near.iter().map(|(id, _)| *id).collect();
    let dead: Vec<i64> = sqlx::query_scalar(
      "SELECT id FROM memories WHERE id = ANY($1) AND status NOT IN ('active', 'pending')",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    self.forget_vectors(&dead);
    near.retain(|(id, _)| !dead.contains(id));
    Ok(near)
  }
}
