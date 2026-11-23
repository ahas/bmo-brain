//! The candidates of the base raised from a source brain (docs/design.md, 「Raising a memory to the base」;
//! docs/boundary-api.md, 「Raising candidates」): the source sifted by its subjects, each memory left judged by the
//! classifier the configuration names, one chat completion each, and a candidate written as a pending row of the
//! base unless the run is dry. It is the administrator's command; the service never runs it.

use std::path::PathBuf;

use bmo_brain_client::{HttpClient, Memory};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::Store;
use crate::store::base::PassOver;

/// The chat model the candidates are judged by.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassifierConfig {
  /// The LLM gateway's base address, whose `/v1/chat/completions` is asked.
  pub address: String,
  /// The file holding the gateway's bearer key, read at each ask.
  pub key_file: PathBuf,
  /// The registration's name.
  pub model: String,
  /// The answer's ceiling.
  #[serde(default = "default_max_tokens")]
  pub max_tokens: i64,
  /// How many asks stand at once.
  #[serde(default = "default_concurrency")]
  pub concurrency: usize,
}

fn default_max_tokens() -> i64 {
  400
}

fn default_concurrency() -> usize {
  4
}

/// The question every memory is asked under.
pub const QUESTION: &str = "You judge whether one memory an agent kept may be handed, unchanged, to every agent \
of every organisation. Answer yes only where all of it holds for anyone doing such work anywhere: how a public \
tool, language, library, protocol or method behaves, a pitfall of one and its remedy, a habit of careful work. \
Answer no where any of it is bound to something of the writer's own: a deployment, a repository or codebase, a \
product or service of theirs, a machine or host, a person by name or by role, a room, a team, an organisation, a \
worker or agent of theirs, an owner's instruction or preference, a plan, a purchase or an errand; and no where it \
makes sense only with private context. When in doubt, answer no. Begin the answer with the one word yes or no, \
then give one short English sentence why.";

/// What the classifier said of one memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Judgement {
  /// It fits anyone, with the model's sentence.
  Fits(String),
  /// It does not.
  DoesNotFit,
  /// No answer to read: the ask failed or the answer was neither yes nor no.
  Unjudged(String),
}

#[derive(Serialize)]
struct Message<'a> {
  role: &'static str,
  content: &'a str,
}

#[derive(Serialize)]
struct Kwargs {
  enable_thinking: bool,
}

#[derive(Serialize)]
struct Ask<'a> {
  model: &'a str,
  messages: Vec<Message<'a>>,
  max_tokens: i64,
  chat_template_kwargs: Kwargs,
}

/// The words a memory is asked with: its type, its text and its recall condition.
pub fn asked(memory: &Memory) -> String {
  let mut words = format!("Type: {}\nMemory: {}", memory.kind, memory.text.trim());
  if let Some(condition) = &memory.recall_condition {
    words.push_str(&format!("\nNeeded when: {}", condition.trim()));
  }
  words
}

/// An answer read: its first word yes or no, and the rest as the reason.
pub fn read_answer(answer: &str) -> Judgement {
  let answer = answer.trim();
  let first: String = answer
    .split_whitespace()
    .next()
    .unwrap_or("")
    .chars()
    .filter(|c| c.is_alphabetic())
    .collect();
  let rest = answer.split_once(char::is_whitespace).map_or("", |(_, rest)| rest).trim();
  match first.to_lowercase().as_str() {
    "yes" => Judgement::Fits(rest.to_owned()),
    "no" => Judgement::DoesNotFit,
    _ => Judgement::Unjudged(format!("the answer begins with neither yes nor no: {first:?}")),
  }
}

/// One memory judged by the classifier.
pub async fn judge(http: &HttpClient, config: &ClassifierConfig, memory: &Memory) -> Judgement {
  let key = match std::fs::read_to_string(&config.key_file) {
    Ok(key) => key.trim().to_owned(),
    Err(why) => return Judgement::Unjudged(format!("{}: {why}", config.key_file.display())),
  };
  let address = format!("{}/v1/chat/completions", config.address.trim_end_matches('/'));
  let words = asked(memory);
  let ask = Ask {
    model: &config.model,
    messages: vec![
      Message {
        role: "system",
        content: QUESTION,
      },
      Message {
        role: "user",
        content: &words,
      },
    ],
    max_tokens: config.max_tokens,
    chat_template_kwargs: Kwargs {
      enable_thinking: false,
    },
  };
  let answer = match http.post(&address).bearer_auth(key).json(&ask).send().await {
    Ok(answer) => answer,
    Err(why) => return Judgement::Unjudged(format!("{address}: {why}")),
  };
  let status = answer.status();
  if !status.is_success() {
    return Judgement::Unjudged(format!("{address} answered {status}"));
  }
  let body: Value = match answer.json().await {
    Ok(body) => body,
    Err(why) => return Judgement::Unjudged(format!("{address}: the answer does not read: {why}")),
  };
  match body["choices"][0]["message"]["content"].as_str() {
    Some(content) => read_answer(content),
    None => Judgement::Unjudged(format!("{address}: the answer holds no message")),
  }
}

/// What a run of the command was told.
#[derive(Clone, Debug)]
pub struct Run {
  pub from: i64,
  pub pass_over: PassOver,
  pub limit: Option<usize>,
  pub dry_run: bool,
}

/// What a run came to, counted, with each candidate's source and the base row written for it, none in a dry run
/// or where the write failed.
#[derive(Clone, Debug, Default)]
pub struct Came {
  pub sifted: crate::store::base::Sifted,
  pub judged: usize,
  pub fits: usize,
  pub does_not_fit: usize,
  pub unjudged: usize,
  pub written: usize,
  pub candidates: Vec<(i64, Option<i64>)>,
  pub failures: Vec<String>,
}

/// The run: the source sifted, each memory left judged, and each candidate written into the base unless the run
/// is dry; without a classifier nothing is judged.
pub async fn raise(
  store: &Store,
  classifier: Option<&ClassifierConfig>,
  run: &Run,
) -> Result<Came, String> {
  let sifted = store
    .base_sift(run.from, &run.pass_over, run.limit)
    .await
    .map_err(|why| why.to_string())?;
  let mut came = Came::default();
  let Some(config) = classifier else {
    came.unjudged = sifted.left.len();
    came.sifted = sifted;
    return Ok(came);
  };
  let http = HttpClient::builder()
    .no_proxy()
    .build()
    .map_err(|why| format!("no HTTP client: {why}"))?;
  let base = if run.dry_run {
    None
  } else {
    Some(store.base_made().await.map_err(|why| why.to_string())?)
  };
  let judged: Vec<(Memory, Judgement)> = futures_util::stream::iter(sifted.left.iter().cloned())
    .map(|memory| {
      let http = &http;
      async move {
        let judgement = judge(http, config, &memory).await;
        (memory, judgement)
      }
    })
    .buffered(config.concurrency.max(1))
    .collect()
    .await;
  for (memory, judgement) in judged {
    match judgement {
      Judgement::Fits(reason) => {
        came.judged += 1;
        came.fits += 1;
        let written = match base {
          None => None,
          Some(base) => match store.base_raise(base, &memory, &config.model, &reason).await {
            Ok(row) => {
              came.written += 1;
              Some(row.id)
            }
            Err(why) => {
              came.failures.push(format!("memory {}: {why}", memory.id));
              None
            }
          },
        };
        came.candidates.push((memory.id, written));
      }
      Judgement::DoesNotFit => {
        came.judged += 1;
        came.does_not_fit += 1;
      }
      Judgement::Unjudged(why) => {
        came.unjudged += 1;
        came.failures.push(format!("memory {}: {why}", memory.id));
      }
    }
  }
  came.sifted = sifted;
  Ok(came)
}
