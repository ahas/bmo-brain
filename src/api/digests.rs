//! The digests' routes (docs/knowledge.md, 「The digest run」; docs/people-book.md, 「The launcher and the worker」;
//! docs/boundary-api.md, 「The digests」): the memo digest asked for by the clock or by a room, the person book in
//! every form, the owner's pause, and a room's worker before its session folds. What is read or set in a request is
//! answered there; the work itself is launched apart through the agency, in the unit its form runs under, and the
//! route answers what came of the launch.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{FixedOffset, TimeZone};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

use super::{Answer, Api, ApiError, Body, refusal};
use crate::Failure;
use crate::commands::{PeopleRun, root_agent};
use crate::editors::archive::{
  forget_read, keeps_room_book, pending_notes, read_count, reading_since, set_reading_since, unkept_rooms,
  unread_counts,
};
use crate::editors::fragments::store::now_seconds;
use crate::editors::memo::MemoDigest;
use crate::editors::people::BATCH_SIZE;
use crate::editors::people::sort::{Failures, sort_status};
use crate::editors::prompts::seoul_timestamp;
use crate::editors::recollection::recollection_for;
use crate::editors::settle::{
  PauseState, digest_paused, digest_paused_until, pause_digests, resting, resume_digests,
};
use crate::ports::agency::{Launch, LaunchOutcome};
use crate::ports::http::PortFailure;
use crate::schedules::launches::{BeforeFold, RoomDigest};

/// The digests' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/digest/run", post(run))
    .route("/digest/now", post(now))
    .route("/digest/people", post(people))
    .route("/digest/pause", post(pause))
    .route("/people/before-fold", post(before_fold))
}

fn outcome(word: &str) -> Json<Value> {
  Json(json!({ "outcome": word }))
}

// What a launch came to, as a route answers it.
fn launched(unit: &str, launch: Result<Launch, PortFailure>) -> Json<Value> {
  match launch {
    Ok(Launch {
      outcome: LaunchOutcome::Launched,
      ..
    }) => Json(json!({ "outcome": "launched", "unit": unit })),
    Ok(Launch {
      outcome: LaunchOutcome::Running,
      ..
    }) => Json(json!({ "outcome": "running", "unit": unit })),
    Ok(Launch { why, .. }) => Json(json!({ "outcome": "refused", "why": why.unwrap_or_default() })),
    Err(why) => Json(json!({ "outcome": "refused", "why": why.to_string() })),
  }
}

// How many notes wait for the memo digest, in one room where one is named.
async fn pending(api: &Api, chat_id: Option<i64>) -> Result<usize, Failure> {
  Ok(
    pending_notes(&api.db, None)
      .await?
      .iter()
      .filter(|note| chat_id.is_none_or(|chat_id| note.chat_id == chat_id))
      .count(),
  )
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunAsked {
  chat_id: Option<i64>,
}

async fn run(State(api): State<Arc<Api>>, Body(asked): Body<RunAsked>) -> Answer {
  // While the digests are paused nothing starts, and what the pause holds back is said to the owner once.
  if digest_paused(&api.db).await? {
    MemoDigest {
      ground: api.ground(),
    }
    .notice_pause()
    .await?;
    return Ok(outcome("paused"));
  }

  // The digest is a full high-effort session; during a rest it must not start.
  if resting(api.receiver.as_ref()).await? {
    return Ok(outcome("resting"));
  }

  if pending(&api, asked.chat_id).await? == 0 {
    return Ok(outcome("nothing"));
  }

  let launches = api.launches();

  Ok(match asked.chat_id {
    None => launched(crate::schedules::launches::MEMO_UNIT, launches.memo_digest().await),
    Some(chat_id) => room_digest(launches.room_digest(chat_id).await?, None),
  })
}

fn room_digest(digest: RoomDigest, notes: Option<usize>) -> Json<Value> {
  match digest {
    RoomDigest::Running(unit) => Json(json!({ "outcome": "running", "unit": unit })),
    RoomDigest::Launched(unit) => match notes {
      Some(notes) => Json(json!({ "outcome": "launched", "unit": unit, "notes": notes })),
      None => Json(json!({ "outcome": "launched", "unit": unit })),
    },
    RoomDigest::Refused(why) => Json(json!({ "outcome": "refused", "why": why })),
  }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NowAsked {
  // The turn's agent, which the wire names: the room's digest files every agent's notes of the room.
  #[allow(dead_code)]
  agent: Option<String>,
  chat_id: i64,
}

// One room's digest launched where nothing stops it: never during a rest, never for a room with no pending notes,
// and never beside a digest of the room still running.
async fn now(State(api): State<Arc<Api>>, Body(asked): Body<NowAsked>) -> Answer {
  if resting(api.receiver.as_ref()).await? {
    return Ok(outcome("resting"));
  }

  let notes = pending(&api, Some(asked.chat_id)).await?;

  if notes == 0 {
    return Ok(outcome("nothing"));
  }

  Ok(room_digest(api.launches().room_digest(asked.chat_id).await?, Some(notes)))
}

// A field present with its value, present as null, or absent: `since` set to a day or `none`, read, or not asked.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
  Option::<String>::deserialize(deserializer).map(Some)
}

/// What the person book's route is asked.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleAsked {
  pub chat_id: Option<i64>,
  pub agent: Option<String>,
  #[serde(default)]
  pub all: bool,
  #[serde(default)]
  pub split: bool,
  pub person: Option<i64>,
  pub spans: Option<i64>,
  #[serde(default)]
  pub sheets: bool,
  #[serde(default)]
  pub reform: bool,
  pub batches: Option<i64>,
  #[serde(default)]
  pub sort: bool,
  #[serde(default)]
  pub status: bool,
  pub groups: Option<i64>,
  #[serde(default, deserialize_with = "present")]
  pub since: Option<Option<String>>,
  #[serde(default)]
  pub forget_read: bool,
}

/// The moment a Korean calendar day begins, for a day written `YYYY-MM-DD`, or none for anything else.
pub fn seoul_day_start(day: &str) -> Option<i64> {
  let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
  let shaped = day.len() == 10 && day.as_bytes()[4] == b'-' && day.as_bytes()[7] == b'-';
  let moment = date.and_hms_opt(0, 0, 0)?.and_utc().timestamp() - 9 * 3600;

  (shaped && seoul_timestamp(moment).starts_with(day)).then_some(moment)
}

fn coded(code: &str, message: &str, params: &[(&str, Value)]) -> Json<Value> {
  let params: Map<String, Value> =
    params.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect();

  refusal(code, message, params, vec![])
}

fn answered(lines: Vec<String>) -> Json<Value> {
  Json(json!({ "outcome": "answered", "answered": true, "lines": lines }))
}

#[allow(clippy::too_many_lines)]
async fn people(State(api): State<Arc<Api>>, Body(asked): Body<PeopleAsked>) -> Answer {
  let ground = api.ground();
  let agent = match &asked.agent {
    Some(agent) => agent.clone(),
    None => root_agent(&ground).await?,
  };
  let Some(recollection) =
    recollection_for(ground.agency, ground.brain_root, ground.chat_key, &agent).await?
  else {
    return Ok(coded(
      "no-memory",
      "The agent keeps no long-term memory, so it has no person book.",
      &[("agent", json!(agent))],
    ));
  };
  let chat = asked.chat_id;

  // An agent's book is written from the rooms it answers and no other, and never from a one-to-one.
  if let Some(chat) = chat {
    let answers = api
      .receiver
      .room_answerers(Some(chat))
      .await
      .map_err(Failure::from)?
      .iter()
      .any(|room| room.chat_id == chat && room.agents.contains(&agent));

    if !answers {
      return Ok(coded(
        "not-answered",
        "The agent does not answer this room, so its book is not written from there.",
        &[("agent", json!(agent))],
      ));
    }

    if !keeps_room_book(api.receiver.as_ref(), chat).await? {
      return Ok(coded(
        "one-to-one",
        "The room is a one-to-one, which keeps no room part of the person book.",
        &[],
      ));
    }
  }

  if asked.reform && (chat.is_none() || asked.batches.is_none_or(|batches| batches < 1)) {
    return Err(ApiError::bad_request(
      "A book is written again with reform, a chatId and batches of at least 1.",
    ));
  }

  if asked.sort
    && (chat.is_some()
      || (asked.person.is_none() && !asked.all && !asked.status)
      || asked.groups.is_some_and(|groups| groups < 1))
  {
    return Err(ApiError::bad_request(
      "The sort takes no chatId, and takes a person, all or status, with groups of at least 1 where given.",
    ));
  }

  // What is left to sort is read off the disk and asks for nothing, so it answers during a rest too.
  if asked.sort && asked.status {
    let unkept = unkept_rooms(api.receiver.as_ref(), &recollection.brain).await?;
    let (mut lines, people) =
      sort_status(&recollection.brain, &mut Failures { db: &api.db }, &unkept).await?;
    let left = if people.is_empty() {
      "none".to_owned()
    } else {
      people.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")
    };

    lines.push(format!("people left to sort: {left}"));

    return Ok(answered(lines));
  }

  // The day a room's book starts reading from, which is the brain's own and answers during a rest too.
  if let Some(day) = &asked.since {
    let Some(chat) = chat else {
      return Err(ApiError::bad_request("The day a book starts reading from is read or set for a chatId."));
    };

    if let Some(day) = day {
      let moment = if day == "none" {
        None
      } else {
        Some(
          seoul_day_start(day)
            .ok_or_else(|| ApiError::bad_request(format!("Not a day: {day} (YYYY-MM-DD, Korean time).")))?,
        )
      };

      set_reading_since(&api.db, &agent, chat, moment).await?;
    }

    let since = reading_since(&api.db, &agent, chat).await?;
    let left = unread_counts(&api.db, api.receiver.as_ref(), &agent)
      .await?
      .get(&chat)
      .copied()
      .unwrap_or(0);

    return Ok(answered(vec![format!(
      "chat {chat} person book of {} reads {}; {left} messages left to read",
      recollection.label,
      since.map_or_else(
        || "from its first message".to_owned(),
        |since| format!("from {}", seoul_timestamp(since).chars().take(10).collect::<String>())
      )
    )]));
  }

  // Every person-book mark taken off, for one room or all, so the books are read again from the start.
  if asked.forget_read {
    if chat.is_none() == !asked.all {
      return Err(ApiError::bad_request("Marks are forgotten for a chatId, or for all rooms with all."));
    }

    let forgotten = forget_read(api.receiver.as_ref(), &agent, chat).await?;

    return Ok(answered(vec![format!(
      "people digest: {forgotten} messages will be read into the person book of {} again",
      recollection.label
    )]));
  }

  if (asked.split || asked.sheets) && chat.is_none() {
    return Err(ApiError::bad_request("A cut and a sheets' rewrite are asked for a chatId."));
  }

  if asked.spans.is_some_and(|spans| spans < 1) {
    return Err(ApiError::bad_request("A cut takes spans of at least 1."));
  }

  // A book rewritten halfway cannot take the old one's place, so a room needing more batches is refused first.
  if let (true, Some(chat), Some(given)) = (asked.reform, chat, asked.batches) {
    let folded = read_count(api.receiver.as_ref(), &agent, chat).await?;
    let needed = (folded + BATCH_SIZE - 1) / BATCH_SIZE;

    if needed > given {
      return Ok(coded(
        "too-few-batches",
        "The room's folded messages do not fit the batches given.",
        &[("needed", json!(needed)), ("given", json!(given))],
      ));
    }
  }

  let run = PeopleRun {
    chat,
    agent: Some(agent.clone()),
    all: asked.all,
    split: asked.split,
    sheets: asked.sheets,
    reform: asked.reform,
    sort: asked.sort,
    person: asked.person,
    spans: asked.spans,
    batches: asked.batches,
    groups: asked.groups,
  };
  let (unit, launch) = api.launches().people_run(&run, &agent).await;

  Ok(launched(&unit, launch))
}

/// What the pause route is told to do: show how the digests stand, lift the pause, pause them until someone lifts
/// it, or pause them for a while, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseAsked {
  Status,
  Off,
  On,
  For(i64),
}

impl PauseAsked {
  /// The word given read: `status`, `off`, `on`, or a length such as `2h`, `90m` or `30s`, a bare number being
  /// minutes. None for anything else, a length of nought among it.
  pub fn read(word: &str) -> Option<Self> {
    match word {
      "status" => return Some(Self::Status),
      "off" => return Some(Self::Off),
      "on" => return Some(Self::On),
      _ => {}
    }

    let word = word.trim();
    let digits = word.trim_end_matches(|character: char| !character.is_ascii_digit());

    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
      return None;
    }

    let amount: i64 = digits.parse().ok()?;
    let seconds = match word[digits.len()..].trim_start().to_ascii_lowercase().as_str() {
      "h" => amount.checked_mul(3600)?,
      "m" | "" => amount.checked_mul(60)?,
      "s" => amount,
      _ => return None,
    };

    (seconds > 0).then_some(Self::For(seconds))
  }
}

/// How the digests stand, as the pause's reading says it.
pub fn pause_line(state: PauseState) -> String {
  match state {
    PauseState::Running => "digests are running".to_owned(),
    PauseState::UntilLifted => "digests are paused until told otherwise".to_owned(),
    PauseState::Until(until) => format!(
      "digests are paused until {} (Korean time)",
      FixedOffset::east_opt(9 * 3600)
        .and_then(|seoul| seoul.timestamp_opt(until, 0).single())
        .map_or_else(|| until.to_string(), |at| at.format("%Y-%m-%d %H:%M:%S").to_string())
    ),
  }
}

#[derive(Debug, Deserialize)]
struct PauseBody {
  set: String,
}

async fn pause(State(api): State<Arc<Api>>, Body(asked): Body<PauseBody>) -> Answer {
  let Some(set) = PauseAsked::read(&asked.set) else {
    return Err(ApiError::bad_request(
      "The pause is set with status, off, on, or a length such as 2h, 90m or 30s, a bare number being minutes.",
    ));
  };

  match set {
    PauseAsked::Status => {}
    PauseAsked::Off => resume_digests(&api.db).await?,
    PauseAsked::On => pause_digests(&api.db, None).await?,
    PauseAsked::For(seconds) => pause_digests(&api.db, Some(now_seconds() + seconds)).await?,
  }

  let state = PauseState::of(digest_paused_until(&api.db).await?);
  let until = match state {
    PauseState::Running => Value::Null,
    PauseState::UntilLifted => json!("forever"),
    PauseState::Until(until) => json!(until),
  };

  Ok(Json(json!({ "pausedUntil": until, "answered": true, "lines": [pause_line(state)] })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FoldAsked {
  chat_id: i64,
  agent: String,
}

// The agent's worker of the room launched before its session folds, where the agent's book is written from the room;
// a room that cannot be read starts all the same.
async fn before_fold(State(api): State<Arc<Api>>, Body(asked): Body<FoldAsked>) -> Answer {
  Ok(match api.launches().people_before_fold(asked.chat_id, &asked.agent).await {
    BeforeFold::Skipped => outcome("skipped"),
    BeforeFold::Launched(unit) => Json(json!({ "outcome": "launched", "unit": unit })),
    BeforeFold::Running(unit) => Json(json!({ "outcome": "running", "unit": unit })),
    BeforeFold::Refused(why) => Json(json!({ "outcome": "refused", "why": why })),
  })
}
