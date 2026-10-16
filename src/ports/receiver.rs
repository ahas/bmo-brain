//! The receiver port: the messages of a room and an agent's read marks on them, the rooms and the people in
//! them, the reading of a promise, the roads, the owner's word and rest, and a notice to the owner
//! (`docs/boundary-api.md`, 「The receiver」).

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ports::http::{Caller, Done, Nothing, Peer, PortFailure};

/// One message of a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
  pub chat_id: i64,
  pub message_id: i64,
  pub sender_id: Option<i64>,
  pub sender_name: String,
  pub sent_at: i64,
  pub text: String,
  pub reply_to_message_id: Option<i64>,
}

/// A room's title and the people that have stood as a sender in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomTitle {
  pub chat_id: i64,
  pub title: Option<String>,
  pub senders: Vec<i64>,
}

/// The agents answering a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomAnswerers {
  pub chat_id: i64,
  pub agents: Vec<String>,
}

/// The name a person goes by, where anything names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonName {
  pub person: i64,
  pub name: Option<String>,
}

/// Whose road a room is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoadKind {
  Owner,
  Group,
}

/// A room's roads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Roads {
  pub kind: RoadKind,
  pub one_to_one: bool,
}

/// Whether a person is the owner, and whether the owner spoke in a room within a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerSpoke {
  pub owner: bool,
  pub spoke: bool,
}

/// Whether the agents rest, and until when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rest {
  pub resting: bool,
  pub until: Option<i64>,
}

/// A notice to the owner: its code and the code's own parameters, which the web unit's language packs
/// render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnerNotice {
  pub code: String,
  #[serde(flatten)]
  pub params: Map<String, Value>,
}

/// What the brain asks of the receiver.
pub trait Receiver: Send + Sync {
  /// The messages of a room past the agent's read marks, oldest first, from `since` where it is given.
  fn unread_messages<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    limit: i64,
    since: Option<i64>,
  ) -> BoxFuture<'a, Result<Vec<Message>, PortFailure>>;

  /// The messages of a room the agent has marked, after `from`, oldest first.
  fn read_messages<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    from: i64,
    limit: i64,
  ) -> BoxFuture<'a, Result<Vec<Message>, PortFailure>>;

  /// Marks the messages read for the agent.
  fn keep_read_marks<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    message_ids: &'a [i64],
  ) -> BoxFuture<'a, Result<bool, PortFailure>>;

  /// Lets go of the agent's read marks in a room, or in every room where none is named, and answers how
  /// many went.
  fn forget_read_marks<'a>(
    &'a self,
    agent: &'a str,
    chat_id: Option<i64>,
  ) -> BoxFuture<'a, Result<i64, PortFailure>>;

  /// Every room's title and senders.
  fn room_titles(&self) -> BoxFuture<'_, Result<Vec<RoomTitle>, PortFailure>>;

  /// The agents answering a room, or each room where none is named.
  fn room_answerers(&self, chat_id: Option<i64>) -> BoxFuture<'_, Result<Vec<RoomAnswerers>, PortFailure>>;

  /// Those of the people who have stood as a sender in the room.
  fn spoken_in<'a>(
    &'a self,
    chat_id: i64,
    people: &'a [i64],
  ) -> BoxFuture<'a, Result<Vec<i64>, PortFailure>>;

  /// The name each person goes by.
  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<PersonName>, PortFailure>>;

  /// Whether a text waits for something to happen first, by the receiver's reading of promises.
  fn waits_on_happening<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>>;

  /// A room's roads.
  fn roads(&self, chat_id: i64) -> BoxFuture<'_, Result<Roads, PortFailure>>;

  /// Whether a message's sender is the owner.
  fn sender_is_owner(&self, chat_id: i64, message_id: i64) -> BoxFuture<'_, Result<bool, PortFailure>>;

  /// Whether the person is the owner, and whether the owner sent a message in the room in the `seconds`
  /// before `before`.
  fn owner_spoke(
    &self,
    chat_id: i64,
    person: i64,
    before: i64,
    seconds: i64,
  ) -> BoxFuture<'_, Result<OwnerSpoke, PortFailure>>;

  /// Whether the agents rest.
  fn rest_state(&self) -> BoxFuture<'_, Result<Rest, PortFailure>>;

  /// Tells the owner a notice.
  fn notify_owner<'a>(&'a self, notice: &'a OwnerNotice) -> BoxFuture<'a, Result<bool, PortFailure>>;
}

/// The receiver over HTTP.
#[derive(Debug, Clone)]
pub struct HttpReceiver {
  caller: Caller,
}

impl HttpReceiver {
  pub fn new(peer: &Peer) -> Self {
    Self::with_caller(Caller::new(peer))
  }

  pub fn with_caller(caller: Caller) -> Self {
    Self { caller }
  }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnreadAsk<'a> {
  agent: &'a str,
  chat_id: i64,
  limit: i64,
  since: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadAsk<'a> {
  agent: &'a str,
  chat_id: i64,
  from: i64,
  limit: i64,
}

#[derive(Deserialize)]
struct Messages {
  messages: Vec<Message>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeepAsk<'a> {
  agent: &'a str,
  chat_id: i64,
  message_ids: &'a [i64],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgetAsk<'a> {
  agent: &'a str,
  chat_id: Option<i64>,
}

#[derive(Deserialize)]
struct Forgotten {
  forgotten: i64,
}

#[derive(Deserialize)]
struct Titles {
  rooms: Vec<RoomTitle>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoomAsk {
  chat_id: Option<i64>,
}

#[derive(Deserialize)]
struct Answerers {
  rooms: Vec<RoomAnswerers>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpokenAsk<'a> {
  chat_id: i64,
  people: &'a [i64],
}

#[derive(Serialize, Deserialize)]
struct People<P> {
  people: P,
}

#[derive(Deserialize)]
struct Names {
  names: Vec<PersonName>,
}

#[derive(Serialize)]
struct TextAsk<'a> {
  text: &'a str,
}

#[derive(Deserialize)]
struct Waits {
  waits: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatAsk {
  chat_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageAsk {
  chat_id: i64,
  message_id: i64,
}

#[derive(Deserialize)]
struct Owner {
  owner: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnerSpokeAsk {
  chat_id: i64,
  person: i64,
  before: i64,
  seconds: i64,
}

impl Receiver for HttpReceiver {
  fn unread_messages<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    limit: i64,
    since: Option<i64>,
  ) -> BoxFuture<'a, Result<Vec<Message>, PortFailure>> {
    Box::pin(async move {
      let ask = UnreadAsk {
        agent,
        chat_id,
        limit,
        since,
      };
      let answer: Messages = self.caller.post("/messages/unread", &ask).await?;
      Ok(answer.messages)
    })
  }

  fn read_messages<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    from: i64,
    limit: i64,
  ) -> BoxFuture<'a, Result<Vec<Message>, PortFailure>> {
    Box::pin(async move {
      let ask = ReadAsk {
        agent,
        chat_id,
        from,
        limit,
      };
      let answer: Messages = self.caller.post("/messages/read", &ask).await?;
      Ok(answer.messages)
    })
  }

  fn keep_read_marks<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    message_ids: &'a [i64],
  ) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let ask = KeepAsk {
        agent,
        chat_id,
        message_ids,
      };
      let answer: Done = self.caller.post("/read-marks/keep", &ask).await?;
      Ok(answer.ok)
    })
  }

  fn forget_read_marks<'a>(
    &'a self,
    agent: &'a str,
    chat_id: Option<i64>,
  ) -> BoxFuture<'a, Result<i64, PortFailure>> {
    Box::pin(async move {
      let answer: Forgotten = self.caller.post("/read-marks/forget", &ForgetAsk { agent, chat_id }).await?;
      Ok(answer.forgotten)
    })
  }

  fn room_titles(&self) -> BoxFuture<'_, Result<Vec<RoomTitle>, PortFailure>> {
    Box::pin(async move {
      let answer: Titles = self.caller.post("/rooms/titles", &Nothing {}).await?;
      Ok(answer.rooms)
    })
  }

  fn room_answerers(&self, chat_id: Option<i64>) -> BoxFuture<'_, Result<Vec<RoomAnswerers>, PortFailure>> {
    Box::pin(async move {
      let answer: Answerers = self.caller.post("/rooms/answerers", &RoomAsk { chat_id }).await?;
      Ok(answer.rooms)
    })
  }

  fn spoken_in<'a>(
    &'a self,
    chat_id: i64,
    people: &'a [i64],
  ) -> BoxFuture<'a, Result<Vec<i64>, PortFailure>> {
    Box::pin(async move {
      let answer: People<Vec<i64>> =
        self.caller.post("/rooms/spoken-in", &SpokenAsk { chat_id, people }).await?;
      Ok(answer.people)
    })
  }

  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<PersonName>, PortFailure>> {
    Box::pin(async move {
      let answer: Names = self.caller.post("/people/names", &People { people }).await?;
      Ok(answer.names)
    })
  }

  fn waits_on_happening<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Waits = self.caller.post("/promises/waits", &TextAsk { text }).await?;
      Ok(answer.waits)
    })
  }

  fn roads(&self, chat_id: i64) -> BoxFuture<'_, Result<Roads, PortFailure>> {
    Box::pin(async move { self.caller.post("/rooms/roads", &ChatAsk { chat_id }).await })
  }

  fn sender_is_owner(&self, chat_id: i64, message_id: i64) -> BoxFuture<'_, Result<bool, PortFailure>> {
    Box::pin(async move {
      let ask = MessageAsk {
        chat_id,
        message_id,
      };
      let answer: Owner = self.caller.post("/messages/sender-is-owner", &ask).await?;
      Ok(answer.owner)
    })
  }

  fn owner_spoke(
    &self,
    chat_id: i64,
    person: i64,
    before: i64,
    seconds: i64,
  ) -> BoxFuture<'_, Result<OwnerSpoke, PortFailure>> {
    Box::pin(async move {
      let ask = OwnerSpokeAsk {
        chat_id,
        person,
        before,
        seconds,
      };
      self.caller.post("/rooms/owner-spoke", &ask).await
    })
  }

  fn rest_state(&self) -> BoxFuture<'_, Result<Rest, PortFailure>> {
    Box::pin(async move { self.caller.post("/rest", &Nothing {}).await })
  }

  fn notify_owner<'a>(&'a self, notice: &'a OwnerNotice) -> BoxFuture<'a, Result<bool, PortFailure>> {
    Box::pin(async move {
      let answer: Done = self.caller.post("/owner/notice", notice).await?;
      Ok(answer.ok)
    })
  }
}
