//! The service's configuration (docs/boundary-api.md, 「Configuration」): the TOML file the binary's `--config`
//! names, read once at start and held for the process's life.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bmo_brain_client::Rules;
use serde::Deserialize;

/// The configuration, key by key as the file writes it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
  pub listen: String,
  pub token: String,
  pub database: String,
  /// The numbers the brain reads by (docs/design.md); each has a default.
  #[serde(default)]
  pub rules: Rules,
  /// The data root whose env file the brain's tool reads the service's address from; none where the brain
  /// ships no tool env.
  #[serde(default)]
  pub data_root: Option<PathBuf>,
}

/// How long the health command waits on one answer.
pub const HEALTH_PATIENCE: Duration = Duration::from_secs(5);

/// The configuration a deployment links beside the binary's directory: `brain.toml` in the directory above
/// the one holding the running binary.
pub fn beside_binary() -> Option<PathBuf> {
  let binary = std::env::current_exe().ok()?;
  Some(binary.parent()?.parent()?.join("brain.toml"))
}

impl Config {
  /// The configuration a file holds, or why it could not be read.
  pub fn read(path: &Path) -> Result<Self, String> {
    let text = std::fs::read_to_string(path).map_err(|why| format!("{}: {why}", path.display()))?;
    toml::from_str(&text).map_err(|why| format!("{}: {why}", path.display()))
  }

  /// The address the service answers at, as a caller on this machine reaches it: an unspecified host is asked
  /// at the loopback one.
  pub fn base_address(&self) -> String {
    let listen = self.listen.trim();
    let (host, port) = listen.rsplit_once(':').unwrap_or((listen, "80"));
    let host = match host {
      "" | "0.0.0.0" => "127.0.0.1",
      "[::]" | "::" => "[::1]",
      host => host,
    };
    format!("http://{host}:{port}")
  }
}
