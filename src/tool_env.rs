//! Where the brain's tool finds the service (docs/boundary-api.md, 「Its tool」): the data root's env file, the
//! service's base address and token written into it from the configuration, readable by the user alone and
//! rewritten only where it would change.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// The env file under the data root the tool reads the brain's address and token from.
pub const ENV_FILE: &str = "env/brain.env";

/// The variable naming the brain's base address.
pub const ADDRESS_VARIABLE: &str = "BRAIN_ADDRESS";

/// The variable naming the bearer token the brain takes.
pub const TOKEN_VARIABLE: &str = "BRAIN_TOKEN";

/// What writing the env file came to, and where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Written {
  Written(PathBuf),
  Unchanged(PathBuf),
}

fn bare(value: &str) -> bool {
  !value.is_empty()
    && !value.chars().any(|character| {
      character.is_whitespace() || matches!(character, '"' | '\'' | '`' | '#' | '$' | '\\')
    })
}

/// The env file's text for this configuration, or why a value cannot stand in it.
pub fn env_text(config: &Config) -> Result<String, String> {
  let address = config.base_address();
  for (name, value) in [(ADDRESS_VARIABLE, address.as_str()), (TOKEN_VARIABLE, config.token.as_str())] {
    if !bare(value) {
      return Err(format!(
        "{name} cannot stand bare in {ENV_FILE}: it is empty or holds whitespace, a quote, '#', '$' or '\\'"
      ));
    }
  }
  Ok(format!("{ADDRESS_VARIABLE}={address}\n{TOKEN_VARIABLE}={}\n", config.token))
}

/// The env file written under the data root, where it would change; nothing is written where a value cannot
/// stand in it.
pub fn write_env(config: &Config, data_root: &Path) -> Result<Written, String> {
  let path = data_root.join(ENV_FILE);
  let text = env_text(config)?;
  if std::fs::read(&path).is_ok_and(|standing| standing == text.as_bytes()) {
    return Ok(Written::Unchanged(path));
  }
  if let Some(folder) = path.parent() {
    std::fs::create_dir_all(folder).map_err(|why| format!("{}: {why}", folder.display()))?;
  }
  std::fs::write(&path, text.as_bytes()).map_err(|why| format!("{}: {why}", path.display()))?;
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
    .map_err(|why| format!("{}: {why}", path.display()))?;
  Ok(Written::Written(path))
}
