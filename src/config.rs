//! The service's configuration (docs/boundary-api.md, 「Configuration」): the TOML file the binary's `--config`
//! names, read once at start and held for the process's life, and what the routes stand on built from it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use serde::Deserialize;

use crate::api::Api;
use crate::api::memories::Withering;
use crate::ports::agency::HttpAgency;
use crate::ports::gate::HttpGate;
use crate::ports::http::Peer;
use crate::ports::job::HttpJob;
use crate::ports::receiver::HttpReceiver;
use crate::schedules::apart::UserUnits;

/// The configuration, key by key as the file writes it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
  pub listen: String,
  pub token: String,
  pub database: String,
  pub data_root: PathBuf,
  pub brain: PathBuf,
  pub prompts: PathBuf,
  pub chat_key: String,
  pub owner: Option<i64>,
  #[serde(default)]
  pub owner_names: Vec<String>,
  pub service_unit: String,
  pub agency: Peer,
  pub receiver: Peer,
  pub gate: Peer,
  pub job: Peer,
  pub own_command: Vec<String>,
}

impl Config {
  /// The configuration a file holds, or why it could not be read.
  pub fn read(path: &Path) -> Result<Self, String> {
    let text = std::fs::read_to_string(path).map_err(|why| format!("{}: {why}", path.display()))?;

    toml::from_str(&text).map_err(|why| format!("{}: {why}", path.display()))
  }

  /// The brain's database, connected.
  pub async fn connect(&self) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let mut options = ConnectOptions::new(self.database.clone());

    options.sqlx_logging(false);

    Database::connect(options).await
  }

  /// What the routes stand on, over a connected database and the units this configuration names.
  pub fn api(&self, db: DatabaseConnection) -> Api {
    Api {
      db,
      database: self.database.clone(),
      agency: Arc::new(HttpAgency::new(&self.agency)),
      receiver: Arc::new(HttpReceiver::new(&self.receiver)),
      gate: Arc::new(HttpGate::new(&self.gate)),
      job: Arc::new(HttpJob::new(&self.job)),
      starter: Arc::new(UserUnits::default()),
      brain_root: self.brain.clone(),
      data_root: self.data_root.clone(),
      prompts_root: self.prompts.clone(),
      chat_key: self.chat_key.clone(),
      owner: self.owner,
      owner_names: self.owner_names.clone(),
      service_unit: self.service_unit.clone(),
      program: self.own_command.clone(),
      token: self.token.clone(),
      withering: Withering::default(),
    }
  }
}
