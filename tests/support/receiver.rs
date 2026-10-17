//! The receiver in memory: the messages a test lays down, each agent's read marks on them, the rooms,
//! names, roads and own numbers a test sets, the owner as `OWNER`, the quiet instructions it takes, and every call
//! recorded.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use bmo_brain::ports::http::PortFailure;
use bmo_brain::ports::receiver::{
  Message, OwnerNotice, OwnerSpoke, PersonName, QuietInstruction, Receiver, RoadKind, Roads, RoomAnswerers,
  RoomCount, RoomSince, RoomTitle,
};
use futures_util::future::BoxFuture;
use serde_json::json;

use super::OWNER;
use super::calls::Calls;

#[derive(Debug, Default)]
pub struct FakeReceiver {
  pub messages: Mutex<Vec<Message>>,
  /// Each agent's read marks, by agent and room.
  pub marks: Mutex<BTreeMap<(String, i64), BTreeSet<i64>>>,
  pub titles: Mutex<Vec<RoomTitle>>,
  pub answerers: Mutex<Vec<RoomAnswerers>>,
  pub names: Mutex<BTreeMap<i64, String>>,
  /// The texts that wait for something to happen first.
  pub waiting: Mutex<BTreeSet<String>>,
  /// Each room's roads; a room not set is a group that is no one-to-one.
  pub roads: Mutex<BTreeMap<i64, Roads>>,
  /// Whether a quiet instruction is refused rather than taken.
  pub quiet_refused: Mutex<bool>,
  /// The sender each agent's own words stand under, by agent and room.
  pub own_numbers: Mutex<BTreeMap<(String, i64), i64>>,
  pub notices: Mutex<Vec<OwnerNotice>>,
  pub calls: Calls,
}

/// A message of `chat_id` from `sender`, sent at `sent_at`, with its id as its text.
pub fn message(chat_id: i64, message_id: i64, sender: i64, sent_at: i64) -> Message {
  Message {
    chat_id,
    message_id,
    sender_id: Some(sender),
    sender_name: format!("person-{sender}"),
    sent_at,
    text: format!("message {message_id}"),
    reply_to_message_id: None,
  }
}

impl FakeReceiver {
  pub fn with_messages(messages: Vec<Message>) -> Self {
    let fake = Self::default();
    *fake.messages.lock().unwrap() = messages;
    fake
  }

  fn marked(&self, agent: &str, chat_id: i64) -> BTreeSet<i64> {
    let marks = self.marks.lock().unwrap();
    marks.get(&(agent.to_owned(), chat_id)).cloned().unwrap_or_default()
  }

  fn room(&self, chat_id: i64) -> Vec<Message> {
    let mut room: Vec<Message> = self
      .messages
      .lock()
      .unwrap()
      .iter()
      .filter(|message| message.chat_id == chat_id)
      .cloned()
      .collect();
    room.sort_by_key(|message| (message.sent_at, message.message_id));
    room
  }
}

impl Receiver for FakeReceiver {
  fn unread_messages<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    limit: i64,
    since: Option<i64>,
  ) -> BoxFuture<'a, Result<Vec<Message>, PortFailure>> {
    let body = json!({ "agent": agent, "chatId": chat_id, "limit": limit, "since": since });
    self.calls.take("unread_messages", body);
    let marked = self.marked(agent, chat_id);
    let unread = self
      .room(chat_id)
      .into_iter()
      .filter(|message| !marked.contains(&message.message_id))
      .filter(|message| since.is_none_or(|since| message.sent_at >= since))
      .take(usize::try_from(limit).unwrap_or(0))
      .collect();
    Box::pin(async move { Ok(unread) })
  }

  fn read_messages<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    from: i64,
    limit: i64,
  ) -> BoxFuture<'a, Result<Vec<Message>, PortFailure>> {
    let body = json!({ "agent": agent, "chatId": chat_id, "from": from, "limit": limit });
    self.calls.take("read_messages", body);
    let marked = self.marked(agent, chat_id);
    let room = self.room(chat_id);
    let at = room.iter().position(|message| message.message_id == from);
    let read = room
      .into_iter()
      .enumerate()
      .filter(|(place, message)| at.map_or(message.message_id > from, |at| *place > at))
      .map(|(_, message)| message)
      .filter(|message| marked.contains(&message.message_id))
      .take(usize::try_from(limit).unwrap_or(0))
      .collect();
    Box::pin(async move { Ok(read) })
  }

  fn unread_counts<'a>(
    &'a self,
    agent: &'a str,
    since: &'a [RoomSince],
  ) -> BoxFuture<'a, Result<Vec<RoomCount>, PortFailure>> {
    self.calls.take("unread_counts", json!({ "agent": agent, "since": since }));
    let rooms: BTreeSet<i64> =
      self.messages.lock().unwrap().iter().map(|message| message.chat_id).collect();
    let counts = rooms
      .into_iter()
      .filter_map(|chat_id| {
        let marked = self.marked(agent, chat_id);
        let from = since.iter().find(|room| room.chat_id == chat_id).map(|room| room.since);
        let count = self
          .room(chat_id)
          .iter()
          .filter(|message| !marked.contains(&message.message_id))
          .filter(|message| from.is_none_or(|from| message.sent_at >= from))
          .count() as i64;
        (count > 0).then_some(RoomCount { chat_id, count })
      })
      .collect();
    Box::pin(async move { Ok(counts) })
  }

  fn read_count<'a>(&'a self, agent: &'a str, chat_id: i64) -> BoxFuture<'a, Result<i64, PortFailure>> {
    self.calls.take("read_count", json!({ "agent": agent, "chatId": chat_id }));
    let marked = self.marked(agent, chat_id);
    let count =
      self.room(chat_id).iter().filter(|message| marked.contains(&message.message_id)).count() as i64;
    Box::pin(async move { Ok(count) })
  }

  fn keep_read_marks<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
    message_ids: &'a [i64],
  ) -> BoxFuture<'a, Result<bool, PortFailure>> {
    let body = json!({ "agent": agent, "chatId": chat_id, "messageIds": message_ids });
    self.calls.take("keep_read_marks", body);
    let mut marks = self.marks.lock().unwrap();
    marks.entry((agent.to_owned(), chat_id)).or_default().extend(message_ids);
    Box::pin(async move { Ok(true) })
  }

  fn forget_read_marks<'a>(
    &'a self,
    agent: &'a str,
    chat_id: Option<i64>,
  ) -> BoxFuture<'a, Result<i64, PortFailure>> {
    self.calls.take("forget_read_marks", json!({ "agent": agent, "chatId": chat_id }));
    let mut marks = self.marks.lock().unwrap();
    let mut forgotten = 0;
    marks.retain(|(owner, room), ids| {
      let goes = owner == agent && chat_id.is_none_or(|chat_id| *room == chat_id);
      if goes {
        forgotten += ids.len() as i64;
      }
      !goes
    });
    Box::pin(async move { Ok(forgotten) })
  }

  fn room_titles(&self) -> BoxFuture<'_, Result<Vec<RoomTitle>, PortFailure>> {
    self.calls.take("room_titles", json!({}));
    let titles = self.titles.lock().unwrap().clone();
    Box::pin(async move { Ok(titles) })
  }

  fn room_answerers(&self, chat_id: Option<i64>) -> BoxFuture<'_, Result<Vec<RoomAnswerers>, PortFailure>> {
    self.calls.take("room_answerers", json!({ "chatId": chat_id }));
    let all = self.answerers.lock().unwrap().clone();
    let rooms = all
      .into_iter()
      .filter(|room| chat_id.is_none_or(|chat_id| room.chat_id == chat_id))
      .collect();
    Box::pin(async move { Ok(rooms) })
  }

  fn spoken_in<'a>(
    &'a self,
    chat_id: i64,
    people: &'a [i64],
  ) -> BoxFuture<'a, Result<Vec<i64>, PortFailure>> {
    self.calls.take("spoken_in", json!({ "chatId": chat_id, "people": people }));
    let senders: BTreeSet<i64> =
      self.room(chat_id).iter().filter_map(|message| message.sender_id).collect();
    let spoke = people.iter().copied().filter(|person| senders.contains(person)).collect();
    Box::pin(async move { Ok(spoke) })
  }

  fn spoke_between(
    &self,
    chat_id: i64,
    person: i64,
    from: i64,
    to: i64,
  ) -> BoxFuture<'_, Result<bool, PortFailure>> {
    let body = json!({ "chatId": chat_id, "person": person, "from": from, "to": to });
    self.calls.take("spoke_between", body);
    let spoke = self
      .room(chat_id)
      .iter()
      .any(|message| message.sender_id == Some(person) && message.sent_at >= from && message.sent_at <= to);
    Box::pin(async move { Ok(spoke) })
  }

  fn own_number<'a>(
    &'a self,
    agent: &'a str,
    chat_id: i64,
  ) -> BoxFuture<'a, Result<Option<i64>, PortFailure>> {
    self.calls.take("own_number", json!({ "agent": agent, "chatId": chat_id }));
    let number = self.own_numbers.lock().unwrap().get(&(agent.to_owned(), chat_id)).copied();
    Box::pin(async move { Ok(number) })
  }

  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<PersonName>, PortFailure>> {
    self.calls.take("display_names", json!({ "people": people }));
    let names = self.names.lock().unwrap();
    let named = people
      .iter()
      .map(|&person| PersonName {
        person,
        name: names.get(&person).cloned(),
      })
      .collect();
    Box::pin(async move { Ok(named) })
  }

  fn waits_on_happening<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("waits_on_happening", json!({ "text": text }));
    let waits = self.waiting.lock().unwrap().contains(text);
    Box::pin(async move { Ok(waits) })
  }

  fn roads(&self, chat_id: i64) -> BoxFuture<'_, Result<Roads, PortFailure>> {
    self.calls.take("roads", json!({ "chatId": chat_id }));
    let set = self.roads.lock().unwrap().get(&chat_id).copied();
    let roads = set.unwrap_or(Roads {
      kind: RoadKind::Group,
      one_to_one: false,
    });
    Box::pin(async move { Ok(roads) })
  }

  fn sender_is_owner(&self, chat_id: i64, message_id: i64) -> BoxFuture<'_, Result<bool, PortFailure>> {
    self
      .calls
      .take("sender_is_owner", json!({ "chatId": chat_id, "messageId": message_id }));
    let owner = self
      .room(chat_id)
      .iter()
      .any(|message| message.message_id == message_id && message.sender_id == Some(OWNER));
    Box::pin(async move { Ok(owner) })
  }

  fn owner_spoke(
    &self,
    chat_id: i64,
    person: i64,
    before: i64,
    seconds: i64,
  ) -> BoxFuture<'_, Result<OwnerSpoke, PortFailure>> {
    let body = json!({ "chatId": chat_id, "person": person, "before": before, "seconds": seconds });
    self.calls.take("owner_spoke", body);
    let spoke = self.room(chat_id).iter().any(|message| {
      message.sender_id == Some(OWNER) && message.sent_at < before && message.sent_at >= before - seconds
    });
    let answer = OwnerSpoke {
      owner: person == OWNER,
      spoke,
    };
    Box::pin(async move { Ok(answer) })
  }

  fn quiet_instruction<'a>(
    &'a self,
    instruction: &'a QuietInstruction,
  ) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("quiet_instruction", instruction);
    let taken = !*self.quiet_refused.lock().unwrap();
    Box::pin(async move { Ok(taken) })
  }

  fn notify_owner<'a>(&'a self, notice: &'a OwnerNotice) -> BoxFuture<'a, Result<bool, PortFailure>> {
    self.calls.take("notify_owner", notice);
    self.notices.lock().unwrap().push(notice.clone());
    Box::pin(async move { Ok(true) })
  }
}
