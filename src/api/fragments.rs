//! The fragment rows a turn's gate reads and what it hands back (docs/fragments.md, 「What a room may be offered」;
//! docs/boundary-api.md, 「Fragments」): the active rows of the kinds asked that the turn's agent may be offered in a
//! room — its own and those marked for every agent — in the order their indexes stand, and the rows a turn
//! attached counted. Which rows a room may be offered is each brain library's own answer; a shipped kind offers
//! every row.

use std::collections::BTreeMap;

use fragment_desk::desk::FragmentFilter;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::Failure;
use crate::editors::archive::keeps_room_book;
use crate::editors::fragments::libraries::{LIBRARIES, Offering};
use crate::editors::fragments::store::{self, Row, Standing};
use crate::ports::receiver::Receiver;

/// One fragment row as a gate is offered it, whole.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
  pub kind: String,
  pub chat_id: Option<i64>,
  pub owner: Option<i64>,
  pub agent: String,
  pub all_agents: bool,
  pub slug: String,
  pub title: String,
  pub path: String,
  pub scope: Option<String>,
  pub position: i64,
  pub active: bool,
  pub last_hit_at: Option<i64>,
  pub hits: i64,
  pub set_by: Option<i64>,
  pub set_at: i64,
  pub hand: Option<bool>,
  pub hand_at: Option<i64>,
  pub keywords: Vec<String>,
  pub items: Vec<i64>,
  pub written_on: Option<String>,
  pub confirmed_on: Option<String>,
  pub status: String,
  pub body_hash: Option<String>,
}

impl Fragment {
  /// A row with what it has come to, as a screen or a gate reads it.
  pub fn of((row, standing): (Row, Standing)) -> Self {
    Self {
      kind: row.kind,
      chat_id: row.chat_id,
      owner: row.owner,
      agent: row.agent,
      all_agents: row.all_agents,
      slug: row.slug,
      title: row.title,
      path: row.path,
      scope: row.scope,
      position: row.position,
      active: row.active,
      last_hit_at: standing.last_hit_at,
      hits: standing.hits,
      set_by: standing.set_by,
      set_at: standing.set_at,
      hand: row.hand,
      hand_at: standing.hand_at,
      keywords: row.keywords,
      items: row.items,
      written_on: row.written_on,
      confirmed_on: row.confirmed_on,
      status: row.status,
      body_hash: row.body_hash,
    }
  }
}

/// Which rooms' rows a read takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rooms {
  /// The rows naming this room.
  Of(i64),
  /// The rows naming no room, which every room shares.
  None,
  /// Every row, whatever room it names.
  Every,
}

/// The active rows of one kind one agent's turn may read — its own and those marked for every agent — in the
/// order their entries stand.
pub async fn rows(
  db: &DatabaseConnection,
  kind: &str,
  rooms: Rooms,
  agent: &str,
) -> Result<Vec<(Row, Standing)>, Failure> {
  let filter = FragmentFilter {
    kind: Some(kind.to_owned()),
    chat_id: match rooms {
      Rooms::Of(chat_id) => Some(Some(chat_id)),
      Rooms::None => Some(None),
      Rooms::Every => None,
    },
    readable_by: Some(agent.to_owned()),
    active: Some(true),
    ..FragmentFilter::default()
  };

  Ok(store::rows_standing(db, &filter).await?)
}

// The rows of one brain library a room may be offered, as the library says.
async fn offered(
  db: &DatabaseConnection,
  receiver: &dyn Receiver,
  kind: &str,
  offering: Offering,
  agent: &str,
  chat_id: i64,
) -> Result<Vec<(Row, Standing)>, Failure> {
  match offering {
    Offering::Room => rows(db, kind, Rooms::Of(chat_id), agent).await,
    Offering::RoomWithShared => {
      let mut found = rows(db, kind, Rooms::Of(chat_id), agent).await?;

      found.extend(rows(db, kind, Rooms::None, agent).await?);
      found.sort_by_key(|(row, _)| (row.position, row.id));

      Ok(found)
    }
    Offering::RoomPartWithSpeakers => {
      // A one-to-one keeps no room part: what an older book left under it stands, and nothing offers it.
      let mut found = if keeps_room_book(receiver, chat_id).await? {
        rows(db, kind, Rooms::Of(chat_id), agent).await?
      } else {
        vec![]
      };
      let mut by_person: BTreeMap<i64, Vec<(Row, Standing)>> = BTreeMap::new();

      for (row, standing) in rows(db, kind, Rooms::None, agent).await? {
        if let Some(owner) = row.owner {
          by_person.entry(owner).or_default().push((row, standing));
        }
      }

      let people: Vec<i64> = by_person.keys().copied().collect();
      let spoke = if people.is_empty() { vec![] } else { receiver.spoken_in(chat_id, &people).await? };

      for (person, rows) in by_person {
        if spoke.contains(&person) {
          found.extend(rows);
        }
      }

      Ok(found)
    }
  }
}

/// The active rows of each kind asked, in the order asked, that the agent's turn may be offered in the room.
pub async fn offer(
  db: &DatabaseConnection,
  receiver: &dyn Receiver,
  agent: &str,
  chat_id: i64,
  kinds: &[String],
) -> Result<Vec<Fragment>, Failure> {
  let mut fragments = vec![];

  for kind in kinds {
    let found = match LIBRARIES.iter().find(|library| library.kind() == kind) {
      Some(library) => offered(db, receiver, kind, library.offering(), agent, chat_id).await?,
      None => rows(db, kind, Rooms::Every, agent).await?,
    };

    fragments.extend(found.into_iter().map(Fragment::of));
  }

  Ok(fragments)
}

/// One row a turn attached, as the gate names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
  pub kind: String,
  pub chat_id: Option<i64>,
  pub owner: Option<i64>,
  pub slug: String,
}

/// What a turn attached, counted on the rows the agent may read.
pub async fn hits(db: &DatabaseConnection, agent: &str, attached: &[Hit], now: i64) -> Result<(), Failure> {
  for hit in attached {
    store::count_hit(db, agent, (&hit.kind, hit.chat_id, hit.owner, &hit.slug), now).await?;
  }

  Ok(())
}
