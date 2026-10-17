//! The memory tools' routes (docs/memories.md, 「The memory routes」; docs/boundary-api.md, 「Memory rows」, 「Notes」
//! and 「Watches and reminders」): a memory row written, moved on, checked off, taken down, put back, raised or
//! listed, a note kept for the room's digest, and a watch set. Each body carries the turn the tool runs in, and acts
//! in the turn's own room unless `chat` names another or `global` names none; the work is the memory role's library,
//! `memory-rows`, read against the receiver and the agency.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use brain_support::ceiling::{MAX_ANSWER_CHARS, Said, cap_items};
use brain_support::database::Fault;
use futures_util::future::BoxFuture;
use memory_rows::memories::{self, AddAsked, Answered, Asked, Handle, Refused, Work};
use memory_rows::notes::{self, NoteAsked};
use memory_rows::people::{Known, KnownRoom, KnownRooms};
use memory_rows::watches::{self, LaunchWatch, WatchArguments};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{Answer, Api, Body, reading, refusal};
use crate::Failure;
use crate::editors::archive::pending_notes;
use crate::editors::fragments::store::now_seconds;
use crate::editors::people::launcher::wall_clock;
use crate::ports::agency::Agency;
use crate::ports::receiver::Receiver;
use crate::schedules::launches::Launches;

/// The memory tools' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/memory/list", post(memory_list))
    .route("/memory/add", post(memory_add))
    .route("/memory/update", post(memory_update))
    .route("/memory/done", post(memory_done))
    .route("/memory/retire", post(memory_retire))
    .route("/memory/revive", post(memory_revive))
    .route("/memory/candidate", post(memory_candidate))
    .route("/notes/add", post(note_add))
    .route("/notes/list", post(note_list))
    .route("/watches/set", post(watch_set))
}

/// The turn a tool runs in.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
  pub agent: Option<String>,
  pub chat_id: Option<i64>,
  pub message_id: Option<i64>,
}

const NO_ROOM: &str =
  "This call was made outside any room's turn, so it has no room to act in: name one with chat.";
const NO_AGENT: &str =
  "This call was made outside any agent's turn, so there is no agent whose memory it would be.";

// Why a call cannot go on, as its answer says it.
type Stop = Json<Value>;

fn stopped(code: &str, message: &str) -> Stop {
  refusal(code, message, Map::new(), vec![])
}

impl Turn {
  // The agent the turn is.
  fn agent(&self) -> Result<&str, Stop> {
    self.agent.as_deref().ok_or_else(|| stopped("no-agent", NO_AGENT))
  }

  // The room a call acts in: the one it named, or the turn's own.
  fn room(&self, named: Option<i64>) -> Result<i64, Stop> {
    named.or(self.chat_id).ok_or_else(|| stopped("no-room", NO_ROOM))
  }

  // The scope a memory call names: none for a global row, which names no room, and otherwise its room.
  fn scope(&self, global: bool, chat: Option<i64>) -> Result<Option<i64>, Stop> {
    if global {
      return match chat {
        Some(_) => Err(stopped("memory-refused", "global and chat name two different scopes; give one")),
        None => Ok(None),
      };
    }

    self.room(chat).map(Some)
  }
}

/// A memory operation's answer as the route answers it: its lines, or its refusal after the lines it wrote first.
pub fn answered(result: Answered) -> Json<Value> {
  match result {
    Ok(lines) => reading(lines),
    Err(Refused {
      lines,
      code,
      parameters,
      reason,
    }) => {
      let params = parameters.into_iter().map(|(name, value)| (name.to_owned(), json!(value))).collect();

      refusal(code, &reason, params, lines)
    }
  }
}

// A row as a call names it.
fn handle(given: &str) -> Result<Handle, Stop> {
  memories::handle_of(Some(given), "memory").map_err(|refused| answered(Err(refused)))
}

// What a memory row's text is read against, through the receiver and the agency.
struct Ported<'a> {
  agency: &'a dyn Agency,
  receiver: &'a dyn Receiver,
}

fn fault(why: impl std::fmt::Display) -> Fault {
  Fault::new(why.to_string())
}

impl Known for Ported<'_> {
  fn rooms(&self) -> BoxFuture<'_, Result<KnownRooms, Fault>> {
    Box::pin(async move {
      let titles = self.receiver.room_titles().await.map_err(fault)?;
      let mut senders: Vec<i64> = titles.iter().flat_map(|room| room.senders.iter().copied()).collect();

      senders.sort_unstable();
      senders.dedup();

      Ok(KnownRooms {
        rooms: titles
          .into_iter()
          .map(|room| KnownRoom {
            chat_id: room.chat_id,
            title: room.title,
          })
          .collect(),
        senders,
      })
    })
  }

  fn display_names<'a>(&'a self, people: &'a [i64]) -> BoxFuture<'a, Result<Vec<Option<String>>, Fault>> {
    Box::pin(async move {
      let named = self.receiver.display_names(people).await.map_err(fault)?;

      Ok(
        people
          .iter()
          .map(|person| named.iter().find(|one| one.person == *person).and_then(|one| one.name.clone()))
          .collect(),
      )
    })
  }

  fn agent_names(&self) -> BoxFuture<'_, Result<Vec<String>, Fault>> {
    Box::pin(async move {
      Ok(
        self
          .agency
          .registrations(None)
          .await
          .map_err(fault)?
          .into_iter()
          .filter(|registration| registration.awake)
          .map(|registration| registration.label.unwrap_or(registration.name))
          .collect(),
      )
    })
  }

  fn waits_on_happening<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<bool, Fault>> {
    Box::pin(async move { self.receiver.waits_on_happening(text).await.map_err(fault) })
  }
}

// A watch's check put in a run of its own through the agency's unit launch.
struct Launched<'a>(Launches<'a>);

impl LaunchWatch for Launched<'_> {
  fn launch(&self, id: i64) -> BoxFuture<'_, Result<bool, Fault>> {
    Box::pin(
      async move { self.0.launch_watch_of(id, wall_clock()).await.map_err(|why: Failure| fault(why)) },
    )
  }
}

fn flatten(answer: Result<Json<Value>, Stop>) -> Answer {
  Ok(answer.unwrap_or_else(|stop| stop))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAsked {
  #[serde(default)]
  turn: Turn,
  chat: Option<i64>,
  #[serde(default)]
  global: bool,
  #[serde(rename = "type")]
  kind: Option<String>,
  #[serde(default)]
  all: bool,
}

// A listing cut to whole lines under the ceiling every answer stands under, saying how to ask for the rest.
fn listed(lines: Vec<String>) -> Json<Value> {
  let mut cut = Said::new("lines", "Narrow the question with type.");

  cut.limit = MAX_ANSWER_CHARS - 200;

  let capped = cap_items(&lines, |kept| json!({ "lines": kept }).to_string(), cut);
  let mut kept: Vec<String> = capped.kept.to_vec();

  kept.extend(capped.note);

  reading(kept)
}

async fn memory_list(State(api): State<Arc<Api>>, Body(asked): Body<ListAsked>) -> Answer {
  let answer = async {
    let scope = asked.turn.scope(asked.global, asked.chat)?;
    let agent = asked.turn.agent()?;
    let result = memories::list(&api.db, agent, scope, asked.kind.as_deref(), asked.all).await;

    Ok(match result {
      Ok(lines) => listed(lines),
      refused => answered(refused),
    })
  };

  flatten(answer.await)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddArgs {
  #[serde(default)]
  turn: Turn,
  chat: Option<i64>,
  #[serde(default)]
  global: bool,
  #[serde(rename = "type")]
  kind: String,
  action: String,
  condition: Option<String>,
  on: Option<String>,
  until: Option<String>,
  name: Option<String>,
  every: Option<i64>,
  within: Option<i64>,
  slug: Option<String>,
  #[serde(rename = "for")]
  for_user: Option<i64>,
  by: Option<i64>,
  message: Option<i64>,
  todo: Option<i64>,
  reach: Option<String>,
  replaces: Option<String>,
}

async fn memory_add(State(api): State<Arc<Api>>, Body(args): Body<AddArgs>) -> Answer {
  let answer = async {
    let scope = args.turn.scope(args.global, args.chat)?;
    let agent = args.turn.agent()?;
    let known = Ported {
      agency: api.agency.as_ref(),
      receiver: api.receiver.as_ref(),
    };
    let launch = Launched(api.launches());
    let work = Work {
      db: &api.db,
      owner_id: api.owner,
      known: &known,
      launch: &launch,
      parses: &watches::condition_parses,
    };
    let asked = AddAsked {
      agent: agent.to_owned(),
      scope,
      kind: Some(args.kind.clone()),
      action: args.action.clone(),
      slug: args.slug.clone(),
      condition: args.condition.clone(),
      on: args.on.clone(),
      until: args.until.clone(),
      name: args.name.clone(),
      every: args.every.map(|every| every.to_string()),
      within: args.within.map(|within| within.to_string()),
      for_user: args.for_user,
      set_by: args.by,
      message_id: args.message,
      todo_id: args.todo,
      reach: args.reach.clone(),
      replaces: args.replaces.clone(),
    };

    Ok(answered(memories::add(&work, &asked, now_seconds()).await))
  };

  flatten(answer.await)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeArgs {
  #[serde(default)]
  turn: Turn,
  memory: String,
  chat: Option<i64>,
  #[serde(default)]
  global: bool,
  action: Option<String>,
  text: Option<String>,
  why: Option<String>,
  #[allow(dead_code)]
  message: Option<i64>,
}

// Which change a route makes to a row it names.
#[derive(Clone, Copy)]
enum Change {
  Update,
  Done,
  Retire,
  Revive,
  Candidate,
}

impl Change {
  // Whether the change speaks for a row tied to no room: update and done act on a room's own row.
  fn takes_global(self) -> bool {
    !matches!(self, Self::Update | Self::Done)
  }
}

async fn change(api: &Api, args: &ChangeArgs, change: Change) -> Answer {
  let answer = async {
    let scope = args.turn.scope(change.takes_global() && args.global, args.chat)?;
    let handle = handle(&args.memory)?;
    let agent = args.turn.agent()?;
    let asked = Asked {
      agent,
      scope,
      handle: &handle,
    };
    let now = now_seconds();
    let result = match change {
      Change::Update => memories::update(&api.db, &asked, args.action.as_deref(), now).await,
      Change::Done => memories::done(&api.db, &asked, args.text.as_deref(), now).await,
      Change::Retire => memories::retire(&api.db, &asked, args.why.as_deref(), now).await,
      Change::Revive => memories::revive(&api.db, &asked).await,
      Change::Candidate => memories::candidate(&api.db, &asked, args.why.as_deref(), now).await,
    };

    Ok(answered(result))
  };

  flatten(answer.await)
}

async fn memory_update(State(api): State<Arc<Api>>, Body(args): Body<ChangeArgs>) -> Answer {
  change(&api, &args, Change::Update).await
}

async fn memory_done(State(api): State<Arc<Api>>, Body(args): Body<ChangeArgs>) -> Answer {
  change(&api, &args, Change::Done).await
}

async fn memory_retire(State(api): State<Arc<Api>>, Body(args): Body<ChangeArgs>) -> Answer {
  change(&api, &args, Change::Retire).await
}

async fn memory_revive(State(api): State<Arc<Api>>, Body(args): Body<ChangeArgs>) -> Answer {
  change(&api, &args, Change::Revive).await
}

async fn memory_candidate(State(api): State<Arc<Api>>, Body(args): Body<ChangeArgs>) -> Answer {
  change(&api, &args, Change::Candidate).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteArgs {
  #[serde(default)]
  turn: Turn,
  #[serde(default)]
  text: String,
  author: Option<i64>,
  #[serde(default)]
  global: bool,
  chat: Option<i64>,
  #[allow(dead_code)]
  message: Option<i64>,
}

async fn note_add(State(api): State<Arc<Api>>, Body(args): Body<NoteArgs>) -> Answer {
  let chat_id = match args.turn.room(args.chat) {
    Ok(chat_id) => chat_id,
    Err(stop) => return Ok(stop),
  };
  let agent = match args.turn.agent() {
    Ok(agent) => agent,
    Err(stop) => return Ok(stop),
  };
  let asked = NoteAsked {
    agent,
    chat_id,
    text: &args.text,
    author: args.author,
    shared: args.global,
  };

  Ok(answered(notes::note(&api.db, &asked, now_seconds()).await.map_err(Failure::from)?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteListAsked {
  agent: String,
  chat_id: Option<i64>,
}

async fn note_list(State(api): State<Arc<Api>>, Body(asked): Body<NoteListAsked>) -> Answer {
  let notes: Vec<Value> = pending_notes(&api.db, Some(&asked.agent))
    .await?
    .into_iter()
    .filter(|note| asked.chat_id.is_none_or(|chat_id| note.chat_id == chat_id))
    .map(|note| {
      json!({
        "id": note.id,
        "agent": note.agent,
        "chatId": note.chat_id,
        "author": note.author,
        "text": note.text,
        "global": note.shared,
        "at": note.created_at,
      })
    })
    .collect();

  Ok(Json(json!({ "notes": notes })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchArgs {
  #[serde(default)]
  turn: Turn,
  chat: Option<i64>,
  until: Option<String>,
  at: Option<String>,
  name: Option<String>,
  note: Option<String>,
  every: Option<i64>,
  within: Option<i64>,
  item: Option<i64>,
  #[allow(dead_code)]
  message: Option<i64>,
}

async fn watch_set(State(api): State<Arc<Api>>, Body(args): Body<WatchArgs>) -> Answer {
  let chat_id = match args.turn.room(args.chat) {
    Ok(chat_id) => chat_id,
    Err(stop) => return Ok(stop),
  };
  let agent = match args.turn.agent() {
    Ok(agent) => agent,
    Err(stop) => return Ok(stop),
  };
  // Who set it off is the room the turn answers, as a session names its own chat.
  let chat = chat_id.to_string();
  let by = format!("chat:{}", args.turn.chat_id.unwrap_or(chat_id));
  let every = args.every.map(|every| every.to_string());
  let within = args.within.map(|within| within.to_string());
  let item = args.item.map(|item| item.to_string());
  let asked = WatchArguments {
    agent,
    chat: Some(&chat),
    by: Some(&by),
    until: args.until.as_deref(),
    at: args.at.as_deref(),
    name: args.name.as_deref(),
    note: args.note.as_deref(),
    every: every.as_deref(),
    within: within.as_deref(),
    item: item.as_deref(),
  };
  let launch = Launched(api.launches());
  let result = watches::watch(&api.db, &asked, &launch, &watches::condition_parses, now_seconds())
    .await
    .map_err(Failure::from)?;

  Ok(answered(result))
}
