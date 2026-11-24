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
  BrainClient, DuplicatesQuery, MemoriesQuery, NewBrain, NewMemory, Recall, RecallAsk, RecallPart,
  SimilarAsk, SubjectKey,
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
  /// The most characters of a text the stand-in takes, none where zero; it refuses a batch holding a longer one,
  /// and always refuses a text that says `unembeddable`, as an engine refuses a text past its context.
  limit: Arc<AtomicUsize>,
  /// Every text it was handed, in order.
  seen: Arc<std::sync::Mutex<Vec<String>>>,
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
  gate.seen.lock().unwrap().extend(texts.iter().cloned());
  let limit = gate.limit.load(Ordering::SeqCst);
  if texts
    .iter()
    .any(|text| text.contains("unembeddable") || (limit > 0 && text.chars().count() > limit))
  {
    return (StatusCode::BAD_REQUEST, Json(json!({ "error": "the input exceeds the context size" })));
  }
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
      limit: Arc::new(AtomicUsize::new(0)),
      seen: Arc::new(std::sync::Mutex::new(Vec::new())),
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
      max_tokens: 480,
      // The stand-in reads every word of a text, so the tests embed queries bare unless one asks otherwise.
      query_instruction: String::new(),
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
    internal_only: None,
    shared: None,
    locked: None,
    standing: None,
  }
}

async fn brain_of(client: &BrainClient) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
      kind: None,
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

#[test]
fn a_text_is_cut_by_a_conservative_count_of_its_tokens() {
  use bmo_brain::store::vectors::{cut, estimated_tokens, query_text};
  assert_eq!(estimated_tokens("\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}"), 5.0);
  assert_eq!(estimated_tokens("abcdef"), 2.0);
  assert_eq!(estimated_tokens("\u{6f22}\u{5b57}\u{304b}\u{306a}"), 4.0);
  assert_eq!(cut("\u{ac00}\u{b098}\u{b2e4}\u{b77c}\u{b9c8}", 3.0), "\u{ac00}\u{b098}\u{b2e4}");
  assert_eq!(cut("abcdefghi", 2.0), "abcdef");
  assert_eq!(cut("ab\u{ac00}\u{b098}", 2.0), "ab\u{ac00}");
  assert_eq!(cut("short", 480.0), "short");
  // A Korean text of 600 syllables is cut at 480 of them, under an engine's 512.
  let long = "\u{ac00}".repeat(600);
  assert_eq!(cut(&long, 480.0).chars().count(), 480);
  // A query's words are cut so the instruction and the words together stay within the count.
  let asked = query_text(&long, "Given a question, retrieve memories that answer it", 480.0);
  assert!(
    asked.starts_with("Instruct: Given a question, retrieve memories that answer it\nQuery:"),
    "{asked}"
  );
  assert!(estimated_tokens(&asked) <= 480.0, "{}", estimated_tokens(&asked));
  assert_eq!(query_text("my car", "", 480.0), "my car");
}

#[tokio::test]
async fn a_refused_batch_is_embedded_one_by_one_cut_shorter_and_a_text_refused_even_then_is_recorded() {
  let gateway = Gateway::new().await;
  // The stand-in takes texts of 200 characters at most: a text of 300 is taken once cut to 64 tokens.
  gateway.gate.limit.store(200, Ordering::SeqCst);
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let short = client.memory_add(brain, &memory("The dog sleeps.")).await.unwrap();
  let long = client.memory_add(brain, &memory(&"garden ".repeat(43))).await.unwrap();
  let refused = client.memory_add(brain, &memory("An unembeddable row.")).await.unwrap();
  let after = client.memory_add(brain, &memory("The car is red.")).await.unwrap();
  assert_eq!(served.store.embed_backlog().await, Ok(3));
  assert_eq!(vectors_of(&served.store, "stand-in").await, [short.id, long.id, after.id]);
  let health = client.health().await.unwrap().embedding.unwrap();
  assert_eq!((health.vectorised, health.lacking, health.rejected), (3, 0, 1));
  // The long text went out whole, then cut at 240, 120 and 64 tokens, the last of 192 characters taken.
  let seen = gateway.gate.seen.lock().unwrap().clone();
  let long_sent: Vec<usize> = seen
    .iter()
    .filter(|text| text.starts_with("garden"))
    .map(|text| text.chars().count())
    .collect();
  assert_eq!(long_sent.last(), Some(&192), "{long_sent:?}");
  // The refused row is passed over from then on: nothing more is asked of the gateway for it.
  let asked = gateway.gate.asks.load(Ordering::SeqCst);
  assert_eq!(served.store.embed_backlog().await, Ok(0));
  assert_eq!(gateway.gate.asks.load(Ordering::SeqCst), asked);
  let _ = refused;
}

#[tokio::test]
async fn a_query_is_embedded_after_the_models_instruction() {
  let gateway = Gateway::new().await;
  let mut config = gateway.config("stand-in");
  config.query_instruction = "Given a question, retrieve memories that answer it".to_owned();
  let served = Served::embedding(config).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  client.memory_add(brain, &memory("The automobile is parked outside.")).await.unwrap();
  served.store.embed_backlog().await.unwrap();
  recall(client, brain, "where is my car").await;
  let seen = gateway.gate.seen.lock().unwrap().clone();
  assert_eq!(
    seen.first().map(String::as_str),
    Some("The automobile is parked outside."),
    "a row goes raw"
  );
  assert_eq!(
    seen.last().map(String::as_str),
    Some("Instruct: Given a question, retrieve memories that answer it\nQuery:where is my car")
  );
}

#[tokio::test]
async fn a_gateway_that_refuses_every_ask_records_no_row_as_refused() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  client.memory_add(brain, &memory("The dog sleeps.")).await.unwrap();
  // Every text of more than one character refused, as a gateway refuses a model out of its table.
  gateway.gate.limit.store(1, Ordering::SeqCst);
  assert!(served.store.embed_backlog().await.is_err());
  let health = client.health().await.unwrap().embedding.unwrap();
  assert_eq!((health.vectorised, health.lacking, health.rejected), (0, 1, 0));
  // Once it takes texts again, the row is embedded.
  gateway.gate.limit.store(0, Ordering::SeqCst);
  assert_eq!(served.store.embed_backlog().await, Ok(1));
}

#[tokio::test]
async fn two_rows_about_one_subject_close_in_meaning_are_a_duplicate_candidate_with_their_cosine() {
  let gateway = Gateway::new().await;
  let served = Served::embedding(gateway.config("stand-in")).await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = SubjectKey {
    kind: "person".to_owned(),
    key: "person:a".to_owned(),
  };
  let about = |text: &str| NewMemory {
    subjects: vec![person.clone()],
    ..memory(text)
  };
  let first = client.memory_add(brain, &about("Member drives the automobile")).await.unwrap();
  let second = client.memory_add(brain, &about("Member drives the car")).await.unwrap();
  client.memory_add(brain, &about("Member grows tomatoes")).await.unwrap();
  served.store.embed_backlog().await.unwrap();

  let pairs = client.duplicates(brain, &DuplicatesQuery::default()).await.unwrap();

  // Two of four stems shared, under the word share; the same concept in meaning, over the cosine.
  assert_eq!(
    pairs.iter().map(|p| (p.first.id, p.second.id)).collect::<Vec<_>>(),
    [(first.id, second.id)]
  );
  assert!((pairs[0].word_share - 0.5).abs() < 1e-9, "{:?}", pairs[0]);
  assert!(pairs[0].vector_similarity.is_some_and(|cosine| cosine > 0.99), "{:?}", pairs[0]);
}
