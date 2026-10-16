//! A watch set (docs/memories.md, 「Watches」): asked for, checked, written and its check handed to a run of
//! its own, by the watch route and by a memory row that sets one with it alike.

use brain_support::clock::{clock, epoch_of};
use brain_support::database::{Fault, all, one, run};
use brain_support::js;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use futures_util::future::BoxFuture;
use sea_orm::{DatabaseConnection, QueryResult};

use crate::memories::{Answered, Handle, Refused, memory_of};

/// The most characters a watch's name is shown with.
const MOST_NAME: usize = 40;

/// A standing watch, as much of it as a memo tells of.
#[derive(Clone, Debug)]
pub struct Watch {
  pub id: i64,
  pub chat_id: i64,
  pub by_chat: i64,
  pub name: Option<String>,
  pub note: String,
  pub memory_id: Option<i64>,
  pub created_at: i64,
}

/// A note or a name brought to one line of at most `limit` characters, counted as the console counts
/// them, cut with an ellipsis.
pub fn one_line(text: &str, limit: usize) -> String {
  let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
  let units: Vec<u16> = line.encode_utf16().collect();

  if units.len() > limit {
    format!("{}…", String::from_utf16_lossy(&units[..limit - 1]))
  } else {
    line
  }
}

/// The short name a watch is shown by: the one it was given, or the head of its note where it was
/// given none.
pub fn watch_name(watch: &Watch) -> String {
  let given = watch.name.as_deref().map(str::trim).unwrap_or_default();
  let shown = if given.is_empty() {
    watch.note.split('\n').find(|line| !line.trim().is_empty()).unwrap_or_default()
  } else {
    given
  };

  one_line(shown, MOST_NAME)
}

/// The longest a watch's note is shown on its line.
pub const MAX_NOTE_LENGTH: usize = 1_000;

/// A moment as a session writes one: `YYYY-MM-DD HH:MM` on the Seoul clock, or an ISO moment — with its
/// offset, a bare day read as UTC's midnight, or a day and time with no offset read on this machine's
/// clock.
pub fn seoul_when(when: &str) -> Option<i64> {
  let when = when.trim();

  if let Ok(wall) = NaiveDateTime::parse_from_str(when, "%Y-%m-%d %H:%M") {
    return Some(epoch_of(wall));
  }

  if let Ok(moment) = DateTime::parse_from_rfc3339(when) {
    return Some(moment.timestamp());
  }

  if let Ok(day) = NaiveDate::parse_from_str(when, "%Y-%m-%d") {
    return Some(day.and_hms_opt(0, 0, 0)?.and_utc().timestamp());
  }

  ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"]
    .iter()
    .find_map(|form| NaiveDateTime::parse_from_str(when, form).ok())
    .and_then(|wall| Local.from_local_datetime(&wall).earliest())
    .map(|moment| moment.timestamp())
}

// The bounds a watch is set within.
const MIN_INTERVAL_SECONDS: i64 = 15;
const DEFAULT_INTERVAL_SECONDS: i64 = 60;
const MAX_INTERVAL_SECONDS: i64 = 30 * 60;
const DEFAULT_WITHIN_MINUTES: i64 = 120;
const MAX_WITHIN_MINUTES: i64 = 24 * 60;
const MAX_TIMER_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_STANDING_PER_CHAT: usize = 10;
const MAX_CONDITION_LENGTH: usize = 2_000;
const MAX_NAME_LENGTH: usize = 40;

/// What a watch is set with, as the refusal of a call it cannot read says.
pub const WATCH_USAGE: &str = "watch_set takes note, what you are waiting for and what you promised, to \
                               whom, and exactly one of until, a short read-only check, or at, \
                               'YYYY-MM-DD HH:MM' for a timer only when no check can say it; and optionally \
                               name, a short name; every, in seconds, and within, in minutes, beside a check; \
                               item, the memory it serves; chat and message";

/// A text with its white space closed up: trimmed, and every run inside it one space.
pub fn collapse(text: &str) -> String {
  text.split(js::is_space).filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Whether bash can read a check at all. It parses without running anything.
pub fn condition_parses(condition: &str) -> bool {
  std::process::Command::new("bash")
    .args(["-n", "-c", condition])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .is_ok_and(|status| status.success())
}

/// What a watch is asked for, as its arguments give it.
pub struct WatchArguments<'a> {
  /// The agent setting it, which its end wakes.
  pub agent: &'a str,
  pub chat: Option<&'a str>,
  pub by: Option<&'a str>,
  pub until: Option<&'a str>,
  pub at: Option<&'a str>,
  /// The short name it is shown by; one set without is shown by the head of its note.
  pub name: Option<&'a str>,
  pub note: Option<&'a str>,
  pub every: Option<&'a str>,
  pub within: Option<&'a str>,
  /// The memory it serves.
  pub item: Option<&'a str>,
}

/// A watch as it is to be written: a check run every interval until its deadline, or a timer.
#[derive(Clone, Debug, PartialEq)]
pub struct WatchAsked {
  pub agent: String,
  pub chat_id: i64,
  pub by_chat: i64,
  pub timer: bool,
  pub condition: Option<String>,
  pub name: Option<String>,
  pub note: String,
  pub dedupe_key: String,
  pub interval_seconds: Option<i64>,
  pub due_at: Option<i64>,
  pub deadline_at: i64,
  pub memory_id: Option<i64>,
}

/// A watch that stands, as much of it as its line says.
#[derive(Clone, Debug)]
pub struct SetWatch {
  pub watch: Watch,
  pub timer: bool,
  pub interval_seconds: Option<i64>,
  pub deadline_at: i64,
}

/// One watch's row, as much of it as a memo and a line tell of.
pub fn watch_row(row: &QueryResult) -> Result<Watch, Fault> {
  Ok(Watch {
    id: row.try_get("", "id")?,
    chat_id: row.try_get("", "chat_id")?,
    by_chat: row.try_get("", "by_chat")?,
    name: row.try_get("", "name")?,
    note: row.try_get("", "note")?,
    memory_id: row.try_get("", "memory_id")?,
    created_at: row.try_get("", "created_at")?,
  })
}

fn whole(text: &str) -> bool {
  let digits = text.strip_prefix('-').unwrap_or(text);

  !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// What a watch is asked for, checked before a row or a run exists; every refusal says what was wrong in
/// words a session can act on. `now` is in unix seconds.
pub fn watch_request(
  args: &WatchArguments<'_>,
  now: i64,
  parses: &dyn Fn(&str) -> bool,
) -> Result<WatchAsked, String> {
  let trimmed = |text: Option<&str>| js::trim(text.unwrap_or_default()).to_owned();
  let Some(by_chat) = args.by.and_then(by_chat_strict) else {
    return Err("a watch has to say whose it is: --by chat:<your own chatId>.".to_owned());
  };
  let chat = trimmed(args.chat);
  let Some(chat_id) = whole(&chat).then(|| chat.parse::<i64>().ok()).flatten() else {
    return Err("--chat has to be the room's chat id.".to_owned());
  };
  let note = trimmed(args.note);

  if note.is_empty() {
    return Err(
      "a watch needs --note: what you are waiting for and what you promised, to whom.".to_owned(),
    );
  }

  if js::units(&note) > MAX_NOTE_LENGTH {
    return Err(format!("--note is at most {MAX_NOTE_LENGTH} characters."));
  }

  let name = Some(trimmed(args.name)).filter(|given| !given.is_empty()).map(|given| collapse(&given));

  if name.as_deref().is_some_and(|name| js::units(name) > MAX_NAME_LENGTH) {
    return Err(format!(
      "--name is at most {MAX_NAME_LENGTH} characters: a few words a line can show, with the whole of it in --note."
    ));
  }

  let memory_id = match args.item {
    None => None,
    Some(item) => {
      let item = js::trim(item);

      if item.is_empty() || !item.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("--item has to be a memory's id.".to_owned());
      }

      Some(item.parse::<i64>().map_err(|_| "--item has to be a memory's id.".to_owned())?)
    }
  };

  if args.until.is_none() == args.at.is_none() {
    return Err(
      "give exactly one of --until '<check>' (the watch) or --at '<time>' (the timer, only when no check can say it)."
        .to_owned(),
    );
  }

  if let Some(at) = args.at {
    if args.every.is_some() || args.within.is_some() {
      return Err("--every and --within belong to a check; a timer takes --at alone.".to_owned());
    }

    let Some(due_at) = seoul_when(at) else {
      return Err(format!("cannot parse time: {at}"));
    };

    if due_at <= now {
      return Err("refusing a time in the past.".to_owned());
    }

    if due_at - now > MAX_TIMER_SECONDS {
      return Err("a timer is at most a week out.".to_owned());
    }

    return Ok(WatchAsked {
      agent: args.agent.to_owned(),
      chat_id,
      by_chat,
      timer: true,
      condition: None,
      name,
      dedupe_key: format!("at:{due_at}:{}", collapse(&note)),
      note,
      interval_seconds: None,
      due_at: Some(due_at),
      deadline_at: due_at,
      memory_id,
    });
  }

  let condition = trimmed(args.until);

  if condition.is_empty() {
    return Err(
      "--until needs a check: a short read-only shell command that exits 0 once the thing is so."
        .to_owned(),
    );
  }

  if js::units(&condition) > MAX_CONDITION_LENGTH {
    return Err(format!("--until is at most {MAX_CONDITION_LENGTH} characters."));
  }

  let whole_of = |given: Option<&str>, default: i64| match given {
    None => Some(default),
    Some(given) => js::number(given).filter(|value| value.fract() == 0.0).map(|value| value as i64),
  };
  let every = whole_of(args.every, DEFAULT_INTERVAL_SECONDS)
    .filter(|every| (MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(every));
  let Some(every) = every else {
    return Err(format!(
      "--every is whole seconds, from {MIN_INTERVAL_SECONDS} to {MAX_INTERVAL_SECONDS}."
    ));
  };
  let within = whole_of(args.within, DEFAULT_WITHIN_MINUTES)
    .filter(|within| (1..=MAX_WITHIN_MINUTES).contains(within));
  let Some(within) = within else {
    return Err(format!("--within is whole minutes, from 1 to {MAX_WITHIN_MINUTES}."));
  };

  if !parses(&condition) {
    return Err("bash cannot parse the check given to --until.".to_owned());
  }

  Ok(WatchAsked {
    agent: args.agent.to_owned(),
    chat_id,
    by_chat,
    timer: false,
    dedupe_key: collapse(&condition),
    condition: Some(condition),
    name,
    note,
    interval_seconds: Some(every),
    due_at: None,
    deadline_at: now + within * 60,
    memory_id,
  })
}

// The chat a watch's setter is named by, read as the launcher reads it: `chat:` and a whole number, nothing
// around them but white space.
fn by_chat_strict(given: &str) -> Option<i64> {
  let number = js::trim(given).strip_prefix("chat:")?;

  whole(number).then(|| number.parse().ok()).flatten()
}

/// How a watch's check is put in a run of its own: whether the run came up.
pub trait LaunchWatch: Sync {
  fn launch(&self, id: i64) -> BoxFuture<'_, Result<bool, Fault>>;
}

// Why a memory may not be served by a watch set for a room, or none where it may. A row of another room is
// refused without a word about what it holds.
async fn item_refusal(
  db: &DatabaseConnection,
  id: i64,
  chat_id: i64,
  agent: &str,
) -> Result<Option<String>, Fault> {
  let item = memory_of(db, &Handle::Id(id)).await?;

  Ok(match item {
    None => Some(format!("there is no memory {id}.")),
    Some(item) if item.agent != agent => {
      Some(format!("memory {id} is another agent's; a watch serves only a row of its own agent's."))
    }
    Some(item) if item.chat_id != Some(chat_id) => {
      Some(format!("memory {id} is not this room's; a watch serves only a row of its own --chat."))
    }
    Some(item) if item.done_at.is_some() => Some(format!(
      "memory {id} is already checked off; there is nothing left for a watch to serve."
    )),
    Some(item) if !item.active => {
      Some(format!("memory {id} is down; there is nothing left for a watch to serve."))
    }
    Some(_) => None,
  })
}

async fn standing_duplicate(db: &DatabaseConnection, asked: &WatchAsked) -> Result<Option<i64>, Fault> {
  Ok(
    one(
      db,
      r#"SELECT "id" FROM "watches" WHERE "by_chat" = $1 AND "dedupe_key" = $2 AND "state" = 'active'"#,
      vec![asked.by_chat.into(), asked.dedupe_key.as_str().into()],
    )
    .await?
    .map(|row| row.try_get("", "id"))
    .transpose()?,
  )
}

/// One watch set: the memory it serves checked, the chat's allowance held to, the row written and a check
/// handed to a run of its own. A check that could not be launched ends its row, so nothing stands that nobody
/// watches. A watch set alone and one a memory sets with its row are both set here.
pub async fn set_watch(
  db: &DatabaseConnection,
  asked: &WatchAsked,
  launch: &dyn LaunchWatch,
  now: i64,
) -> Result<Result<SetWatch, String>, Fault> {
  if let Some(served) = asked.memory_id
    && let Some(refusal) = item_refusal(db, served, asked.chat_id, &asked.agent).await?
  {
    return Ok(Err(refusal));
  }

  let standing = all(
    db,
    r#"SELECT "id" FROM "watches" WHERE "by_chat" = $1 AND "state" = 'active'"#,
    vec![asked.by_chat.into()],
  )
  .await?;

  if standing.len() >= MAX_STANDING_PER_CHAT {
    return Ok(Err(format!(
      "{MAX_STANDING_PER_CHAT} watches already stand for chat {}; cancel one that no longer matters first.",
      asked.by_chat
    )));
  }

  if let Some(id) = standing_duplicate(db, asked).await? {
    return Ok(Err(format!("already watching this as #{id}")));
  }

  let written = one(
    db,
    r#"INSERT INTO "watches" ("agent", "chat_id", "by_chat", "kind", "condition", "name", "note", "dedupe_key", "interval_seconds", "due_at", "deadline_at", "state", "created_at", "memory_id")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'active', $12, $13)
       RETURNING *"#,
    vec![
      asked.agent.as_str().into(),
      asked.chat_id.into(),
      asked.by_chat.into(),
      if asked.timer { "timer" } else { "condition" }.into(),
      asked.condition.clone().into(),
      asked.name.clone().into(),
      asked.note.as_str().into(),
      asked.dedupe_key.as_str().into(),
      asked.interval_seconds.into(),
      asked.due_at.into(),
      asked.deadline_at.into(),
      now.into(),
      asked.memory_id.into(),
    ],
  )
  .await;
  let row = match written {
    Ok(Some(row)) => row,
    Ok(None) => return Err(Fault::new("the watch was not written")),
    Err(refusal) => {
      return match standing_duplicate(db, asked).await? {
        Some(id) => Ok(Err(format!("already watching this as #{id}"))),
        None => Err(refusal),
      };
    }
  };
  let watch = watch_row(&row)?;
  let id = watch.id;

  if !asked.timer && !launch.launch(id).await? {
    run(
      db,
      r#"UPDATE "watches" SET "state" = 'cancelled', "ended_at" = $1 WHERE "id" = $2 AND "state" = 'active'"#,
      vec![now.into(), id.into()],
    )
    .await?;

    return Ok(Err(format!("#{id} could not be launched, so nothing is watching it")));
  }

  Ok(Ok(SetWatch {
    watch,
    timer: asked.timer,
    interval_seconds: asked.interval_seconds,
    deadline_at: asked.deadline_at,
  }))
}

/// What a watch that stands is said as, by whoever set it.
pub fn watch_set_line(set: &SetWatch) -> String {
  let serving = set
    .watch
    .memory_id
    .map_or_else(String::new, |memory| format!(", serving memory #{memory}"));
  let name = watch_name(&set.watch);

  if set.timer {
    format!("watch #{} ({name}): a timer for {}{serving}", set.watch.id, clock(set.deadline_at))
  } else {
    format!(
      "watch #{} ({name}): checking every {} seconds until {}{serving}",
      set.watch.id,
      set.interval_seconds.unwrap_or_default(),
      clock(set.deadline_at)
    )
  }
}

/// A watch set alone: asked for and stood up. A call that could not be read is refused `usage`, and one the
/// watches would not take `memory-refused`. `now` is in unix seconds.
pub async fn watch(
  db: &DatabaseConnection,
  args: &WatchArguments<'_>,
  launch: &dyn LaunchWatch,
  parses: &(dyn Fn(&str) -> bool + Sync),
  now: i64,
) -> Result<Answered, Fault> {
  let asked = match watch_request(args, now, parses) {
    Ok(asked) => asked,
    Err(reason) => return Ok(Err(Refused::coded("usage", vec![("verb", "watch_set".to_owned())], reason))),
  };

  Ok(match set_watch(db, &asked, launch, now).await? {
    Ok(set) => Ok(vec![watch_set_line(&set)]),
    Err(reason) => Err(Refused::of(reason)),
  })
}
