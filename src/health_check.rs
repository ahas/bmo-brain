//! Whether the service a configuration names is ready (docs/boundary-api.md, 「Health」): its health route asked at
//! its listen address with its token, as a deployment asks it before it takes a release as up.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;
use crate::tool_env::base_address;

/// How long the service is waited on for one answer.
pub const PATIENCE: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
struct Health {
  ready: bool,
  reason: Option<String>,
}

/// Nothing where the service answers ready; otherwise why it is not: the reason it gives, the status it answered
/// with, or that nothing answered.
pub async fn ask_health(config: &Config) -> Result<(), String> {
  let url = format!("{}/health", base_address(&config.listen));
  let client = Client::builder()
    .no_proxy()
    .timeout(PATIENCE)
    .build()
    .map_err(|why| format!("no client for {url}: {why}"))?;
  let answer = client
    .get(&url)
    .bearer_auth(&config.token)
    .send()
    .await
    .map_err(|why| format!("{url} did not answer: {why}"))?;
  let status = answer.status();

  if !status.is_success() {
    return Err(format!("{url} answered {status}"));
  }

  let bytes = answer.bytes().await.map_err(|why| format!("{url} answered unreadably: {why}"))?;
  let health: Health =
    serde_json::from_slice(&bytes).map_err(|why| format!("{url} answered no health: {why}"))?;

  if health.ready {
    Ok(())
  } else {
    Err(format!(
      "the service is not ready: {}",
      health.reason.as_deref().unwrap_or("it gave no reason")
    ))
  }
}
