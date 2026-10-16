//! The desk an editor run writes its rows on: read afresh for every call, and written back whole once a
//! call that writes has been allowed.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A call the tool refuses: nothing was written, and the message says why and what to do instead.
#[derive(Debug)]
pub struct Refusal(pub String);

impl fmt::Display for Refusal {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

/// What can stop a call: a refusal of what it asked for, or a failure to read what it stands on.
#[derive(Debug)]
pub enum DeskError {
  Refused(Refusal),
  Failed(String),
}

impl DeskError {
  pub fn refused(why: impl Into<String>) -> Self {
    Self::Refused(Refusal(why.into()))
  }
}

impl<E: std::error::Error> From<E> for DeskError {
  fn from(error: E) -> Self {
    Self::Failed(error.to_string())
  }
}

pub type Desked<T> = Result<T, DeskError>;

/// One row write an editor made, or the taking away of a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentWrite {
  pub path: String,
  pub removed: bool,
  pub title: String,
  pub keywords: Vec<String>,
  pub items: Vec<i64>,
  pub shared: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub all_agents: Option<bool>,
}

/// One read of the table a desk's reach asks for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentFilter {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  /// A room, `null` for the rows naming no room, or absent for every room.
  #[serde(default, skip_serializing_if = "Option::is_none", with = "absent_or_null")]
  pub chat_id: Option<Option<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owners: Option<Vec<i64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub readable_by: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub active: Option<bool>,
}

// A field whose absence and whose `null` say different things.
mod absent_or_null {
  use serde::{Deserialize, Deserializer, Serialize, Serializer};

  pub fn serialize<S: Serializer>(value: &Option<Option<i64>>, serializer: S) -> Result<S::Ok, S::Error> {
    value.flatten().serialize(serializer)
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(deserializer).map(Some)
  }
}

/// Where a run that writes beside a book writes, and where its pieces will stand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Beside {
  pub from: String,
  pub to: String,
}

/// The deployment's names the checks read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Names {
  pub agent: String,
  pub agent_key: String,
  pub common_words: Vec<String>,
}

/// The desk as its file holds it. What this reader does not know is kept as it stood.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Desk {
  pub room: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default)]
  pub folder: String,
  pub places: Vec<String>,
  pub reach: Vec<FragmentFilter>,
  pub beside: Option<Beside>,
  pub carried: Vec<FragmentWrite>,
  pub db: String,
  pub brain: String,
  pub before: BTreeMap<String, String>,
  pub writes: Vec<FragmentWrite>,
  pub names: Option<Names>,
  #[serde(flatten)]
  pub rest: Map<String, Value>,
}

impl Desk {
  pub fn read(path: &Path) -> Desked<Self> {
    let failed = |why: &dyn fmt::Display| DeskError::Failed(format!("{}: {why}", path.display()));

    serde_json::from_str(&fs::read_to_string(path).map_err(|why| failed(&why))?).map_err(|why| failed(&why))
  }

  /// The desk written whole, so a reader never finds half of it.
  pub fn save(&self, path: &Path) -> Desked<()> {
    let staged = path.with_extension(format!("json.{}.tmp", std::process::id()));

    if let Some(dir) = path.parent() {
      fs::create_dir_all(dir)?;
    }

    fs::write(&staged, format!("{}\n", serde_json::to_string_pretty(self)?))?;
    fs::rename(&staged, path)?;

    Ok(())
  }

  /// The names the checks read, which the batch that opened the desk wrote on it.
  pub fn names(&self) -> Desked<&Names> {
    self.names.as_ref().ok_or_else(|| {
      DeskError::refused("This desk names none of the deployment's names; it was opened before the tool host could read it. Let the batch open a new desk.")
    })
  }
}
