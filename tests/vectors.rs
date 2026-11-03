//! A memory searched by its meaning (docs/design.md, 「Vectors」), over a stand-in gateway that embeds a text
//! into eight dimensions: one for each of four concepts a few words name, and the rest of its words hashed into
//! the other four, so a paraphrase lands near its original with no word in common.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use bmo_brain::store::Store;
use bmo_brain::store::vectors::EmbeddingConfig;
use bmo_brain_client::{
  BrainClient, MemoriesQuery, NewBrain, NewMemory, Recall, RecallAsk, RecallPart, SimilarAsk,
};
use serde_json::{Value, json};

use crate::support::{Served, labels};

const KEY: &str = "stand-in-key";
const DIMS: usize = 8;

/// The concepts the stand-in places in a dimension of their own, each by the words that name it.
const CONCEPTS: [&[&str]; 4] = [
  &["car", "automobile", "vehicle"],
  &["key", "keys"],
  &["dog", "puppy", "hound"],
  &["garden", "tomato", "tomatoes"],
];

/// The stand-in's vector of a text.
fn embedded(text: &str) -> Vec<f32> {
  let mut vector = vec![0.0_f32; DIMS];
  for word in text
    .to_lowercase()
    .split(|c: char| !c.is_alphanumeric())
    .filter(|word| !word.is_empty())
  {
    match CONCEPTS.iter().position(|names| names.contains(&word)) {
      Some(concept) => vector[concept] += 1.0,
      None => {
        let hashed = word.bytes().map(usize::from).sum::<usize>() % 4;
        vector[4 + hashed] += 0.3;
      }
    }
  }
  vector
}

#[derive(Clone)]
struct Gate {
  down: Arc<AtomicBool>,
  asks: Arc<AtomicUsize>,
}

async fn embeddings(
  State(gate): State<Gate>,
  headers: HeaderMap,
  Json(ask): Json<Value>,
) -> (StatusCode, Json<Value>) {
  gate.asks.fetch_add(1, Ordering::SeqCst);
  if gate.down.load(Ordering::SeqCst) {
    return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "down" })));
  }
  let bearer = headers.get("authorization").and_then(|value| value.to_str().ok());
  if bearer != Some(&format!("Bearer {KEY}")) {
    return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorized" })));
  }
  let texts: Vec<String> = serde_json::from_value(ask["input"].clone()).unwrap_or_default();
  let data: Vec<Value> = texts
    .iter()
    .enumerate()
    .map(|(index, text)| json!({ "object": "embedding", "index": index, "embedding": embedded(text) }))
    .collect();
  (StatusCode::OK, Json(json!({ "object": "list", "data": data })))
}

/// The stand-in gateway answering on a port of its own, and the configuration that names it.
struct Gateway {
  gate: Gate,
  base: String,
  key_file: std::path::PathBuf,
  serving: tokio::task::JoinHandle<()>,
}

impl Gateway {
  async fn new() -> Gateway {
    static FILES: AtomicUsize = AtomicUsize::new(0);
    let key_file = std::env::temp_dir().join(format!(
      "bmo-brain-stand-in-key-{}-{}",
      std::process::id(),
      FILES.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&key_file, format!("{KEY}\n")).unwrap();
    let gate = Gate {
      down: Arc::new(AtomicBool::new(false)),
      asks: Arc::new(AtomicUsize::new(0)),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new().route("/v1/embeddings", post(embeddings)).with_state(gate.clone());
    let serving = tokio::spawn(async move {
      axum::serve(listener, router).await.unwrap();
    });
    Gateway {
      gate,
      base,
      key_file,
      serving,
    }
  }

  fn config(&self, model: &str) -> EmbeddingConfig {
    EmbeddingConfig {
      address: self.base.clone(),
      key_file: self.key_file.clone(),
      model: model.to_owned(),
      dims: DIMS,
      batch: 2,
    }
  }
}

impl Drop for Gateway {
  fn drop(&mut self) {
    self.serving.abort();
    let _ = std::fs::remove_file(&self.key_file);
  }
}

fn memory(text: &str) -> NewMemory {
  NewMemory {
    key: None,
    kind: "fact".to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects: vec![],
    importance: None,
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
  }
}

async fn brain_of(client: &BrainClient) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
    })
    .await
    .unwrap()
    .id
}

async fn recall(client: &BrainClient, brain: i64, words: &str) -> Recall {
  client
    .recall(
      brain,
      &RecallAsk {
        words: words.to_owned(),
        record: Some(false),
        ..RecallAsk::default()
      },
    )
    .await
    .unwrap()
}

async fn vectors_of(store: &Store, model: &str) -> Vec<i64> {
  sqlx::query_scalar("SELECT memory_id FROM memory_vectors WHERE model = $1 ORDER BY memory_id")
    .bind(model)
    .fetch_all(&store.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_write_wakes_the_embedder_and_it_keeps_the_rows_vector() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let mut stored = served.store.vectors.embedded();
  let embedder = bmo_brain::store::vectors::stand(served.store.clone());
  let row = client.memory_add(brain, &memory("The automobile is parked outside.")).await.unwrap();
  while !vectors_of(&served.store, "stand-in").await.contains(&row.id) {
    stored.changed().await.unwrap();
  }
  let (dims, vector): (i32, Vec<f32>) =
    sqlx::query_as("SELECT dims, vector FROM memory_vectors WHERE memory_id = $1")
      .bind(row.id)
      .fetch_one(&served.store.pool)
      .await
      .unwrap();
  assert_eq!((dims, vector), (8, embedded("The automobile is parked outside.")));
  embedder.abort();
}

#[tokio::test]
async fn the_backfill_embeds_every_live_row_lacking_a_vector() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  for text in ["The dog sleeps.", "The garden grows tomatoes.", "The car is red."] {
    client.memory_add(brain, &memory(text)).await.unwrap();
  }
  let before = client.health().await.unwrap().embedding.unwrap();
  assert_eq!((before.vectorised, before.lacking), (0, 3));
  assert_eq!(served.store.embed_backlog().await, Ok(3));
  let after = client.health().await.unwrap().embedding.unwrap();
  assert_eq!(
    (after.vectorised, after.lacking, after.model.as_str(), after.dims),
    (3, 0, "stand-in", 8)
  );
  // Two batches of two at most: the gateway was asked twice.
  assert_eq!(gateway.gate.asks.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_paraphrase_with_no_word_in_common_is_found_by_its_meaning() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let parked = client.memory_add(brain, &memory("The automobile is parked outside.")).await.unwrap();
  client.memory_add(brain, &memory("The garden grows tomatoes.")).await.unwrap();
  served.store.embed_backlog().await.unwrap();

  let turn = recall(client, brain, "where is my car").await;
  let choice = turn.chosen.iter().find(|c| c.memory_id == parked.id).expect("found by meaning");
  assert_eq!((choice.part, choice.score), (RecallPart::Searched, 0.0));
  assert!(choice.vector_similarity.is_some_and(|cosine| cosine > 0.5), "{choice:?}");
  assert!(choice.fused_score.is_some_and(|fused| fused > 0.0), "{choice:?}");

  let likes = client
    .similar(
      brain,
      &SimilarAsk {
        text: "my car".to_owned(),
        subjects: vec![],
        limit: Some(1),
        as_of: None,
      },
    )
    .await
    .unwrap();
  assert_eq!(likes.iter().map(|m| m.id).collect::<Vec<_>>(), [parked.id]);
  let listed = client
    .memories(
      brain,
      &MemoriesQuery {
        q: Some("car".to_owned()),
        limit: Some(1),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(listed.items.iter().map(|m| m.id).collect::<Vec<_>>(), [parked.id]);
  let health = client.health().await.unwrap().embedding.unwrap();
  assert_eq!((health.vector_asks, health.vector_skipped), (3, 0));
}

#[tokio::test]
async fn a_row_high_in_both_lists_stands_above_one_high_in_one() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let both = client.memory_add(brain, &memory("The car keys hang by the door.")).await.unwrap();
  let meaning = client.memory_add(brain, &memory("The automobile is parked outside.")).await.unwrap();
  let words = client.memory_add(brain, &memory("Keys to the shed are lost.")).await.unwrap();
  served.store.embed_backlog().await.unwrap();
  let turn = recall(client, brain, "car keys").await;
  let searched: Vec<i64> = turn
    .chosen
    .iter()
    .filter(|c| c.part == RecallPart::Searched)
    .map(|c| c.memory_id)
    .collect();
  assert_eq!(searched.first(), Some(&both.id), "{:?}", turn.chosen);
  let fused = |id: i64| turn.chosen.iter().find(|c| c.memory_id == id).and_then(|c| c.fused_score).unwrap();
  // First in the word list and first in the vector list: 1 / (60 + 1) twice.
  assert!((fused(both.id) - 2.0 / 61.0).abs() < 1e-12, "{}", fused(both.id));
  assert!(
    fused(both.id) > fused(meaning.id) && fused(both.id) > fused(words.id),
    "{:?}",
    turn.chosen
  );
}

#[tokio::test]
async fn a_gateway_that_does_not_answer_leaves_the_words_alone_and_is_counted() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let parked = client.memory_add(brain, &memory("The automobile is parked outside.")).await.unwrap();
  let car = client.memory_add(brain, &memory("The car is red.")).await.unwrap();
  served.store.embed_backlog().await.unwrap();
  gateway.gate.down.store(true, Ordering::SeqCst);
  let turn = recall(client, brain, "where is my car").await;
  let chosen: Vec<i64> = turn.chosen.iter().map(|c| c.memory_id).collect();
  assert_eq!(chosen, [car.id], "by words alone: {:?}", turn.chosen);
  assert!(turn.chosen.iter().all(|c| c.vector_similarity.is_none()));
  let health = client.health().await.unwrap().embedding.unwrap();
  assert_eq!((health.vector_skipped, health.word_asks, health.vector_asks), (1, 1, 0));
  assert!(health.last_error.is_some());
  // A write while the gateway is down leaves its row lacking, and the backfill says why.
  client.memory_add(brain, &memory("The dog sleeps.")).await.unwrap();
  assert!(served.store.embed_backlog().await.is_err());
  let _ = parked;
}

#[tokio::test]
async fn a_change_of_model_embeds_every_row_again_and_lets_the_old_vectors_go() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("model-a")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let one = client.memory_add(brain, &memory("The dog sleeps.")).await.unwrap();
  let two = client.memory_add(brain, &memory("The car is red.")).await.unwrap();
  served.store.embed_backlog().await.unwrap();
  assert_eq!(vectors_of(&served.store, "model-a").await, [one.id, two.id]);

  let changed = Store::over(served.store.pool.clone()).embedding(Some(gateway.config("model-b")));
  assert_eq!(changed.load_vectors().await, Ok(0));
  let before = changed.embedding_health().await.unwrap();
  assert_eq!((before.vectorised, before.lacking), (0, 2));
  // Until the new model has filled every row the old model's vectors stay; once it has, they go.
  assert_eq!(changed.embed_backlog().await, Ok(2));
  assert_eq!(vectors_of(&changed, "model-b").await, [one.id, two.id]);
  assert!(vectors_of(&changed, "model-a").await.is_empty());
}
