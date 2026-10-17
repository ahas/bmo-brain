//! The reminders an agent sets itself (docs/boundary-api.md, 「Watches and reminders」 and 「Runs launched
//! apart」), served on a local port: a repeat rule read only in its own grammar and its next moment, a reminder
//! set, listed and called off, serving only an open memory of its own agent's in its own room, every one whose
//! moment came woken once in its room with a repeating one moved on, and a watch ended with its memo before its
//! unit is stopped.

mod support;

use bmo_brain::editors::fragments::store::now_seconds;
use bmo_brain::reminders::{Every, Reminder, fire_due, next_moment, parse_every, wake};
use bmo_brain::schedules::clock::{REMINDER_PASS, reminder_pass};
use bmo_brain::watches::Item;
use serde_json::{Value, json};

use support::served::{Served, served};
use support::{AGENT, OTHER_AGENT, ROOM};

// 2026-10-10 09:00 in Seoul, a Saturday.
const SATURDAY_NINE: i64 = 1_791_590_400;
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;

#[test]
fn a_rule_reads_only_in_its_own_grammar() {
  assert_eq!(parse_every(" Daily 7:05 "), Some(Every::Daily { hour: 7, minute: 5 }));
  assert_eq!(
    parse_every("weekly Monday 21:30"),
    Some(Every::Weekly {
      weekday: 1,
      hour: 21,
      minute: 30
    })
  );
  assert_eq!(
    parse_every("monthly 31 09:00"),
    Some(Every::Monthly {
      day: 31,
      hour: 9,
      minute: 0
    })
  );

  for wrong in [
    "daily 24:00",
    "daily 7:5",
    "weekly mo 09:00",
    "monthly 0 09:00",
    "monthly 32 09:00",
    "hourly 09:00",
    "daily",
  ] {
    assert_eq!(parse_every(wrong), None, "{wrong}");
  }

  assert_eq!(parse_every("weekly mon 09:00").unwrap().to_string(), "weekly mon 09:00");
}

#[test]
fn the_next_moment_is_strictly_ahead_and_a_missing_monthly_day_is_the_months_last() {
  let daily = parse_every("daily 09:00").unwrap();

  assert_eq!(next_moment(daily, SATURDAY_NINE - 60), SATURDAY_NINE);
  assert_eq!(next_moment(daily, SATURDAY_NINE), SATURDAY_NINE + DAY);
  // The Monday after.
  assert_eq!(
    next_moment(parse_every("weekly mon 09:00").unwrap(), SATURDAY_NINE),
    SATURDAY_NINE + 2 * DAY
  );
  // 2026-10-31 09:00, then 2026-11-30 09:00, November lacking a 31st.
  let october = next_moment(parse_every("monthly 31 09:00").unwrap(), SATURDAY_NINE);

  assert_eq!(october, SATURDAY_NINE + 21 * DAY);
  assert_eq!(next_moment(parse_every("monthly 31 09:00").unwrap(), october), october + 30 * DAY);
}

impl Served {
  async fn memory_row(&self, slug: &str, agent: &str, chat_id: i64, active: i64) -> i64 {
    self
      .scratch
      .run(
        r#"INSERT INTO "memories" ("slug", "type", "chat_id", "reach", "condition", "action", "set_at", "active", "hits", "event", "agent")
           VALUES ($1, 'promise', $2, 'all', 'when it is time', 'go to the dentist', 1, $3, 0, '', $4)"#,
        vec![slug.into(), chat_id.into(), active.into(), agent.into()],
      )
      .await;

    self
      .scratch
      .count(r#"SELECT "id" FROM "memories" WHERE "slug" = $1"#, vec![slug.into()])
      .await
  }

  async fn set(&self, at: Option<&str>, every: Option<&str>, reason: &str, memory: Option<&str>) -> Value {
    self
      .post(
        "/reminders/set",
        json!({ "agent": AGENT, "chatId": ROOM, "at": at, "every": every, "reason": reason, "memory": memory, "user": 5 }),
      )
      .await
  }

  async fn reminder(&self, agent: &str, due_at: i64, every: Option<&str>, memory: Option<i64>) -> i64 {
    self
      .scratch
      .run(
        r#"INSERT INTO "reminders" ("agent", "chat_id", "due_at", "reason", "memory_id", "every", "fired", "created_at")
           VALUES ($1, $2, $3, 'say the meeting starts', $4, $5, 0, $6)"#,
        vec![agent.into(), ROOM.into(), due_at.into(), memory.into(), every.into(), (due_at - HOUR).into()],
      )
      .await;

    self.scratch.count(r#"SELECT MAX("id") FROM "reminders""#, vec![]).await
  }
}

fn refused(answer: &Value) -> (String, String) {
  assert_eq!(answer["answered"], json!(false), "{answer}");
  (
    answer["refusal"]["code"].as_str().unwrap().to_owned(),
    answer["refusal"]["message"].as_str().unwrap().to_owned(),
  )
}

#[tokio::test]
async fn a_reminder_is_set_listed_and_called_off_and_serves_only_its_own_open_memory() {
  let api = served().await;

  assert_eq!(refused(&api.set(None, None, "say it", None).await).0, "usage");
  assert_eq!(
    refused(&api.set(Some("2099-01-01 09:00"), Some("daily 09:00"), "x", None).await).0,
    "usage"
  );
  assert_eq!(refused(&api.set(Some("2099-01-01 09:00"), None, "   ", None).await).0, "usage");
  assert!(
    refused(&api.set(Some("2099-01-01 09:00"), None, &"x".repeat(1001), None).await)
      .1
      .starts_with("a reason is at most 1000 characters")
  );
  assert_eq!(
    refused(&api.set(Some("2001-01-01 09:00"), None, "late", None).await),
    ("reminder-refused".into(), "refusing a time in the past".into())
  );
  assert_eq!(
    refused(&api.set(Some("tomorrow"), None, "x", None).await).1,
    "cannot parse time: tomorrow"
  );
  assert!(
    refused(&api.set(None, Some("hourly 09:00"), "x", None).await)
      .1
      .starts_with("cannot read the repeat rule: hourly 09:00;")
  );

  let memory = api.memory_row("dentist", AGENT, ROOM, 1).await;

  api.memory_row("theirs", OTHER_AGENT, ROOM, 1).await;
  api.memory_row("elsewhere", AGENT, -200, 1).await;
  api.memory_row("closed", AGENT, ROOM, 0).await;

  for (handle, why) in [
    ("nothing", "there is no memory nothing."),
    (
      "theirs",
      "memory theirs is another agent's; a reminder serves only a row of its own agent's.",
    ),
    (
      "elsewhere",
      "memory elsewhere is not this room's; a reminder serves only a row of its own room.",
    ),
    (
      "closed",
      "memory closed is no longer open; there is nothing left for a reminder to serve.",
    ),
  ] {
    assert_eq!(refused(&api.set(Some("2099-01-01 09:00"), None, "x", Some(handle)).await).1, why);
  }

  assert_eq!(api.scratch.count(r#"SELECT COUNT(*) FROM "reminders""#, vec![]).await, 0);

  let once = api.set(Some("2099-01-01 09:00"), None, "  dentist  ", Some("dentist")).await;
  let daily = api.set(None, Some("Daily 08:30"), "stand-up", None).await;
  let once_line = once["lines"][0].as_str().unwrap();
  let daily_line = daily["lines"][0].as_str().unwrap();

  assert!(
    once_line.starts_with("reminder #") && once_line.ends_with(": 2099-01-01 09:00 KST"),
    "{once_line}"
  );
  assert!(daily_line.contains(": every daily 08:30, next "), "{daily_line}");

  let listed = api.post("/reminders/list", json!({ "chatId": ROOM })).await;
  let lines: Vec<&str> =
    listed["lines"].as_array().unwrap().iter().map(|line| line.as_str().unwrap()).collect();

  assert_eq!(lines.len(), 2);
  assert!(lines[0].ends_with(" for 5: stand-up"), "{lines:?}");
  assert!(
    lines[1].ends_with(&format!(": 2099-01-01 09:00 KST for 5 [memory #{memory}]: dentist")),
    "{lines:?}"
  );

  let daily_id: i64 = daily_line["reminder #".len()..].split(':').next().unwrap().parse().unwrap();

  assert_eq!(
    api.post("/reminders/cancel", json!({ "id": daily_id })).await["lines"],
    json!([format!("cancelled #{daily_id}")])
  );
  assert_eq!(
    refused(&api.post("/reminders/cancel", json!({ "id": daily_id })).await),
    ("no-reminder".into(), format!("no reminder #{daily_id}"))
  );
  assert_eq!(
    api.post("/reminders/list", json!({ "chatId": ROOM })).await["reminders"]
      .as_array()
      .unwrap()
      .len(),
    1
  );
}

#[tokio::test]
async fn a_reminder_whose_moment_came_wakes_its_agent_once_and_a_repeating_one_moves_on() {
  let api = served().await;
  let now = now_seconds();
  let once = api.reminder(OTHER_AGENT, now - 120, None, None).await;
  let daily = api.reminder(AGENT, now - 60, Some("daily 09:00"), None).await;
  let later = api.reminder(AGENT, now + HOUR, None, None).await;

  assert_eq!(fire_due(api.scratch.db(), api.receiver.as_ref(), now).await.unwrap(), 2);
  assert_eq!(fire_due(api.scratch.db(), api.receiver.as_ref(), now).await.unwrap(), 0);

  let fired =
    |id: i64| api.scratch.count(r#"SELECT "fired" FROM "reminders" WHERE "id" = $1"#, vec![id.into()]);

  assert_eq!(fired(once).await, 1);
  assert_eq!(fired(daily).await, 0);
  assert_eq!(fired(later).await, 0);
  assert!(
    api
      .scratch
      .count(r#"SELECT "due_at" FROM "reminders" WHERE "id" = $1"#, vec![daily.into()])
      .await
      > now
  );

  let wakes = api.receiver.calls.of("quiet_instruction");

  assert_eq!(wakes.len(), 2);
  assert_eq!(
    (wakes[0]["agent"].clone(), wakes[0]["chatId"].clone(), wakes[0]["kind"].clone()),
    (json!(OTHER_AGENT), json!(ROOM), json!("reminder"))
  );

  let text = wakes[0]["text"].as_str().unwrap();

  assert!(text.starts_with("A reminder you set has come due."), "{text}");
  assert!(text.contains(&format!("in Room {ROOM} (chat {ROOM})")), "{text}");
  assert!(wakes[1]["text"].as_str().unwrap().contains(&format!("cancel reminder #{daily}")));

  // A wake the agency will not take leaves the moment standing for the next pass.
  let refused = api.reminder(AGENT, now - 60, None, None).await;

  *api.receiver.quiet_refused.lock().unwrap() = true;
  assert!(fire_due(api.scratch.db(), api.receiver.as_ref(), now).await.is_err());
  assert_eq!(fired(refused).await, 0);
  *api.receiver.quiet_refused.lock().unwrap() = false;

  // The clock's pass is the same firing, every thirty seconds from the start.
  reminder_pass(&api.api).await.unwrap();
  assert_eq!(fired(refused).await, 1);
  assert_eq!(REMINDER_PASS, std::time::Duration::from_secs(30));
}

#[test]
fn a_wake_says_how_late_it_comes_and_tells_of_the_memory_as_it_stands() {
  let reminder = Reminder {
    id: 4,
    agent: "agent-a".into(),
    chat_id: ROOM,
    user_id: Some(5),
    due_at: SATURDAY_NINE,
    reason: "the meeting starts".into(),
    memory_id: Some(12),
    every: Some("daily 09:00".into()),
    created_at: SATURDAY_NINE - DAY,
  };
  let open = Item {
    id: 12,
    slug: "standup".into(),
    kind: "promise".into(),
    chat_id: Some(ROOM),
    condition: "when the meeting is near".into(),
    action: "remind everyone".into(),
    active: true,
    done_at: None,
    done_text: None,
    down_reason: None,
  };
  let on_time = wake(&reminder, "the room", Some(&open), SATURDAY_NINE + 60, Some(SATURDAY_NINE + DAY));

  assert!(on_time.contains(
    "You set reminder #4 at 2026-10-09 09:00 KST in the room (chat -100), repeating daily 09:00."
  ));
  assert!(!on_time.contains("late"));
  assert!(on_time.contains("It is for user 5."));
  assert!(on_time.contains("It serves your memory #12 (standup, promise), as it stands now:\nWhen: when the meeting is near\nremind everyone\nEND OF MEMORY"));
  assert!(
    on_time.ends_with(
      "It comes again at 2026-10-11 09:00 KST. Once it is no longer wanted, cancel reminder #4."
    )
  );

  let late = wake(&reminder, "the room", None, SATURDAY_NINE + 45 * 60, None);

  assert!(late.contains("so this comes 45 minutes late and stands for every time it came due meanwhile"));
  assert!(late.contains("It was set for memory #12, which can no longer be found."));

  let done = Item {
    done_at: Some(SATURDAY_NINE),
    done_text: Some("went".into()),
    ..open.clone()
  };
  let down = Item {
    active: false,
    down_reason: Some("moved".into()),
    ..open
  };

  assert!(
    wake(&reminder, "r", Some(&done), SATURDAY_NINE, None)
      .contains("which you checked off at 2026-10-10 09:00 KST: went.")
  );
  assert!(
    wake(&reminder, "r", Some(&down), SATURDAY_NINE, None).contains("which has been taken down: moved.")
  );
}

#[tokio::test]
async fn a_watch_is_ended_with_its_memo_before_its_unit_is_stopped() {
  let api = served().await;
  let set = api
    .post(
      "/watches/set",
      json!({ "turn": { "agent": AGENT, "chatId": ROOM }, "until": "test -e /tmp/never-there", "note": "tell them once the job ends" }),
    )
    .await;

  assert_eq!(set["answered"], json!(true), "{set}");

  let unit = api.starter.calls.of("start")[0]["unit"].as_str().unwrap().to_owned();

  assert_eq!(api.post("/watches/stopped", json!({ "unit": unit })).await, json!({ "ended": true }));
  assert_eq!(api.post("/watches/stopped", json!({ "unit": unit })).await, json!({ "ended": false }));
  assert_eq!(
    api.post("/watches/stopped", json!({ "unit": "bmo-watch-0-0" })).await,
    json!({ "ended": false })
  );

  let memos = api.receiver.calls.of("quiet_instruction");
  let text = memos[0]["text"].as_str().unwrap();

  assert_eq!(memos.len(), 1);
  assert!(
    text.starts_with("A watch you set yourself was stopped before it came to anything."),
    "{text}"
  );
  assert!(text.contains("Its check is not being run any more: test -e /tmp/never-there"));
  assert_eq!(
    api.post("/watches/list", json!({ "by": ROOM, "all": true })).await["watches"][0]["state"],
    json!("cancelled")
  );
}
