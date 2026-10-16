//! What a memory row's text is read against that the brain does not hold: the rooms and the people the
//! archive knows, the names the awake agents go by, and the reading of promises. The work that builds the
//! rows' answers supplies it.

use brain_support::database::Fault;
use futures_util::future::BoxFuture;

/// A room the archive knows, by its chat id and the title it goes under where it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownRoom {
  pub chat_id: i64,
  pub title: Option<String>,
}

/// The rooms the archive knows, and every person who has stood as a sender in one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownRooms {
  pub rooms: Vec<KnownRoom>,
  pub senders: Vec<i64>,
}

/// Where the facts of other people, rooms and agents are read from.
pub trait Known: Sync {
  /// The rooms and the senders the archive knows.
  fn rooms(&self) -> BoxFuture<'_, Result<KnownRooms, Fault>>;

  /// The name each person goes by, in the order they were asked for: the one their latest message went
  /// under, or none where nothing names them.
  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<Option<String>>, Fault>>;

  /// The label each awake agent goes by, or its name where it has none.
  fn agent_names(&self) -> BoxFuture<'_, Result<Vec<String>, Fault>>;

  /// Whether a text waits for something to happen first, by the reading of promises.
  fn waits_on_happening<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<bool, Fault>>;
}
