//! The brain a tool answers through (docs/boundary-api.md, 「Its tools」): where it stands and the token it takes,
//! read from the tool's own environment or the data root's env file, and one route asked with what it answers read
//! as the wire's conventions say.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::session::Session;

/// The variable naming the brain's base address.
pub const ADDRESS_VARIABLE: &str = "BRAIN_ADDRESS";

/// The variable naming the bearer token the brain takes.
pub const TOKEN_VARIABLE: &str = "BRAIN_TOKEN";

/// The env file under the data root a tool reads the two from where its own environment holds neither.
pub const ENV_FILE: &str = "env/brain.env";

// How long one route is waited on, inside the host's timeout for the whole call.
const PATIENCE: Duration = Duration::from_secs(25);

/// What a route answered: a reading's lines, a refusal with the lines written before it, or any other object.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
  Reading(Vec<String>),
  Refused {
    code: String,
    message: String,
    lines: Vec<String>,
  },
  Object(Value),
}

/// The brain, as one tool call reaches it.
#[derive(Clone, Debug)]
pub struct Brain {
  pub base: String,
  pub token: String,
  client: reqwest::Client,
}

// One variable: the tool's own environment first, then the env file's.
fn variable(name: &str, file: &[(String, String)]) -> Option<String> {
  std::env::var(name)
    .ok()
    .or_else(|| file.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()))
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
}

fn lines_of(value: &Value) -> Vec<String> {
  value
    .get("lines")
    .and_then(Value::as_array)
    .map(|lines| lines.iter().filter_map(Value::as_str).map(str::to_owned).collect())
    .unwrap_or_default()
}

impl Brain {
  /// The brain at an address with a token.
  pub fn at(base: &str, token: &str) -> Self {
    Self {
      base: base.trim_end_matches('/').to_owned(),
      token: token.to_owned(),
      client: reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("a client with no TLS and no proxy always builds"),
    }
  }

  /// The brain the record's call reaches, or why it names none.
  pub fn of(session: &Session) -> Result<Self, String> {
    Self::under(&session.data_root)
  }

  /// The brain as a tool under this data root reaches it.
  pub fn under(data_root: &Path) -> Result<Self, String> {
    let path = data_root.join(ENV_FILE);
    let file: Vec<(String, String)> = match dotenvy::from_path_iter(&path) {
      Ok(read) => read.filter_map(Result::ok).collect(),
      Err(_) => vec![],
    };
    let named = |name: &str| {
      variable(name, &file)
        .ok_or_else(|| format!("no {name} in this tool's environment or in {}", path.display()))
    };

    Ok(Self::at(&named(ADDRESS_VARIABLE)?, &named(TOKEN_VARIABLE)?))
  }

  /// One route asked with a body, and its answer read: a reading, a refusal or another object; why it could not be
  /// read where the brain could not be reached or answered with an error.
  pub async fn post(&self, path: &str, body: &impl Serialize) -> Result<Said, String> {
    let url = format!("{}{path}", self.base);
    let answer = self
      .client
      .post(&url)
      .bearer_auth(&self.token)
      .header(reqwest::header::CONTENT_TYPE, "application/json")
      .body(serde_json::to_vec(body).map_err(|why| why.to_string())?)
      .timeout(PATIENCE)
      .send()
      .await
      .map_err(|why| format!("the brain at {} could not be reached: {why}", self.base))?;
    let status = answer.status();
    let bytes = answer
      .bytes()
      .await
      .map_err(|why| format!("the brain's answer could not be read: {why}"))?;
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    if !status.is_success() {
      let error = value.get("error").and_then(Value::as_str).unwrap_or_default();

      return Err(format!("the brain answered {} to {path}: {error}", status.as_u16()));
    }

    Ok(match value.get("answered").and_then(Value::as_bool) {
      Some(false) => Said::Refused {
        code: value["refusal"]["code"].as_str().unwrap_or_default().to_owned(),
        message: value["refusal"]["message"].as_str().unwrap_or_default().to_owned(),
        lines: lines_of(&value),
      },
      Some(true) => Said::Reading(lines_of(&value)),
      None => Said::Object(value),
    })
  }
}
