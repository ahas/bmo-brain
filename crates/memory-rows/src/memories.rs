//! Everything an agent holds in a room, or holds for no room at all, written, moved on, checked off, taken
//! down, put back, raised and listed (docs/memories.md, 「The memory routes」): a promise it made, something
//! it will check later, a follow-up handed to it, a rule the owner set for a situation, and the state of
//! something under way. Only the brain's database is touched, and a watch set with a row is the one thing
//! started beside it; nothing is ever sent to a chat, and no row is ever removed.
//!
//! A scope is a room's id, or none for a row tied to no room. The scope a caller speaks for is what every
//! write is checked against, and so is the agent: every operation acts for one agent and touches its rows
//! alone (docs/agent-ownership.md).

use std::sync::LazyLock;

use brain_support::ceiling::{Said, cap_items};
use brain_support::database::{Fault, all, one, run};
use brain_support::js;
use rand::Rng;
use regex::Regex;
use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, TransactionTrait};

use crate::events::{EVENT_KINDS, Event, read_event};
use crate::people::{Known, KnownRooms};
use crate::watches::{self as wakes, WatchAsked};

/// The kinds a row is, in the order a refusal names them.
pub const MEMORY_TYPES: [&str; 5] = ["promise", "check", "followup", "situation", "state"];

/// The kinds of room a global row reaches, in the order a refusal names them.
pub const MEMORY_REACHES: [&str; 3] = ["owner", "group", "all"];

/// The longest condition a row holds: a line the matcher reads against a situation.
pub const CONDITION_CAP: usize = 500;

/// The longest action a row holds: the body that attaches.
pub const ACTION_CAP: usize = 2_000;

/// What an operation answered where it would not do what it was asked: the lines it had already written,
/// the refusal's code with its parameters (docs/boundary-api.md, 「Memory refusals」), and the sentence
/// saying why.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
  pub lines: Vec<String>,
  pub code: &'static str,
  pub parameters: Vec<(&'static str, String)>,
  pub reason: String,
}

impl Refused {
  /// A refusal no code of its own names, `memory-refused`, its sentence saying why.
  pub fn of(reason: impl Into<String>) -> Self {
    Self::coded("memory-refused", vec![], reason)
  }

  /// A refusal under its own code, with the code's parameters.
  pub fn coded(
    code: &'static str,
    parameters: Vec<(&'static str, String)>,
    reason: impl Into<String>,
  ) -> Self {
    Self {
      lines: vec![],
      code,
      parameters,
      reason: reason.into(),
    }
  }
}

impl From<Fault> for Refused {
  fn from(fault: Fault) -> Self {
    Self::of(fault.detail)
  }
}

impl From<sea_orm::DbErr> for Refused {
  fn from(error: sea_orm::DbErr) -> Self {
    Fault::from(error).into()
  }
}

/// What a memory operation answers: the lines it prints, or why it did nothing more.
pub type Answered = Result<Vec<String>, Refused>;

/// A row named by its number or its slug; both name the one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handle {
  Id(i64),
  Slug(String),
}

impl std::fmt::Display for Handle {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Id(id) => write!(out, "{id}"),
      Self::Slug(slug) => write!(out, "{slug}"),
    }
  }
}

static WHOLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?[0-9]+$").expect("a whole number"));

/// A handle as an argument gave it, or why it names nothing.
pub fn handle_of(given: Option<&str>, name: &str) -> Result<Handle, Refused> {
  let trimmed = js::trim(given.unwrap_or_default());

  if trimmed.is_empty() {
    return Err(Refused::of(format!("{name} takes the row's id or its slug")));
  }

  if WHOLE.is_match(trimmed) {
    return trimmed
      .parse()
      .map(Handle::Id)
      .map_err(|_| Refused::of(format!("there is no memory '{trimmed}'")));
  }

  Ok(Handle::Slug(trimmed.to_owned()))
}

/// One row whole, as a listing shows it and a change answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memory {
  pub id: i64,
  pub agent: String,
  pub slug: String,
  pub kind: String,
  pub chat_id: Option<i64>,
  pub reach: String,
  pub condition: String,
  pub action: String,
  pub for_user: Option<i64>,
  pub todo_id: Option<i64>,
  pub set_at: i64,
  pub active: bool,
  pub last_hit_at: Option<i64>,
  pub hits: i64,
  pub done_at: Option<i64>,
  pub done_text: Option<String>,
  pub down_reason: Option<String>,
  pub down_at: Option<i64>,
  pub candidate_why: Option<String>,
  pub event: Option<Event>,
  pub event_met_at: Option<i64>,
  pub moved_at: Option<i64>,
}

fn memory_of_row(row: &QueryResult) -> Result<Memory, Fault> {
  let event: String = row.try_get("", "event")?;

  Ok(Memory {
    id: row.try_get("", "id")?,
    agent: row.try_get("", "agent")?,
    slug: row.try_get("", "slug")?,
    kind: row.try_get("", "type")?,
    chat_id: row.try_get("", "chat_id")?,
    reach: row.try_get("", "reach")?,
    condition: row.try_get("", "condition")?,
    action: row.try_get("", "action")?,
    for_user: row.try_get("", "for_user")?,
    todo_id: row.try_get("", "todo_id")?,
    set_at: row.try_get("", "set_at")?,
    active: row.try_get::<i64>("", "active")? != 0,
    last_hit_at: row.try_get("", "last_hit_at")?,
    hits: row.try_get("", "hits")?,
    done_at: row.try_get("", "done_at")?,
    done_text: row.try_get("", "done_text")?,
    down_reason: row.try_get("", "down_reason")?,
    down_at: row.try_get("", "down_at")?,
    candidate_why: row.try_get("", "candidate_why")?,
    event: if event.is_empty() { None } else { read_event(&event) },
    event_met_at: row.try_get("", "event_met_at")?,
    moved_at: row.try_get("", "moved_at")?,
  })
}

/// An event as a row names it: its kind, and the item where it names one.
pub fn event_text(event: &Event) -> String {
  match event.item {
    Some(item) => format!("{}:{item}", event.kind),
    None => event.kind.clone(),
  }
}

/// An event as an argument names it, or the refusal saying why it names none. The item is a TODO item's
/// number, with or without its `#`.
pub fn event_asked(text: &str) -> Result<Event, Refused> {
  let given = js::trim(text);
  let parts: Vec<&str> = given.split(':').collect();
  let found = EVENT_KINDS.iter().find(|(kind, _)| *kind == js::trim(parts[0]));
  let Some((kind, names_item)) = found.filter(|_| parts.len() <= 2) else {
    let kinds: Vec<String> = EVENT_KINDS
      .iter()
      .map(|(kind, item)| if *item { format!("{kind}[:<item>]") } else { (*kind).to_owned() })
      .collect();
    let kinds = kinds.join(", ");
    let named = if given.is_empty() { "(empty)" } else { given };

    return Err(Refused::coded(
      "event-unknown",
      vec![("given", given.to_owned()), ("kinds", kinds.clone())],
      format!("unknown event: {named}. Use one of {kinds}."),
    ));
  };
  let Some(item) = parts.get(1) else {
    return Ok(Event {
      kind: (*kind).to_owned(),
      item: None,
    });
  };

  if !names_item {
    return Err(Refused::coded(
      "event-takes-no-item",
      vec![("kind", (*kind).to_owned()), ("given", given.to_owned())],
      format!("the {kind} event takes no item: {given}"),
    ));
  }

  let item = js::trim(item);

  match js::number(item.strip_prefix('#').unwrap_or(item)) {
    Some(number) if number.fract() == 0.0 && number > 0.0 && number < 9_007_199_254_740_992.0 => {
      Ok(Event {
        kind: (*kind).to_owned(),
        item: Some(number as i64),
      })
    }
    _ => Err(Refused::coded(
      "event-bad-item",
      vec![("given", given.to_owned())],
      format!("the event's item is no item number: {given}"),
    )),
  }
}

fn when(epoch_seconds: i64) -> String {
  brain_support::clock::clock(epoch_seconds)
}

// The scope a row is listed under: its room, or the kinds of room a global row reaches (`global-scope`).
fn whose(memory: &Memory) -> String {
  match memory.chat_id {
    None => format!("global · {}", memory.reach),
    Some(chat_id) => format!("chat {chat_id}"),
  }
}

/// One row as a listing shows it. A row handed over as one strand of a TODO item's open work is marked with
/// the item (`from-item`).
pub fn memory_line(memory: &Memory) -> String {
  let for_whom = memory.for_user.map_or_else(String::new, |user| format!(" · for user {user}"));
  let from_item = memory
    .todo_id
    .map_or_else(String::new, |item| format!(" · open strand of item #{item}"));
  let matched = if memory.hits == 0 {
    String::new()
  } else {
    format!(
      " · {} match{}, last {}",
      memory.hits,
      if memory.hits == 1 { "" } else { "es" },
      when(memory.last_hit_at.unwrap_or_default())
    )
  };
  let moved = memory.moved_at.map_or_else(String::new, |at| format!(" · moved {}", when(at)));
  let head = format!(
    "#{} {} · {} · {} · set {}{moved}{for_whom}{from_item}{matched}",
    memory.id,
    memory.kind,
    memory.slug,
    whose(memory),
    when(memory.set_at)
  );
  let mut lines = vec![format!("{} {head}", if memory.active { "[on ]" } else { "[off]" })];

  if !memory.condition.is_empty() {
    lines.push(format!("      when: {}", memory.condition));
  }

  if let Some(event) = &memory.event {
    let came = memory.event_met_at.map_or_else(String::new, |at| format!(" · came {}", when(at)));

    lines.push(format!("      on: {}{came}", event_text(event)));
  }

  lines.push(format!("      then: {}", memory.action));

  if let Some(done_at) = memory.done_at {
    let how = memory.done_text.as_ref().map_or_else(String::new, |text| format!(": {text}"));

    lines.push(format!("      checked {}{how}", when(done_at)));
  }

  if let Some(reason) = &memory.down_reason {
    let at = memory.down_at.map_or_else(String::new, |at| format!("{} ", when(at)));

    lines.push(format!("      down {at}· {reason}"));
  }

  if let Some(why) = &memory.candidate_why {
    lines.push(format!("      candidate · {why}"));
  }

  lines.join("\n")
}

/// One row by either handle, or none where no row answers to it.
pub async fn memory_of<C: ConnectionTrait>(db: &C, handle: &Handle) -> Result<Option<Memory>, Fault> {
  let row = match handle {
    Handle::Id(id) => one(db, r#"SELECT * FROM "memories" WHERE "id" = $1"#, vec![(*id).into()]).await?,
    Handle::Slug(slug) => {
      one(db, r#"SELECT * FROM "memories" WHERE "slug" = $1"#, vec![slug.as_str().into()]).await?
    }
  };

  row.as_ref().map(memory_of_row).transpose()
}

async fn returned<C: ConnectionTrait>(
  db: &C,
  sql: &str,
  values: Vec<sea_orm::Value>,
) -> Result<Option<Memory>, Fault> {
  one(db, sql, values).await?.as_ref().map(memory_of_row).transpose()
}

/// What every operation but the first is asked for: the agent it acts for, the scope it speaks for, and the
/// row.
pub struct Asked<'a> {
  pub agent: &'a str,
  pub scope: Option<i64>,
  pub handle: &'a Handle,
}

// Why a row named by a handle is not this agent's to change, where no row answers to it at all the change
// itself saying so.
async fn other_agents(db: &DatabaseConnection, handle: &Handle, agent: &str) -> Result<(), Refused> {
  match memory_of(db, handle).await? {
    Some(standing) if standing.agent != agent => Err(Refused::of(format!(
      "memory '{handle}' is another agent's; an agent touches only its own rows"
    ))),
    _ => Ok(()),
  }
}

// The slug a change was asked about, whichever handle the caller used.
async fn slug_of(db: &DatabaseConnection, handle: &Handle) -> Result<String, Refused> {
  match handle {
    Handle::Slug(slug) => Ok(slug.clone()),
    Handle::Id(_) => memory_of(db, handle)
      .await?
      .map(|standing| standing.slug)
      .ok_or_else(|| Refused::of(format!("there is no memory '{handle}'"))),
  }
}

fn reason_of(why: Option<&str>) -> Result<String, Refused> {
  let said = js::trim(why.unwrap_or_default());

  if said.is_empty() {
    return Err(Refused::coded(
      "why-missing",
      vec![],
      "--why '<reason>' says why, and a row never comes down for no reason anybody can read",
    ));
  }

  if js::units(said) > CONDITION_CAP {
    return Err(Refused::of(format!(
      "the reason is {} characters; at most {CONDITION_CAP}",
      js::units(said)
    )));
  }

  Ok(said.to_owned())
}

// Why a change of a row taken down or put back could not be made, said the way a session can act on it.
enum Unchanged {
  Missing,
  OtherRoom,
  AlreadyDown(Option<String>),
  AlreadyUp,
}

fn unchanged(handle: &Handle, why: Unchanged) -> Refused {
  Refused::of(match why {
    Unchanged::Missing => format!("there is no memory '{handle}'"),
    Unchanged::OtherRoom => format!(
      "memory '{handle}' belongs to another scope; a room touches only its own rows and a global row is touched with --global"
    ),
    Unchanged::AlreadyDown(reason) => {
      format!("memory '{handle}' is already down: {}", reason.unwrap_or_default())
    }
    Unchanged::AlreadyUp => format!("memory '{handle}' is already active"),
  })
}

// The row a caller may touch in its scope, or why it may not.
async fn in_scope(
  db: &DatabaseConnection,
  slug: &str,
  scope: Option<i64>,
) -> Result<Result<Memory, Unchanged>, Fault> {
  Ok(match memory_of(db, &Handle::Slug(slug.to_owned())).await? {
    None => Err(Unchanged::Missing),
    Some(standing) if standing.chat_id != scope => Err(Unchanged::OtherRoom),
    Some(standing) => Ok(standing),
  })
}

// A row taken down with its reason, the row whole after it.
async fn take_down<C: ConnectionTrait>(
  db: &C,
  slug: &str,
  reason: &str,
  now: i64,
) -> Result<Option<Memory>, Fault> {
  returned(
    db,
    r#"UPDATE "memories" SET "active" = 0, "down_reason" = $1, "down_at" = $2 WHERE "slug" = $3 RETURNING *"#,
    vec![reason.into(), now.into(), slug.into()],
  )
  .await
}

/// Every standing watch serving a row closed, ended, each as the line said of it beside the line that closed
/// the row (docs/memories.md, 「A watch and the memory it serves」). No memo goes out, since the agent closing
/// the row already knows.
pub async fn end_watches_of(
  db: &DatabaseConnection,
  memory_id: i64,
  now: i64,
) -> Result<Vec<String>, Fault> {
  let served = memory_of(db, &Handle::Id(memory_id)).await?;

  if served.as_ref().is_some_and(|row| row.active && row.done_at.is_none()) {
    return Ok(vec![]);
  }

  let standing = all(
    db,
    r#"SELECT * FROM "watches" WHERE "state" = 'active' AND "memory_id" = $1 ORDER BY "id""#,
    vec![memory_id.into()],
  )
  .await?;
  let mut lines = vec![];

  for row in standing {
    let watch = wakes::watch_row(&row)?;
    let ended = run(
      db,
      r#"UPDATE "watches" SET "state" = 'cancelled', "ended_at" = $1 WHERE "id" = $2 AND "state" = 'active'"#,
      vec![now.into(), watch.id.into()],
    )
    .await?;

    if ended == 1 {
      lines.push(format!(
        "watch #{} ({}) cancelled with memory #{memory_id}",
        watch.id,
        wakes::watch_name(&watch)
      ));
    }
  }

  Ok(lines)
}

/// What a row is filed with, as the arguments give it.
#[derive(Clone, Debug, Default)]
pub struct AddAsked {
  pub agent: String,
  pub scope: Option<i64>,
  pub kind: Option<String>,
  pub action: String,
  pub slug: Option<String>,
  pub condition: Option<String>,
  pub on: Option<String>,
  pub until: Option<String>,
  pub name: Option<String>,
  pub every: Option<String>,
  pub within: Option<String>,
  pub for_user: Option<i64>,
  pub set_by: Option<i64>,
  pub message_id: Option<i64>,
  pub todo_id: Option<i64>,
  pub reach: Option<String>,
  pub replaces: Option<String>,
}

/// Where the operations stand: the brain's database, the owner a global row's text is checked without, what
/// a row's text is read against, how a watch a row sets is launched, and whether bash can read its check.
pub struct Work<'a> {
  pub db: &'a DatabaseConnection,
  pub owner_id: Option<i64>,
  pub known: &'a dyn Known,
  pub launch: &'a dyn wakes::LaunchWatch,
  pub parses: &'a (dyn Fn(&str) -> bool + Sync),
}

static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9-]+$").expect("a slug"));
static STANDALONE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?[0-9]+").expect("a number"));
static ID_FORM: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(&format!(r"(?i)(?-u:\b)id[{}]+-?[0-9]+", js::SPACE)).expect("an id written out")
});

/// Whom and what a global row may not name, as the archive knows them.
pub struct GlobalNames {
  pub user_ids: Vec<i64>,
  pub chat_ids: Vec<i64>,
  pub titles: Vec<String>,
  /// The names people are known by, the owner's own left out: the owner is in every room.
  pub people: Vec<String>,
}

/// The names a global row is checked against: the rooms and senders the archive knows, the rooms' titles but
/// those an awake agent goes by, and the names of everyone but the owner.
pub async fn global_names(known: &dyn Known, owner_id: Option<i64>) -> Result<GlobalNames, Fault> {
  let KnownRooms { rooms, mut senders } = known.rooms().await?;

  senders.sort_unstable();
  senders.dedup();

  let agent_names: Vec<String> =
    known.agent_names().await?.iter().map(|name| js::trim(name).to_lowercase()).collect();
  let chat_ids = rooms.iter().map(|room| room.chat_id).collect();
  let titles = rooms
    .into_iter()
    .filter_map(|room| room.title)
    .filter(|title| !agent_names.contains(&js::trim(title).to_lowercase()))
    .collect();
  let others: Vec<i64> = senders.iter().copied().filter(|id| Some(*id) != owner_id).collect();
  let mut people: Vec<String> = vec![];

  for name in known.display_names(&others).await?.into_iter().flatten() {
    if !js::trim(&name).is_empty() && !people.contains(&name) {
      people.push(name);
    }
  }

  Ok(GlobalNames {
    user_ids: senders,
    chat_ids,
    titles,
    people,
  })
}

/// What a global row's text names that it may not: a user or chat id the archive knows, a room's title, a
/// person's name, or a bare `id <number>`.
pub fn global_violations(text: &str, names: &GlobalNames) -> Vec<String> {
  let mut found: Vec<String> = vec![];
  let push = |verdict: String, found: &mut Vec<String>| {
    if !found.contains(&verdict) {
      found.push(verdict);
    }
  };

  for number in STANDALONE_NUMBER.find_iter(text) {
    // A minus right after a digit is the joint of a slug or a date, and the number is read without it.
    let after_digit =
      text[..number.start()].chars().next_back().is_some_and(|before| before.is_ascii_digit());
    let token = if after_digit { &number.as_str()[1..] } else { number.as_str() };

    if token.is_empty() || (after_digit && number.as_str().len() == 1) {
      continue;
    }

    let Ok(value) = token.parse::<i64>() else {
      continue;
    };

    if value.unsigned_abs() >= 1 << 53 {
      continue;
    }

    let bare = value.abs();
    let verdict = if value < 0 && names.chat_ids.contains(&value) {
      Some(format!("chat id {value}"))
    } else if names.user_ids.contains(&bare) {
      Some(format!("user id {bare}"))
    } else if names.chat_ids.contains(&bare) {
      Some(format!("chat id {bare}"))
    } else {
      None
    };

    if let Some(verdict) = verdict {
      push(verdict, &mut found);
    }
  }

  let lower = text.to_lowercase();

  for title in &names.titles {
    let needle = js::trim(title).to_lowercase();

    if js::units(&needle) >= 2 && lower.contains(&needle) {
      push(format!("room title {title}"), &mut found);
    }
  }

  for name in &names.people {
    let needle = js::trim(name).to_lowercase();

    if js::units(&needle) >= 2 && lower.contains(&needle) {
      push(format!("person {name}"), &mut found);
    }
  }

  for written in ID_FORM.find_iter(text) {
    push(format!("'{}'", written.as_str()), &mut found);
  }

  found
}

fn types_list() -> String {
  MEMORY_TYPES.join(", ")
}

/// A row filed, with the row it takes the place of taken down and the watch that wakes it set beside it.
/// `now` is in unix seconds.
pub async fn add(work: &Work<'_>, asked: &AddAsked, now: i64) -> Answered {
  let db = work.db;
  let scope = asked.scope;
  let global = scope.is_none();
  let kind = match asked.kind.as_deref() {
    Some(kind) if MEMORY_TYPES.contains(&kind) => kind,
    given => {
      let not = given.map_or_else(String::new, |given| format!(", not '{given}'"));

      return Err(Refused::of(format!("--type must be one of {}{not}", types_list())));
    }
  };
  let slug = js::trim(asked.slug.as_deref().unwrap_or_default()).to_owned();

  if !slug.is_empty() && !SLUG.is_match(&slug) {
    return Err(Refused::of(format!(
      "--slug names the row in lower-case letters, digits and hyphens, not '{slug}'"
    )));
  }

  let condition = js::trim(asked.condition.as_deref().unwrap_or_default()).to_owned();
  let action = js::trim(&asked.action).to_owned();

  if action.is_empty() {
    return Err(Refused::coded(
      "action-missing",
      vec![],
      "--action '<what>' is what attaches; give it, or --action - with the text on stdin",
    ));
  }

  let event = asked.on.as_deref().map(event_asked).transpose()?;

  if !condition.is_empty() && event.is_some() {
    return Err(Refused::of("--condition and --on are two ways of waiting; a row waits on one of them"));
  }

  let until = asked.until.as_deref();

  if until.is_some() && (global || event.is_some() || !condition.is_empty()) {
    return Err(Refused::of(if global {
      "--until sets a watch, and a watch comes back to a room: give the row a --chat"
    } else {
      "--until, --on and --condition are ways of waiting; a row waits on one of them"
    }));
  }

  if kind == "state" && (global || !condition.is_empty() || event.is_some() || until.is_some()) {
    return Err(Refused::of(if global {
      "a state is what is under way in one room: give it a --chat"
    } else {
      "a state stands in its room's every turn while it is open, so it takes no --condition, --on or --until"
    }));
  }

  if kind != "situation"
    && kind != "state"
    && event.is_none()
    && until.is_none()
    && work.known.waits_on_happening(&action).await?
  {
    return Err(Refused::of(
      "this row waits for something to happen first, and nothing would wake you when it does: give it --on <event> where it is a deployment, a job's end or the queue changing, or --until '<check>' — a short read-only command that exits 0 once it has happened — for anything else",
    ));
  }

  // The watch is asked for before the row is written, so a check the watch would refuse writes nothing.
  let watch_asked = match until {
    None => None,
    Some(until) => {
      let chat = scope.map(|scope| scope.to_string()).unwrap_or_default();
      let by = format!("chat:{chat}");
      let name = asked.name.clone().or_else(|| (!slug.is_empty()).then(|| slug.clone()));
      let arguments = wakes::WatchArguments {
        agent: &asked.agent,
        chat: Some(&chat),
        by: Some(&by),
        until: Some(until),
        at: None,
        name: name.as_deref(),
        note: Some(&action),
        every: asked.every.as_deref(),
        within: asked.within.as_deref(),
        item: None,
      };

      Some(wakes::watch_request(&arguments, now, work.parses).map_err(Refused::of)?)
    }
  };

  if kind == "situation" && condition.is_empty() && event.is_none() {
    return Err(Refused::coded(
      "situation-unpaired",
      vec![],
      "a situation row is a pair: --condition '<when>' is the half the matcher reads, or --on <event> the fact it waits for, and without either it would attach every turn",
    ));
  }

  if js::units(&condition) > CONDITION_CAP {
    return Err(Refused::of(format!(
      "the condition is {} characters; at most {CONDITION_CAP}",
      js::units(&condition)
    )));
  }

  if js::units(&action) > ACTION_CAP {
    return Err(Refused::of(format!(
      "the action is {} characters; at most {ACTION_CAP}",
      js::units(&action)
    )));
  }

  if global && asked.for_user.is_some() {
    return Err(Refused::of(
      "--for names the person a row is for, and a global row is read in rooms that person was never in; give it a --chat instead",
    ));
  }

  if asked.reach.is_some() && !global {
    return Err(Refused::of(
      "--reach says which kinds of room a global row reaches, and a room's own row reaches that room",
    ));
  }

  if let Some(reach) = asked.reach.as_deref().filter(|reach| !MEMORY_REACHES.contains(reach)) {
    return Err(Refused::of(format!(
      "--reach must be one of {}, not '{reach}'",
      MEMORY_REACHES.join(", ")
    )));
  }

  // A global row the owner sets in the owner's own room does not begin governing rooms full of other people
  // unless that is asked for, so the default is that room alone.
  let reach = asked.reach.clone().unwrap_or_else(|| "owner".to_owned());

  if global {
    let names = global_names(work.known, work.owner_id).await?;
    let mut violations: Vec<String> = vec![];

    for found in global_violations(&condition, &names)
      .into_iter()
      .chain(global_violations(&action, &names))
    {
      if !violations.contains(&found) {
        violations.push(found);
      }
    }

    if !violations.is_empty() {
      return Err(Refused::of(format!(
        "a global row is read in rooms it was not written in, so it names no room and nobody; this one names {}",
        violations.join(", ")
      )));
    }
  }

  let replaces = asked.replaces.as_deref().map(|replaces| js::trim(replaces).to_owned());
  let replaced = match &replaces {
    None => None,
    Some(replaces) => {
      let Some(replaced) = memory_of(db, &Handle::Slug(replaces.clone())).await? else {
        return Err(Refused::of(format!("--replaces names no row: there is no memory '{replaces}'")));
      };

      if replaced.chat_id != scope || replaced.agent != asked.agent {
        return Err(Refused::of(format!(
          "memory '{replaces}' belongs to another scope and this row cannot take its place"
        )));
      }

      if !replaced.active {
        return Err(Refused::of(format!(
          "memory '{replaces}' is already down, so nothing is left for this row to replace"
        )));
      }

      Some(replaced)
    }
  };

  if !slug.is_empty() && memory_of(db, &Handle::Slug(slug.clone())).await?.is_some() {
    return Err(Refused::of(format!("memory '{slug}' already stands; a slug names one row everywhere")));
  }

  let event_column = event.as_ref().map(event_text).unwrap_or_default();
  let transaction = db.begin().await?;
  let provisional = if slug.is_empty() {
    format!("unnamed-{:016X}", rand::rng().random::<u64>())
  } else {
    slug.clone()
  };
  let id: i64 = one(
    &transaction,
    r#"INSERT INTO "memories"
         ("agent", "slug", "type", "chat_id", "reach", "condition", "event", "action", "for_user", "message_id", "todo_id", "set_by", "set_at", "active", "last_hit_at", "hits")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 1, NULL, 0)
       RETURNING "id""#,
    vec![
      asked.agent.as_str().into(),
      provisional.into(),
      kind.into(),
      scope.into(),
      reach.as_str().into(),
      condition.as_str().into(),
      event_column.into(),
      action.as_str().into(),
      asked.for_user.into(),
      asked.message_id.into(),
      asked.todo_id.into(),
      asked.set_by.into(),
      now.into(),
    ],
  )
  .await?
  .ok_or_else(|| Refused::of("the memory was not filed"))?
  .try_get("", "id")?;
  let named = if slug.is_empty() { format!("memory-{id}") } else { slug.clone() };

  if slug.is_empty() {
    run(
      &transaction,
      r#"UPDATE "memories" SET "slug" = $1 WHERE "id" = $2"#,
      vec![named.as_str().into(), id.into()],
    )
    .await?;
  }

  if let Some(replaces) = &replaces {
    run(
      &transaction,
      r#"UPDATE "memories" SET "active" = 0, "down_reason" = $1, "down_at" = $2 WHERE "slug" = $3 AND "active" = 1"#,
      vec![format!("replaced by {named}").into(), now.into(), replaces.as_str().into()],
    )
    .await?;
  }

  transaction.commit().await?;

  let written = format!(
    "memory {id} written {} as '{named}'{}",
    match scope {
      None => format!("globally, reaching {reach} rooms"),
      Some(scope) => format!("for chat {scope}"),
    },
    replaces
      .as_ref()
      .map_or_else(String::new, |replaces| format!(", replacing '{replaces}'"))
  );
  let mut lines = vec![written];

  // The row it takes the place of is down now, and so is every watch that served it: ended before a watch of
  // the new row is set, so the same check carried over is not refused as already standing.
  if let Some(replaced) = &replaced {
    lines.extend(end_watches_of(db, replaced.id, now).await?);
  }

  let Some(watch_asked) = watch_asked else {
    return Ok(lines);
  };

  // The row and what wakes it stand together or not at all: a watch that could not be set takes its row down
  // again, saying why.
  let set = wakes::set_watch(
    db,
    &WatchAsked {
      memory_id: Some(id),
      ..watch_asked
    },
    work.launch,
    now,
  )
  .await?;

  match set {
    Err(refusal) => {
      take_down(db, &named, &format!("its watch could not be set: {refusal}"), now).await?;

      Err(Refused {
        lines,
        ..Refused::of(format!("the watch could not be set, so memory {id} was taken down again: {refusal}"))
      })
    }
    Ok(watch) => {
      lines.push(wakes::watch_set_line(&watch));

      Ok(lines)
    }
  }
}

/// An open state moved on: its text becomes where the thing now stands.
pub async fn update(
  db: &DatabaseConnection,
  asked: &Asked<'_>,
  action: Option<&str>,
  now: i64,
) -> Answered {
  let Some(scope) = asked.scope else {
    return Err(Refused::of(
      "--update moves a room's own state on, so it takes --chat rather than --global",
    ));
  };

  other_agents(db, asked.handle, asked.agent).await?;

  let handle = asked.handle;
  let action = js::trim(action.unwrap_or_default());

  if action.is_empty() {
    return Err(Refused::coded(
      "state-missing",
      vec![],
      "--action '<where it stands now>' is where the state now stands; give it, or --action - with the text on stdin",
    ));
  }

  if js::units(action) > ACTION_CAP {
    return Err(Refused::of(format!(
      "the action is {} characters; at most {ACTION_CAP}",
      js::units(action)
    )));
  }

  let Some(standing) = memory_of(db, handle).await? else {
    return Err(Refused::of(format!("there is no memory '{handle}'")));
  };

  if standing.chat_id != Some(scope) {
    return Err(Refused::of(format!(
      "memory '{handle}' is not this room's; a room moves only its own states"
    )));
  }

  if standing.kind != "state" {
    return Err(Refused::of(format!(
      "memory '{handle}' is a {} row, and only a state is rewritten as it moves; file a new row with --replaces {} instead",
      standing.kind, standing.slug
    )));
  }

  let moved = returned(
    db,
    r#"UPDATE "memories" SET "action" = $1, "moved_at" = $2 WHERE "id" = $3 AND "active" = 1 AND "done_at" IS NULL RETURNING *"#,
    vec![action.into(), now.into(), standing.id.into()],
  )
  .await?;

  match moved {
    Some(moved) => Ok(vec![format!("memory {} moved on for chat {scope}", moved.id)]),
    None => Err(Refused::of(format!(
      "memory '{handle}' was already released or taken down; a state taken back up is a new row, written from where that one stood"
    ))),
  }
}

/// An errand checked off, or a state released, in the room it belongs to; the row stays, stamped with when and
/// how it ended.
pub async fn done(db: &DatabaseConnection, asked: &Asked<'_>, text: Option<&str>, now: i64) -> Answered {
  let Some(scope) = asked.scope else {
    return Err(Refused::of(
      "--done checks off a room's own errand, so it takes --chat rather than --global",
    ));
  };

  other_agents(db, asked.handle, asked.agent).await?;

  let handle = asked.handle;
  let result = js::trim(text.unwrap_or_default());

  if js::units(result) > ACTION_CAP {
    return Err(Refused::of(format!(
      "the result is {} characters; at most {ACTION_CAP}",
      js::units(result)
    )));
  }

  let Some(standing) = memory_of(db, handle).await? else {
    return Err(Refused::of(format!("there is no memory '{handle}'")));
  };

  if standing.chat_id != Some(scope) {
    return Err(Refused::of(format!(
      "memory '{handle}' is not this room's; a room checks off only its own rows"
    )));
  }

  if standing.kind == "situation" {
    return Err(Refused::coded(
      "situation-unfinished",
      vec![("handle", handle.to_string()), ("slug", standing.slug.clone())],
      format!(
        "memory '{handle}' is a situation row, which is never finished: take it down with --retire {} --why '<reason>'",
        standing.slug
      ),
    ));
  }

  let transaction = db.begin().await?;
  let checked = returned(
    &transaction,
    r#"UPDATE "memories" SET "done_at" = $1, "done_text" = $2 WHERE "id" = $3 AND "chat_id" = $4 AND "done_at" IS NULL RETURNING *"#,
    vec![
      now.into(),
      (!result.is_empty()).then(|| result.to_owned()).into(),
      standing.id.into(),
      scope.into(),
    ],
  )
  .await?;

  transaction.commit().await?;

  let Some(checked) = checked else {
    return Err(Refused::of(format!(
      "memory '{handle}' was already checked off at {}",
      when(standing.done_at.unwrap_or_default())
    )));
  };
  let closing = if checked.kind == "state" { "released" } else { "checked off" };
  let mut lines = vec![format!("memory {} {closing} for chat {scope}", checked.id)];

  lines.extend(end_watches_of(db, checked.id, now).await?);

  Ok(lines)
}

/// A row taken down with its reason; the row stays whole.
pub async fn retire(db: &DatabaseConnection, asked: &Asked<'_>, why: Option<&str>, now: i64) -> Answered {
  let slug = slug_of(db, asked.handle).await?;

  other_agents(db, &Handle::Slug(slug.clone()), asked.agent).await?;

  let why = reason_of(why)?;
  let standing = in_scope(db, &slug, asked.scope).await?.map_err(|why| unchanged(asked.handle, why))?;

  if !standing.active {
    return Err(unchanged(asked.handle, Unchanged::AlreadyDown(standing.down_reason)));
  }

  let retired = take_down(db, &slug, &why, now)
    .await?
    .ok_or_else(|| unchanged(asked.handle, Unchanged::Missing))?;
  let mut lines = vec![format!(
    "memory '{}' retired; its row stands with what it was and why it came down",
    retired.slug
  )];

  lines.extend(end_watches_of(db, retired.id, now).await?);

  Ok(lines)
}

/// A row put back; what it was taken down for goes with the taking down.
pub async fn revive(db: &DatabaseConnection, asked: &Asked<'_>) -> Answered {
  let slug = slug_of(db, asked.handle).await?;

  other_agents(db, &Handle::Slug(slug.clone()), asked.agent).await?;

  let standing = in_scope(db, &slug, asked.scope).await?.map_err(|why| unchanged(asked.handle, why))?;

  if standing.active {
    return Err(unchanged(asked.handle, Unchanged::AlreadyUp));
  }

  let revived = returned(
    db,
    r#"UPDATE "memories" SET "active" = 1, "down_reason" = NULL, "down_at" = NULL WHERE "slug" = $1 RETURNING *"#,
    vec![slug.as_str().into()],
  )
  .await?
  .ok_or_else(|| unchanged(asked.handle, Unchanged::Missing))?;

  Ok(vec![format!(
    "memory '{}' put back; what it was taken down for goes with the taking down",
    revived.slug
  )])
}

/// A row raised as no longer useful, for a person to settle; it stays active and goes on attaching.
pub async fn candidate(
  db: &DatabaseConnection,
  asked: &Asked<'_>,
  why: Option<&str>,
  now: i64,
) -> Answered {
  let slug = slug_of(db, asked.handle).await?;

  other_agents(db, &Handle::Slug(slug.clone()), asked.agent).await?;

  let why = reason_of(why)?;

  in_scope(db, &slug, asked.scope).await?.map_err(|why| unchanged(asked.handle, why))?;

  let raised = returned(
    db,
    r#"UPDATE "memories" SET "candidate_why" = $1, "candidate_at" = $2 WHERE "slug" = $3 RETURNING *"#,
    vec![why.into(), now.into(), slug.as_str().into()],
  )
  .await?
  .ok_or_else(|| unchanged(asked.handle, Unchanged::Missing))?;

  Ok(vec![format!(
    "memory '{}' raised as a candidate; it stays active until a person takes it down on the console",
    raised.slug
  )])
}

/// The rows of a scope, open ones unless all are asked for, newest first, under the ceiling every answer
/// stands under.
pub async fn list(
  db: &DatabaseConnection,
  agent: &str,
  scope: Option<i64>,
  kind: Option<&str>,
  every: bool,
) -> Answered {
  if let Some(kind) = kind.filter(|kind| !MEMORY_TYPES.contains(kind)) {
    return Err(Refused::of(format!("--type must be one of {}, not '{kind}'", types_list())));
  }

  let mut terms = vec![r#""agent" = $1"#.to_owned()];
  let mut values: Vec<sea_orm::Value> = vec![agent.into()];

  if let Some(scope) = scope {
    values.push(scope.into());
    terms.push(format!(r#""chat_id" = ${}"#, values.len()));
  }

  if let Some(kind) = kind {
    values.push(kind.into());
    terms.push(format!(r#""type" = ${}"#, values.len()));
  }

  let rows = all(
    db,
    &format!(r#"SELECT * FROM "memories" WHERE {} ORDER BY "id" DESC"#, terms.join(" AND ")),
    values,
  )
  .await?;
  let mut memories = vec![];

  for row in &rows {
    let memory = memory_of_row(row)?;

    if (scope.is_some() || memory.chat_id.is_none())
      && (every || (memory.active && memory.done_at.is_none()))
    {
      memories.push(memory);
    }
  }

  if memories.is_empty() {
    let whose = scope.map_or_else(|| "there are no".to_owned(), |scope| format!("chat {scope} has no"));
    let open = if every { "" } else { "open " };
    let what = kind.map_or_else(|| "memories".to_owned(), |kind| format!("{kind} memories"));

    return Ok(vec![format!("{whose} {open}{what}")]);
  }

  let capped = cap_items(
    &memories,
    |kept| kept.iter().map(memory_line).collect::<Vec<_>>().join("\n"),
    Said::new("memories", "Narrow the question with --type."),
  );

  Ok(capped.kept.iter().map(memory_line).chain(capped.note).collect())
}

/// Whether a value given an argument that takes a whole number is one, said as the operation refuses it.
pub fn whole_number(name: &str, raw: Option<&str>) -> Result<Option<i64>, Refused> {
  let Some(raw) = raw else {
    return Ok(None);
  };

  if !WHOLE.is_match(js::trim(raw)) {
    return Err(Refused::of(format!("{name} takes a whole number, not '{raw}'")));
  }

  js::trim(raw)
    .parse()
    .map(Some)
    .map_err(|_| Refused::of(format!("{name} takes a whole number, not '{raw}'")))
}
