//! The watches' and reminders' routes beside the one that sets a watch (docs/memories.md, 「Watches」;
//! docs/boundary-api.md, 「Watches and reminders」): a room's watches or every room's listed, one called off, one's
//! run launched, a run's claim on its watch and its ending of it, a watch ended before its unit is stopped, and a
//! reminder set, called off or a room's listed.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, json};

use super::{Answer, Api, ApiError, Body, reading, refusal};
use crate::editors::archive::room_name;
use crate::editors::fragments::store::now_seconds;
use crate::editors::people::launcher::wall_clock;
use crate::reminders::{NotSet, REMIND_USAGE, RemindAsked, cancel_reminder, remind, reminders};
use crate::watches::{
  ListAsked, NotCancelled, Wait, broken_memo, cancel, claim, end_by_run, fired_memo, item_of, lapsed_memo,
  list, stop_of_unit, watch_of,
};

/// The watches' and reminders' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/watches/list", post(watches_list))
    .route("/watches/cancel", post(watches_cancel))
    .route("/watches/launch", post(watches_launch))
    .route("/watches/claim", post(watches_claim))
    .route("/watches/end", post(watches_end))
    .route("/watches/stopped", post(watches_stopped))
    .route("/reminders/set", post(reminders_set))
    .route("/reminders/cancel", post(reminders_cancel))
    .route("/reminders/list", post(reminders_list))
}

#[derive(Debug, Deserialize)]
struct ListBody {
  by: Option<i64>,
  agent: Option<String>,
  #[serde(default)]
  all: bool,
}

async fn watches_list(State(api): State<Arc<Api>>, Body(asked): Body<ListBody>) -> Answer {
  let watches = list(
    &api.db,
    &ListAsked {
      by: asked.by,
      agent: asked.agent.as_deref(),
      all: asked.all,
    },
  )
  .await?;
  let lines: Vec<String> = if watches.is_empty() {
    vec!["(no watches)".to_owned()]
  } else {
    watches.iter().map(|watch| watch.line()).collect()
  };

  Ok(Json(json!({ "watches": watches, "lines": lines })))
}

#[derive(Debug, Deserialize)]
struct CancelBody {
  id: i64,
  by: i64,
}

async fn watches_cancel(State(api): State<Arc<Api>>, Body(asked): Body<CancelBody>) -> Answer {
  let id = asked.id;
  let (code, message) = match cancel(&api.db, id, asked.by, api.owner, now_seconds()).await? {
    Ok(()) => return Ok(reading(vec![format!("cancelled #{id}")])),
    Err(NotCancelled::NoOwner) => (
      "no-owner",
      "The owner is not known, so no watch is called off: the configuration names no owner.".to_owned(),
    ),
    Err(NotCancelled::NoWatch) => ("no-watch", format!("no watch #{id}")),
    Err(NotCancelled::NotTheirs) => ("not-theirs", format!("watch #{id} was set by another chat")),
    Err(NotCancelled::Ended) => ("ended", format!("watch #{id} has already ended")),
  };

  Ok(refusal(code, &message, Map::new(), vec![]))
}

#[derive(Debug, Deserialize)]
struct Named {
  id: i64,
}

async fn watches_launch(State(api): State<Arc<Api>>, Body(asked): Body<Named>) -> Answer {
  let launched = api.launches().launch_watch_of(asked.id, wall_clock()).await?;

  Ok(Json(json!({ "launched": launched })))
}

#[derive(Debug, Deserialize)]
struct ClaimBody {
  id: i64,
  run: String,
}

async fn watches_claim(State(api): State<Arc<Api>>, Body(asked): Body<ClaimBody>) -> Answer {
  Ok(Json(json!({ "standing": claim(&api.db, asked.id, &asked.run).await? })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Outcome {
  Fired,
  Lapsed,
  Broken,
}

#[derive(Debug, Deserialize)]
struct EndBody {
  id: i64,
  run: String,
  outcome: Outcome,
  exit: Option<i64>,
  #[serde(default)]
  printed: String,
}

// A run's ending of its watch, its memo composed from the watch as it stands: nothing is ended where the watch is
// no longer this run's.
async fn watches_end(State(api): State<Arc<Api>>, Body(asked): Body<EndBody>) -> Answer {
  let Some(standing) = watch_of(&api.db, asked.id).await? else {
    return Ok(Json(json!({ "ended": false })));
  };

  if standing.state != "active" || standing.unit.as_deref() != Some(asked.run.as_str()) {
    return Ok(Json(json!({ "ended": false })));
  }

  let Some(condition) = standing.condition.as_deref() else {
    return Err(ApiError::bad_request("a timer has no run to end it; the brain's clock fires it"));
  };
  let watch = &standing.watch;
  let room = room_name(api.receiver.as_ref(), watch.chat_id).await?;
  let item = match watch.memory_id {
    Some(memory) => item_of(&api.db, memory).await?,
    None => None,
  };
  let now = now_seconds();
  let last = standing.last_exit.zip(standing.last_checked_at);
  let (state, memo) = match asked.outcome {
    Outcome::Fired => (
      "fired",
      fired_memo(
        watch,
        &room,
        item.as_ref(),
        condition,
        standing.last_checked_at.unwrap_or(now),
        &asked.printed,
      ),
    ),
    Outcome::Lapsed => (
      "lapsed",
      lapsed_memo(
        watch,
        &room,
        item.as_ref(),
        &Wait {
          condition,
          deadline_at: standing.deadline_at,
          interval_seconds: standing.interval_seconds,
          last,
        },
      ),
    ),
    Outcome::Broken => (
      "broken",
      broken_memo(
        watch,
        &room,
        item.as_ref(),
        condition,
        asked.exit.or(standing.last_exit).unwrap_or(127),
        &asked.printed,
      ),
    ),
  };
  let ended = end_by_run(&api.db, api.agency.as_ref(), watch, &asked.run, state, &memo, now).await?;

  Ok(Json(json!({ "ended": ended })))
}

#[derive(Debug, Deserialize)]
struct StoppedBody {
  unit: String,
}

async fn watches_stopped(State(api): State<Arc<Api>>, Body(asked): Body<StoppedBody>) -> Answer {
  let ended =
    stop_of_unit(&api.db, api.agency.as_ref(), api.receiver.as_ref(), &asked.unit, now_seconds()).await?;

  Ok(Json(json!({ "ended": ended })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetBody {
  agent: String,
  chat_id: i64,
  at: Option<String>,
  every: Option<String>,
  #[serde(default)]
  reason: String,
  memory: Option<String>,
  user: Option<i64>,
}

async fn reminders_set(State(api): State<Arc<Api>>, Body(asked): Body<SetBody>) -> Answer {
  let set = RemindAsked {
    agent: &asked.agent,
    chat_id: asked.chat_id,
    at: asked.at.as_deref(),
    every: asked.every.as_deref(),
    reason: &asked.reason,
    memory: asked.memory.as_deref(),
    user_id: asked.user,
  };

  Ok(match remind(&api.db, &set, now_seconds()).await? {
    Ok(lines) => reading(lines),
    Err(NotSet::Usage) => {
      let mut params = Map::new();

      params.insert("verb".to_owned(), json!("reminder_set"));
      refusal("usage", REMIND_USAGE, params, vec![])
    }
    Err(NotSet::Refused(why)) => refusal("reminder-refused", &why, Map::new(), vec![]),
  })
}

#[derive(Debug, Deserialize)]
struct CancelReminder {
  id: i64,
}

async fn reminders_cancel(State(api): State<Arc<Api>>, Body(asked): Body<CancelReminder>) -> Answer {
  Ok(if cancel_reminder(&api.db, asked.id).await? {
    reading(vec![format!("cancelled #{}", asked.id)])
  } else {
    refusal("no-reminder", &format!("no reminder #{}", asked.id), Map::new(), vec![])
  })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemindersBody {
  chat_id: i64,
  agent: Option<String>,
  #[serde(default)]
  all: bool,
}

async fn reminders_list(State(api): State<Arc<Api>>, Body(asked): Body<RemindersBody>) -> Answer {
  let found = reminders(&api.db, asked.chat_id, asked.agent.as_deref(), asked.all).await?;
  let lines: Vec<String> = if found.is_empty() {
    vec!["(no reminders)".to_owned()]
  } else {
    found.iter().map(|reminder| reminder.line()).collect()
  };

  Ok(Json(json!({ "reminders": found, "lines": lines })))
}
