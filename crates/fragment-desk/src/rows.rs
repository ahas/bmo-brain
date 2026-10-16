//! What the desk reads of the brain's database: the fragment rows a reach asks for. The table is read
//! whole as the desk opens, from whichever database stands at the place the desk names, and each reach
//! is asked of what was read.

use std::path::Path;

use brain_support::reading::{open_read, retrying};
use sqlx::Row;
use sqlx::any::AnyRow;

use crate::desk::FragmentFilter;

/// One fragment row, in the parts the desk reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
  pub id: i64,
  pub kind: String,
  pub owner: Option<i64>,
  pub slug: String,
  pub title: String,
  pub path: String,
  pub scope: Option<String>,
  pub active: bool,
  pub keywords: Vec<String>,
  pub items: Vec<i64>,
  pub written_on: Option<String>,
  pub confirmed_on: Option<String>,
  pub status: String,
}

/// An agent's brain folder, relative to the brain root, as the agency's registration names it.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentFolder {
  pub agent: String,
  pub folder: String,
}

// A row as a reach is asked of it: the fragment, and where it stands and who may read it.
struct Held {
  fragment: Fragment,
  chat_id: Option<i64>,
  agent: String,
  all_agents: bool,
  position: i64,
}

/// The fragment table as the desk read it, never written.
pub struct Rows {
  held: Vec<Held>,
}

// A list column, which the table keeps as JSON text: what is not a list, or not of the right kind,
// reads as nothing rather than failing the read.
fn list<T: serde::de::DeserializeOwned>(text: &str) -> Vec<T> {
  match serde_json::from_str::<Vec<serde_json::Value>>(text) {
    Ok(values) => values.into_iter().filter_map(|value| serde_json::from_value(value).ok()).collect(),
    Err(_) => Vec::new(),
  }
}

fn held_of(row: &AnyRow) -> Result<Held, sqlx::Error> {
  Ok(Held {
    fragment: Fragment {
      id: row.try_get("id")?,
      kind: row.try_get("kind")?,
      owner: row.try_get("owner")?,
      slug: row.try_get("slug")?,
      title: row.try_get("title")?,
      path: row.try_get("path")?,
      scope: row.try_get("scope")?,
      active: row.try_get::<i64, _>("active")? != 0,
      keywords: list(&row.try_get::<String, _>("keywords")?),
      items: list::<f64>(&row.try_get::<String, _>("items")?)
        .into_iter()
        .filter(|item| item.fract() == 0.0)
        .map(|item| item as i64)
        .collect(),
      written_on: row.try_get("written_on")?,
      confirmed_on: row.try_get("confirmed_on")?,
      status: row.try_get("status")?,
    },
    chat_id: row.try_get("chat_id")?,
    agent: row.try_get("agent")?,
    all_agents: row.try_get::<Option<i64>, _>("all_agents")?.unwrap_or(0) == 1,
    position: row.try_get::<Option<i64>, _>("position")?.unwrap_or(0),
  })
}

impl Rows {
  /// The fragment rows, read from the database at this place, and read again whole where the database
  /// went away under the reading.
  pub async fn open(place: &Path) -> Result<Self, sqlx::Error> {
    retrying(|| Self::read(place)).await
  }

  async fn read(place: &Path) -> Result<Self, sqlx::Error> {
    let mut db = open_read(place).await?;
    let mut held = sqlx::query("SELECT * FROM fragments")
      .fetch_all(&mut db)
      .await?
      .iter()
      .map(held_of)
      .collect::<Result<Vec<_>, _>>()?;

    // The order the entries stand in: by kind, the room (none first), place and number.
    held.sort_by(|a, b| {
      (&a.fragment.kind, a.chat_id.unwrap_or(0), a.position, a.fragment.id).cmp(&(
        &b.fragment.kind,
        b.chat_id.unwrap_or(0),
        b.position,
        b.fragment.id,
      ))
    });

    Ok(Self { held })
  }

  /// The rows a filter asks for, in the order their entries stand.
  pub fn fragments(&self, filter: &FragmentFilter) -> Result<Vec<Fragment>, sqlx::Error> {
    Ok(
      self
        .held
        .iter()
        .filter(|row| filter.kind.as_ref().is_none_or(|kind| &row.fragment.kind == kind))
        .filter(|row| filter.chat_id.is_none_or(|room| row.chat_id == room))
        .filter(|row| {
          filter
            .owners
            .as_ref()
            .is_none_or(|owners| row.fragment.owner.is_some_and(|owner| owners.contains(&owner)))
        })
        .filter(|row| filter.agent.as_ref().is_none_or(|agent| &row.agent == agent))
        .filter(|row| {
          filter.readable_by.as_ref().is_none_or(|reader| &row.agent == reader || row.all_agents)
        })
        .filter(|row| filter.active.is_none_or(|active| row.fragment.active == active))
        .map(|row| row.fragment.clone())
        .collect(),
    )
  }
}
