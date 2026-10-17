//! The reminders an agent sets itself (docs/memories.md, 「Watches」; docs/boundary-api.md, 「Watches and
//! reminders」): a reminder set once or on a repeat rule of the Seoul clock, serving an open memory of its own
//! agent's in its own room where it names one; a room's reminders listed; one called off; and every reminder whose
//! moment has come woken in its room as a quiet instruction to the agent that set it, a repeating one moved to its
//! first moment still ahead, so moments missed while the brain was down come to one wake.

use brain_support::clock::{OFFSET_SECONDS, clock, epoch_of};
use brain_support::database::{all, one, run};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime};
use memory_rows::watches::seoul_when;
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::Serialize;

use crate::Failure;
use crate::editors::archive::room_name;
use crate::ports::agency::{Agency, QuietInstruction};
use crate::ports::receiver::Receiver;
use crate::watches::{Item, item_of};

/// How far past its moment a wake goes before it is told it comes late: a few ticks, so a wake delayed only by
/// the tick itself is not.
pub const LATE_AFTER_SECONDS: i64 = 5 * 60;

/// The longest reason a reminder keeps; the rest belongs in a memory it names.
pub const MAX_REASON_LENGTH: usize = 1_000;

/// The kind of quiet instruction a reminder's wake goes as.
pub const WAKE_KIND: &str = "reminder";

/// What the set route takes, as its refusal of a call missing any of it says.
pub const REMIND_USAGE: &str = "a reminder is set with chatId, at (an ISO moment or 'YYYY-MM-DD HH:MM' on the Seoul clock) or every ('daily HH:MM', 'weekly mon HH:MM' or 'monthly 1 HH:MM'), reason (why it wakes the agent), and optionally memory (an id or a slug) and user";

const DAY_SECONDS: i64 = 86_400;

const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// A moment on the Seoul wall clock.
fn wall(epoch_seconds: i64) -> NaiveDateTime {
  DateTime::from_timestamp(epoch_seconds + OFFSET_SECONDS, 0)
    .unwrap_or_default()
    .naive_utc()
}

/// A repeat rule on the Seoul clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Every {
  Daily { hour: u32, minute: u32 },
  Weekly { weekday: u32, hour: u32, minute: u32 },
  Monthly { day: u32, hour: u32, minute: u32 },
}

/// The rule as a reminder keeps it: `daily HH:MM`, `weekly <mon…sun> HH:MM` or `monthly <day> HH:MM`.
impl std::fmt::Display for Every {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let (hour, minute) = self.clock();

    match *self {
      Self::Daily { .. } => write!(out, "daily {hour:02}:{minute:02}"),
      Self::Weekly { weekday, .. } => {
        write!(out, "weekly {} {hour:02}:{minute:02}", WEEKDAYS[weekday as usize])
      }
      Self::Monthly { day, .. } => write!(out, "monthly {day} {hour:02}:{minute:02}"),
    }
  }
}

impl Every {
  fn clock(&self) -> (u32, u32) {
    match *self {
      Self::Daily { hour, minute }
      | Self::Weekly { hour, minute, .. }
      | Self::Monthly { hour, minute, .. } => (hour, minute),
    }
  }
}

fn digits(text: &str, most: usize) -> Option<u32> {
  (!text.is_empty() && text.len() <= most && text.bytes().all(|byte| byte.is_ascii_digit()))
    .then(|| text.parse().ok())
    .flatten()
}

/// The rule a reminder is set with, `daily HH:MM`, `weekly <mon…sun> HH:MM` or `monthly <1-31> HH:MM`, this
/// repository's own small grammar; none where it is none of them.
pub fn parse_every(given: &str) -> Option<Every> {
  let lowered = given.trim().to_lowercase();
  let words: Vec<&str> = lowered.split_whitespace().collect();
  let (hour, minute) = words.last()?.split_once(':')?;
  let hour = digits(hour, 2)?;
  let minute = (minute.len() == 2).then(|| digits(minute, 2)).flatten()?;

  if hour > 23 || minute > 59 {
    return None;
  }

  match words.as_slice() {
    ["daily", _] => Some(Every::Daily { hour, minute }),
    ["weekly", day, _] => {
      let weekday = WEEKDAYS.iter().position(|name| day.starts_with(name))?;

      Some(Every::Weekly {
        weekday: u32::try_from(weekday).ok()?,
        hour,
        minute,
      })
    }
    ["monthly", day, _] => {
      let day = digits(day, 2)?;

      (1..=31).contains(&day).then_some(Every::Monthly { day, hour, minute })
    }
    _ => None,
  }
}

fn days_in_month(year: i32, month: u32) -> u32 {
  let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };

  NaiveDate::from_ymd_opt(next_year, next_month, 1)
    .and_then(|first| first.pred_opt())
    .map_or(28, |last| last.day())
}

fn at(date: NaiveDate, (hour, minute): (u32, u32)) -> i64 {
  epoch_of(date.and_hms_opt(hour, minute, 0).unwrap_or_default())
}

/// The first moment of the rule strictly after `after`, in epoch seconds. A monthly day a month lacks falls on that
/// month's last day.
pub fn next_moment(every: Every, after: i64) -> i64 {
  let today = wall(after).date();
  let clock = every.clock();

  match every {
    Every::Daily { .. } => {
      let moment = at(today, clock);

      if moment > after { moment } else { moment + DAY_SECONDS }
    }
    Every::Weekly { weekday, .. } => {
      let ahead = (weekday + 7 - today.weekday().num_days_from_sunday()) % 7;
      let moment = at(today + Duration::days(i64::from(ahead)), clock);

      if moment > after { moment } else { moment + 7 * DAY_SECONDS }
    }
    Every::Monthly { day, .. } => {
      let (mut year, mut month) = (today.year(), today.month());

      loop {
        let date =
          NaiveDate::from_ymd_opt(year, month, day.min(days_in_month(year, month))).unwrap_or(today);
        let moment = at(date, clock);

        if moment > after {
          return moment;
        }

        (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
      }
    }
  }
}

/// One reminder as a line: its number, its next moment and its rule.
pub fn reminder_line(id: i64, due_at: i64, every: Option<&str>) -> String {
  let rule = every.map_or_else(String::new, |every| format!("every {every}, next "));

  format!("reminder #{id}: {rule}{}", clock(due_at))
}

/// Why a reminder was not set: the usage, or another refusal with its sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotSet {
  Usage,
  Refused(String),
}

/// What a reminder is set with.
#[derive(Clone, Copy, Debug)]
pub struct RemindAsked<'a> {
  /// The agent setting it, which its moment wakes.
  pub agent: &'a str,
  pub chat_id: i64,
  pub at: Option<&'a str>,
  pub every: Option<&'a str>,
  pub reason: &'a str,
  /// The memory it serves, by id or slug.
  pub memory: Option<&'a str>,
  pub user_id: Option<i64>,
}

// The memory a reminder is to serve, by id or slug, refused where it is not an open row of the same agent's in the
// same room; a row of another room is refused without a word about what it holds.
async fn served_memory(
  db: &DatabaseConnection,
  handle: &str,
  asked: &RemindAsked<'_>,
) -> Result<Result<i64, String>, Failure> {
  let by_id = handle.strip_prefix('-').unwrap_or(handle).bytes().all(|byte| byte.is_ascii_digit());
  let row = if by_id {
    let Ok(id) = handle.parse::<i64>() else {
      return Ok(Err(format!("there is no memory {handle}.")));
    };

    one(
      db,
      r#"SELECT "id", "agent", "chat_id", "active", "done_at" FROM "memories" WHERE "id" = $1"#,
      vec![id.into()],
    )
    .await?
  } else {
    one(
      db,
      r#"SELECT "id", "agent", "chat_id", "active", "done_at" FROM "memories" WHERE "slug" = $1"#,
      vec![handle.into()],
    )
    .await?
  };
  let Some(row) = row else {
    return Ok(Err(format!("there is no memory {handle}.")));
  };

  if row.try_get::<String>("", "agent")? != asked.agent {
    return Ok(Err(format!(
      "memory {handle} is another agent's; a reminder serves only a row of its own agent's."
    )));
  }

  if row.try_get::<Option<i64>>("", "chat_id")? != Some(asked.chat_id) {
    return Ok(Err(format!(
      "memory {handle} is not this room's; a reminder serves only a row of its own room."
    )));
  }

  if row.try_get::<Option<i64>>("", "done_at")?.is_some() || row.try_get::<i64>("", "active")? == 0 {
    return Ok(Err(format!(
      "memory {handle} is no longer open; there is nothing left for a reminder to serve."
    )));
  }

  Ok(Ok(row.try_get("", "id")?))
}

/// A reminder set: a row and nothing else, until its moment comes on the brain's clock. `now` is in epoch seconds.
pub async fn remind(
  db: &DatabaseConnection,
  asked: &RemindAsked<'_>,
  now: i64,
) -> Result<Result<Vec<String>, NotSet>, Failure> {
  let at = asked.at.map(str::trim).unwrap_or_default();
  let given = asked.every.map(str::trim).unwrap_or_default();
  let reason = asked.reason.trim();

  if reason.is_empty() || at.is_empty() == given.is_empty() {
    return Ok(Err(NotSet::Usage));
  }

  if reason.encode_utf16().count() > MAX_REASON_LENGTH {
    return Ok(Err(NotSet::Refused(format!(
      "a reason is at most {MAX_REASON_LENGTH} characters; keep the rest in a memory and name it"
    ))));
  }

  let memory_id = match asked.memory.map(str::trim).filter(|handle| !handle.is_empty()) {
    Some(handle) => match served_memory(db, handle, asked).await? {
      Ok(id) => Some(id),
      Err(why) => return Ok(Err(NotSet::Refused(why))),
    },
    None => None,
  };
  let (due_at, every) = if given.is_empty() {
    let Some(moment) = seoul_when(at) else {
      return Ok(Err(NotSet::Refused(format!("cannot parse time: {at}"))));
    };

    if moment <= now {
      return Ok(Err(NotSet::Refused("refusing a time in the past".to_owned())));
    }

    (moment, None)
  } else {
    let Some(rule) = parse_every(given) else {
      return Ok(Err(NotSet::Refused(format!(
        "cannot read the repeat rule: {given}; it is 'daily HH:MM', 'weekly mon HH:MM' (mon to sun) or 'monthly 1 HH:MM'"
      ))));
    };

    (next_moment(rule, now), Some(rule.to_string()))
  };
  let row = one(
    db,
    r#"INSERT INTO "reminders" ("agent", "chat_id", "user_id", "due_at", "reason", "memory_id", "every", "fired", "created_at")
       VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8) RETURNING "id""#,
    vec![
      asked.agent.into(),
      asked.chat_id.into(),
      asked.user_id.into(),
      due_at.into(),
      reason.into(),
      memory_id.into(),
      every.clone().into(),
      now.into(),
    ],
  )
  .await?
  .ok_or_else(|| Failure::Refused("the reminder's row".to_owned()))?;
  let id: i64 = row.try_get("", "id")?;

  Ok(Ok(vec![reminder_line(id, due_at, every.as_deref())]))
}

/// One reminder called off, a repeating one with every moment after it: whether there was one.
pub async fn cancel_reminder(db: &DatabaseConnection, id: i64) -> Result<bool, Failure> {
  Ok(
    one(db, r#"DELETE FROM "reminders" WHERE "id" = $1 RETURNING "id""#, vec![id.into()])
      .await?
      .is_some(),
  )
}

/// One reminder as the list route answers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
  pub id: i64,
  pub agent: String,
  pub chat_id: i64,
  pub user: Option<i64>,
  pub due_at: i64,
  pub every: Option<String>,
  pub reason: Option<String>,
  pub item: Option<i64>,
  pub fired: bool,
}

impl Listed {
  /// The reminder as a session reads it back: its line, whom it is for, the memory it serves and why it wakes
  /// the agent.
  pub fn line(&self) -> String {
    let fired = if self.fired { " (fired)" } else { "" };
    let who = self.user.map_or_else(String::new, |user| format!(" for {user}"));
    let memory = self.item.map_or_else(String::new, |memory| format!(" [memory #{memory}]"));

    format!(
      "{}{fired}{who}{memory}: {}",
      reminder_line(self.id, self.due_at, self.every.as_deref()),
      self.reason.as_deref().unwrap_or_default()
    )
  }
}

/// A room's reminders, soonest first: those still to come, or with `all` every one it has had; one agent's where
/// an agent is named.
pub async fn reminders(
  db: &DatabaseConnection,
  chat_id: i64,
  agent: Option<&str>,
  all_of_them: bool,
) -> Result<Vec<Listed>, Failure> {
  let mut filter = r#""chat_id" = $1"#.to_owned();
  let mut values: Vec<sea_orm::Value> = vec![chat_id.into()];

  if let Some(agent) = agent {
    values.push(agent.into());
    filter.push_str(r#" AND "agent" = $2"#);
  }

  if !all_of_them {
    filter.push_str(r#" AND "fired" = 0"#);
  }

  let rows = all(
    db,
    &format!(
      r#"SELECT "id", "agent", "chat_id", "user_id", "due_at", "reason", "memory_id", "every", "fired" FROM "reminders"
         WHERE {filter} ORDER BY "due_at" ASC"#
    ),
    values,
  )
  .await?;

  rows
    .iter()
    .map(|row| {
      Ok(Listed {
        id: row.try_get("", "id")?,
        agent: row.try_get("", "agent")?,
        chat_id: row.try_get("", "chat_id")?,
        user: row.try_get("", "user_id")?,
        due_at: row.try_get("", "due_at")?,
        every: row.try_get("", "every")?,
        reason: row.try_get("", "reason")?,
        item: row.try_get("", "memory_id")?,
        fired: row.try_get::<i64>("", "fired")? != 0,
      })
    })
    .collect()
}

/// A reminder as its moment wakes it.
#[derive(Clone, Debug)]
pub struct Reminder {
  pub id: i64,
  pub agent: String,
  pub chat_id: i64,
  pub user_id: Option<i64>,
  pub due_at: i64,
  pub reason: String,
  pub memory_id: Option<i64>,
  pub every: Option<String>,
  pub created_at: i64,
}

// What the wake says about the memory the reminder serves, as it stands at the moment.
fn memory_block(reminder: &Reminder, memory: Option<&Item>) -> Vec<String> {
  let Some(memory_id) = reminder.memory_id else {
    return vec![];
  };
  let Some(memory) = memory else {
    return vec![
      format!("It was set for memory #{memory_id}, which can no longer be found."),
      String::new(),
    ];
  };

  if let Some(done_at) = memory.done_at {
    let said = memory.done_text.as_ref().map_or_else(String::new, |text| format!(": {text}"));

    return vec![
      format!(
        "It serves memory #{} ({}), which you checked off at {}{said}.",
        memory.id,
        memory.slug,
        clock(done_at)
      ),
      String::new(),
    ];
  }

  if !memory.active {
    let why = memory.down_reason.as_ref().map_or_else(String::new, |reason| format!(": {reason}"));

    return vec![
      format!("It serves memory #{} ({}), which has been taken down{why}.", memory.id, memory.slug),
      String::new(),
    ];
  }

  let mut block = vec![format!(
    "It serves your memory #{} ({}, {}), as it stands now:",
    memory.id, memory.slug, memory.kind
  )];

  if !memory.condition.trim().is_empty() {
    block.push(format!("When: {}", memory.condition));
  }

  block.extend([memory.action.clone(), "END OF MEMORY".to_owned(), String::new()]);

  block
}

/// The quiet instruction a reminder's moment puts into its room's session.
pub fn wake(reminder: &Reminder, room: &str, memory: Option<&Item>, now: i64, next: Option<i64>) -> String {
  let repeating = reminder.every.as_ref().map_or_else(String::new, |every| format!(", repeating {every}"));
  let mut lines = vec![
    "A reminder you set has come due.".to_owned(),
    format!(
      "You set reminder #{} at {} in {room} (chat {}){repeating}. It was due at {}.",
      reminder.id,
      clock(reminder.created_at),
      reminder.chat_id,
      clock(reminder.due_at)
    ),
  ];

  if now - reminder.due_at > LATE_AFTER_SECONDS {
    let lateness = (now - reminder.due_at + 30) / 60;
    let meanwhile = if reminder.every.is_some() {
      " and stands for every time it came due meanwhile"
    } else {
      ""
    };

    lines.push(format!(
      "The bot was down or held up at that moment, so this comes {lateness} minutes late{meanwhile}. Judge whether it is still worth saying now, and say that it is late where that matters."
    ));
  }

  if let Some(user) = reminder.user_id {
    lines.push(format!("It is for user {user}."));
  }

  lines.extend([
    String::new(),
    "WHY YOU SET IT, IN YOUR OWN WORDS:".to_owned(),
    reminder.reason.clone(),
    "END OF REASON".to_owned(),
    String::new(),
  ]);
  lines.extend(memory_block(reminder, memory));
  lines.push("Do now what the reason asks, in this room. Look at how things stand — the memory as it is now and the room's own recent talk — and write it fresh, in this room's own manner, rather than repeating words planned in advance: your answer to this turn is what the room hears. Where nothing needs saying any more, output exactly <<SKIP>>.".to_owned());

  if let Some(next) = next {
    lines.extend([
      String::new(),
      format!(
        "It comes again at {}. Once it is no longer wanted, cancel reminder #{}.",
        clock(next),
        reminder.id
      ),
    ]);
  }

  lines.join("\n")
}

// Every reminder whose moment has come, oldest first.
async fn due(db: &DatabaseConnection, now: i64) -> Result<Vec<Reminder>, Failure> {
  all(
    db,
    r#"SELECT "id", "agent", "chat_id", "user_id", "due_at", "reason", "memory_id", "every", "created_at"
       FROM "reminders" WHERE "fired" = 0 AND "due_at" <= $1 ORDER BY "due_at" ASC"#,
    vec![now.into()],
  )
  .await?
  .into_iter()
  .map(|row| {
    Ok(Reminder {
      id: row.try_get("", "id")?,
      agent: row.try_get("", "agent")?,
      chat_id: row.try_get("", "chat_id")?,
      user_id: row.try_get("", "user_id")?,
      due_at: row.try_get("", "due_at")?,
      reason: row.try_get::<Option<String>>("", "reason")?.unwrap_or_default(),
      memory_id: row.try_get("", "memory_id")?,
      every: row.try_get("", "every")?,
      created_at: row.try_get("", "created_at")?,
    })
  })
  .collect()
}

// One moment of a reminder taken: the row done with that moment, fired for good or moved to `next` for a repeating
// one, and its wake handed to the agency, both or neither. False where the row no longer stands at the moment it was
// read at.
async fn fire(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  reminder: &Reminder,
  wake: &str,
  next: Option<i64>,
) -> Result<bool, Failure> {
  let transaction = db.begin().await?;
  let changed = match next {
    None => {
      run(
        &transaction,
        r#"UPDATE "reminders" SET "fired" = 1 WHERE "id" = $1 AND "fired" = 0 AND "due_at" = $2"#,
        vec![reminder.id.into(), reminder.due_at.into()],
      )
      .await?
    }
    Some(next) => {
      run(
        &transaction,
        r#"UPDATE "reminders" SET "due_at" = $1 WHERE "id" = $2 AND "fired" = 0 AND "due_at" = $3"#,
        vec![next.into(), reminder.id.into(), reminder.due_at.into()],
      )
      .await?
    }
  };

  if changed != 1 {
    transaction.rollback().await?;

    return Ok(false);
  }

  let instruction = QuietInstruction {
    agent: reminder.agent.clone(),
    chat_id: reminder.chat_id,
    text: wake.to_owned(),
    kind: Some(WAKE_KIND.to_owned()),
  };

  match agency.quiet_instruction(&instruction).await {
    Ok(true) => {
      transaction.commit().await?;

      Ok(true)
    }
    Ok(false) => {
      transaction.rollback().await?;

      Err(Failure::Refused(format!("the wake of reminder #{}", reminder.id)))
    }
    Err(why) => {
      transaction.rollback().await?;

      Err(why.into())
    }
  }
}

/// Every reminder whose moment has come at `now`, in epoch seconds, woken in its own room; how many were.
pub async fn fire_due(
  db: &DatabaseConnection,
  agency: &dyn Agency,
  receiver: &dyn Receiver,
  now: i64,
) -> Result<usize, Failure> {
  let mut fired = 0;

  for reminder in due(db, now).await? {
    let next = reminder.every.as_deref().and_then(parse_every).map(|rule| next_moment(rule, now));
    let memory = match reminder.memory_id {
      Some(id) => item_of(db, id).await?,
      None => None,
    };
    let room = room_name(receiver, reminder.chat_id).await?;

    if fire(db, agency, &reminder, &wake(&reminder, &room, memory.as_ref(), now, next), next).await? {
      fired += 1;
    }
  }

  Ok(fired)
}
