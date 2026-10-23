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
  /// The console the brain serves its bundle to; none where it serves none.
  #[serde(default)]
  pub console: Option<ConsoleConfig>,
}

/// The console bundle's settings (docs/boundary-api.md, 「The console bundle」).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleConfig {
  /// The console's bearer token, which reaches what the screen uses.
  pub token: String,
  /// The folder the build of `client/` wrote the bundle into; the release's `console` where none is given.
  #[serde(default)]
  pub bundle: Option<PathBuf>,
  /// The other services the screen calls through the console, each by the service name its own manifest gives
  /// it; none where none is given.
  #[serde(default)]
  pub calls: Vec<String>,
}

/// How long the health command waits on one answer.
pub const HEALTH_PATIENCE: Duration = Duration::from_secs(5);

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
