//! The brain's kinds of the delegation chain (docs/design.md, 「The layers a reading takes」;
//! docs/boundary-api.md, 「Declaring the chain's kinds」): whether the base and the org brains are read for the
//! people under a link, declared on the account service by the deployment's command. The service itself never
//! asks the chain; its callers resolve the kinds as they act.

use std::path::{Path, PathBuf};

use bmo_brain_client::HttpClient;
use serde_json::{Value, json};

/// The unit name the kinds are declared under.
pub const DECLARED_BY: &str = "brain";

/// The kind saying whether the base is read.
pub const GLOBAL: &str = "brain.global";

/// The kind saying whether the org brains are read.
pub const ORGANISATION: &str = "brain.organisation";

/// The env file under the data root naming the account service, and its variables.
pub const WEB_ENV_FILE: &str = "env/web.env";
pub const WEB_ADDRESS: &str = "WEB_ADDRESS";
pub const WEB_TOKEN: &str = "WEB_TOKEN";
pub const WEB_TOKEN_FILE: &str = "WEB_TOKEN_FILE";

/// Each kind's key and what it says.
pub const KINDS: [(&str, &str); 2] = [
  (
    GLOBAL,
    "Whether the platform's base of shared memories is read for the people under this link.",
  ),
  (
    ORGANISATION,
    "Whether the organisation's shared memories are read for the people under this link.",
  ),
];

/// The body a kind is declared with: a flag, narrowing, read by default, with rules, at the platform and at units,
/// written by a unit's heads.
pub fn declaration(key: &str, description: &str) -> Value {
  json!({
    "key": key,
    "declaredBy": DECLARED_BY,
    "description": description,
    "shape": "flag",
    "mode": "narrowing",
    "default": true,
    "rules": true,
    "levels": ["platform", "unit"],
    "unitPermission": "manage",
    "personWriter": "heads",
  })
}

/// The `KEY=value` lines of an env file, each value verbatim after its first `=`; none where it does not read.
fn env_lines(path: &Path) -> Vec<(String, String)> {
  std::fs::read_to_string(path)
    .map(|text| {
      text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.to_owned()))
        .collect()
    })
    .unwrap_or_default()
}

/// Where the account service stands: its address and its token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountService {
  pub address: String,
  pub token: String,
}

/// The account service the environment names, else the data root's env file; none where neither names an
/// address.
pub fn account_service(data_root: Option<&Path>) -> Result<Option<AccountService>, String> {
  let file = data_root.map(|root| env_lines(&root.join(WEB_ENV_FILE))).unwrap_or_default();
  let named = |name: &str| {
    std::env::var(name)
      .ok()
      .or_else(|| file.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()))
      .filter(|value| !value.trim().is_empty())
  };
  let Some(address) = named(WEB_ADDRESS) else {
    return Ok(None);
  };
  let token = match (named(WEB_TOKEN), named(WEB_TOKEN_FILE)) {
    (Some(token), _) => token,
    (None, Some(path)) => std::fs::read_to_string(PathBuf::from(path.trim()))
      .map(|token| token.trim().to_owned())
      .map_err(|why| format!("{WEB_TOKEN_FILE} does not read: {why}"))?,
    (None, None) => String::new(),
  };
  Ok(Some(AccountService {
    address: address.trim().trim_end_matches('/').to_owned(),
    token,
  }))
}

/// Every kind declared, `PUT /chain/kinds/{key}`, the keys declared in order; the first refusal is the error.
pub async fn declare(service: &AccountService) -> Result<Vec<String>, String> {
  let http = HttpClient::builder()
    .no_proxy()
    .build()
    .map_err(|why| format!("no HTTP client: {why}"))?;
  let mut declared = vec![];
  for (key, description) in KINDS {
    let answer = http
      .put(format!("{}/chain/kinds/{key}", service.address))
      .bearer_auth(&service.token)
      .json(&declaration(key, description))
      .send()
      .await
      .map_err(|why| format!("the account service was not reached: {why}"))?;
    let status = answer.status();
    if !status.is_success() {
      let said = answer.text().await.unwrap_or_default();
      return Err(format!("the account service refused the kind {key} with {}: {said}", status.as_u16()));
    }
    declared.push(key.to_owned());
  }
  Ok(declared)
}
