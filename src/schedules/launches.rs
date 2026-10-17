//! What the brain's scheduled work starts apart (docs/boundary-api.md, 「Runs launched apart」): a digest that came
//! due launched where no run of its unit stands (docs/knowledge.md, docs/people-book.md), one room's digest or
//! person-book worker, and the checks of the watches that no run holds launched again, each in a run of its own
//! (docs/memories.md, 「Watches」). Every run is one of the brain's own binary, which the brain starts itself as a
//! unit of the user's service manager; whether a unit still runs is the agency's alive.

use std::collections::HashSet;

use brain_support::database::{all, run};

use crate::Failure;
use crate::commands::PeopleRun;
use crate::editors::archive::{book_room, room_name};
use crate::editors::ground::Ground;
use crate::editors::people::{LAUNCHER_UNIT, sort_unit};
use crate::editors::recollection::recollections;
use crate::schedules::apart::{Launch, LaunchOutcome, UnitLaunch};

// How long after it was armed a check is left to its run before a pass may take it for one nobody holds: a run
// just launched is not yet among the running.
const ARM_GRACE_SECONDS: i64 = 60;

/// The memo digest's unit, which one room's digest is named from.
pub const MEMO_UNIT: &str = "bmo-digest";

// The words of the brain's own binary each run is started with, and the work each does.
const MEMO_WORDS: &str = "digest";
const PEOPLE_WORDS: &str = "people-digest";
const WATCH_WORDS: &str = "watch-runner";

/// One active watch, as much of it as a launch reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
  pub id: i64,
  pub chat_id: i64,
  pub by_chat: i64,
  pub kind: String,
  pub unit: Option<String>,
  pub armed_at: Option<i64>,
  pub deadline_at: i64,
  pub interval_seconds: Option<i64>,
}

/// The checks a pass launches again: every condition watch no run holds, once its last run has had the grace to
/// come up. A check past its deadline is launched all the same, since only a run may check it one last time before
/// it lapses.
pub fn to_relaunch(active: &[Standing], held: &HashSet<String>, now: i64) -> Vec<Standing> {
  active
    .iter()
    .filter(|watch| {
      watch.kind == "condition"
        && watch.unit.as_ref().is_none_or(|unit| !held.contains(unit))
        && watch.armed_at.is_none_or(|armed| now - armed >= ARM_GRACE_SECONDS)
    })
    .cloned()
    .collect()
}

/// The unit one launch of a watch's check runs under, which its run claims the watch by.
pub fn watch_unit(id: i64, now_ms: i64) -> String {
  format!("bmo-watch-{id}-{now_ms}")
}

/// One room's digest's unit.
pub fn room_digest_unit(chat_id: i64) -> String {
  format!("{MEMO_UNIT}-{chat_id}")
}

/// What asking for a room's worker before its session folds came to: nothing started for a room the agent's book is
/// not written from, a worker launched or already standing under its unit, or why the launch was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum BeforeFold {
  Skipped,
  Launched(String),
  Running(String),
  Refused(String),
}

/// What asking for one room's digest came to: a run of it already standing, one launched, or why it was refused,
/// the first two under the unit the room's digest goes by.
#[derive(Debug, PartialEq, Eq)]
pub enum RoomDigest {
  Running(String),
  Launched(String),
  Refused(String),
}

/// The scheduled work's launches: what the brain stands on, and the words that start its own binary.
pub struct Launches<'a> {
  pub ground: Ground<'a>,
  pub program: &'a [String],
}

impl Launches<'_> {
  /// A run's whole command: the brain's own binary, the run's words, and the unit it runs under.
  pub fn command(&self, unit: &str, words: &[&str]) -> Vec<String> {
    self
      .program
      .iter()
      .cloned()
      .chain(words.iter().map(|word| (*word).to_owned()))
      .chain(["--unit".to_owned(), unit.to_owned()])
      .collect()
  }

  /// One run started apart: its unit, what it is titled, and its words.
  pub async fn launch(&self, unit: &str, title: &str, words: &[&str]) -> Launch {
    let launch = UnitLaunch {
      unit: unit.to_owned(),
      title: title.to_owned(),
      command: self.command(unit, words),
    };

    self.ground.starter.start(&launch).await
  }

  // A run started apart where no run of its unit stands, a refusal said in the log.
  async fn detached(&self, unit: &str, title: &str, words: &[&str]) {
    if let Launch {
      outcome: LaunchOutcome::Refused,
      why,
    } = self.launch(unit, title, words).await
    {
      eprintln!("{unit} launch failed: {}", why.unwrap_or_default());
    }
  }

  /// A room as a run's title names it: its title, or the owner's private chat, or its number.
  pub async fn room_name(&self, chat_id: i64) -> Result<String, Failure> {
    room_name(self.ground.receiver, chat_id).await
  }

  /// The agents keeping a long-term memory, the brain root's first.
  pub async fn recollectors(&self) -> Result<Vec<String>, Failure> {
    let ground = &self.ground;

    Ok(
      recollections(ground.agency, ground.brain_root, ground.chat_key)
        .await?
        .into_iter()
        .map(|recollection| recollection.agent)
        .collect(),
    )
  }

  /// The memo digest of every room's pending notes launched where no run of it stands.
  pub async fn memo_digest(&self) -> Launch {
    self.launch(MEMO_UNIT, "All pending notes", &[MEMO_WORDS]).await
  }

  /// A run of the person book asked for by hand, in the unit its form runs under: the sort in the agent's sort unit,
  /// a room's work in the agent's worker of the room, and the launcher in its own. Answers the unit and what came of
  /// the launch.
  pub async fn people_run(&self, run: &PeopleRun, agent: &str) -> (String, Launch) {
    let (unit, title) = if run.sort {
      (sort_unit(agent), "Sorting the person book".to_owned())
    } else if let Some(chat_id) = run.chat {
      let title = self.room_name(chat_id).await.unwrap_or_else(|_| chat_id.to_string());

      (crate::editors::people::worker_unit(chat_id, agent), title)
    } else {
      (LAUNCHER_UNIT.to_owned(), "Rooms in turn".to_owned())
    };
    let words = run.words(agent);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let launched = self.launch(&unit, &title, &words).await;

    (unit, launched)
  }

  /// A digest that came due, by its name: the memo digest, or the person-book launcher with every recollecting
  /// agent's sort, each in a unit of its own so one agent's sort never waits on another's.
  pub async fn digest(&self, digest: &str) -> Result<(), String> {
    match digest {
      "memo" => self.detached(MEMO_UNIT, "All pending notes", &[MEMO_WORDS]).await,
      "people" => {
        self.detached(LAUNCHER_UNIT, "Rooms in turn", &[PEOPLE_WORDS]).await;

        for agent in self.recollectors().await.map_err(|why| why.to_string())? {
          self
            .detached(
              &sort_unit(&agent),
              "Sorting the person book",
              &[PEOPLE_WORDS, "--sort", "--all", "--agent", &agent],
            )
            .await;
        }
      }
      other => return Err(format!("no digest is called {other}")),
    }

    Ok(())
  }

  /// One room's notes digested now rather than on the hourly schedule, in a unit of the room's own, so a digest
  /// still running for it is found under that name and never doubled.
  pub async fn room_digest(&self, chat_id: i64) -> Result<RoomDigest, Failure> {
    let unit = room_digest_unit(chat_id);
    let title = self.room_name(chat_id).await?;
    let chat = chat_id.to_string();
    let launch = self.launch(&unit, &title, &[MEMO_WORDS, "--chat", &chat]).await;

    Ok(match launch.outcome {
      LaunchOutcome::Launched => RoomDigest::Launched(unit),
      LaunchOutcome::Running => RoomDigest::Running(unit),
      LaunchOutcome::Refused => RoomDigest::Refused(launch.why.unwrap_or_default()),
    })
  }

  /// A room's person book brought up to date before its session is folded away, in the agent's worker of the
  /// room: what the session is about to forget is read through the receiver before it goes. A room the agent's
  /// book is not written from, a one-to-one above all, starts nothing; where that cannot be read, it starts.
  pub async fn people_before_fold(&self, chat_id: i64, agent: &str) -> BeforeFold {
    if !book_room(self.ground.receiver, agent, chat_id).await.unwrap_or(true) {
      return BeforeFold::Skipped;
    }

    let unit = crate::editors::people::worker_unit(chat_id, agent);
    let title = self.room_name(chat_id).await.unwrap_or_else(|_| chat_id.to_string());
    let chat = chat_id.to_string();
    let words = [PEOPLE_WORDS, "--chat", &chat, "--agent", agent];

    match self.launch(&unit, &title, &words).await {
      Launch {
        outcome: LaunchOutcome::Launched,
        ..
      } => BeforeFold::Launched(unit),
      Launch {
        outcome: LaunchOutcome::Running,
        ..
      } => BeforeFold::Running(unit),
      Launch { why, .. } => {
        let why = why.unwrap_or_default();

        eprintln!("{unit} launch failed: {why}");
        BeforeFold::Refused(why)
      }
    }
  }

  /// One agent's book of one room written in its own unit: a room with a whole batch or more waiting worked until
  /// its backlog is gone. A run of it already standing is taken as started; a refusal is answered.
  pub async fn people_worker(
    &self,
    unit: &str,
    chat_id: i64,
    agent: &str,
    catch_up: bool,
  ) -> Result<(), String> {
    let title = self.room_name(chat_id).await.unwrap_or_else(|_| chat_id.to_string());
    let chat = chat_id.to_string();
    let mut words = vec![PEOPLE_WORDS, "--chat", &chat, "--agent", agent];

    if catch_up {
      words.push("--all");
    }

    match self.launch(unit, &title, &words).await {
      Launch {
        outcome: LaunchOutcome::Refused,
        why,
      } => Err(why.unwrap_or_default()),
      _ => Ok(()),
    }
  }

  // The units of these a run stands under, as the agency's alive says; where nobody could say, or the agency could
  // not be asked, every one, so nothing is launched beside a run that may stand.
  async fn held(&self, units: &[String]) -> HashSet<String> {
    if units.is_empty() {
      return HashSet::new();
    }

    match self.ground.agency.alive(units).await {
      Ok(Some(alive)) => alive.into_iter().collect(),
      Ok(None) => units.iter().cloned().collect(),
      Err(why) => {
        eprintln!("brain: which runs stand could not be read: {why}");
        units.iter().cloned().collect()
      }
    }
  }

  /// Whether a run of this unit still stands.
  pub async fn standing(&self, unit: &str) -> bool {
    self.held(&[unit.to_owned()]).await.contains(unit)
  }

  /// Every check no run holds put in a run of its own. A check that could not be launched is left standing for a
  /// later pass.
  pub async fn relaunch_watches(&self, now_ms: i64) -> Result<(), Failure> {
    let active = self.active_watches().await?;
    let units: Vec<String> = active.iter().filter_map(|watch| watch.unit.clone()).collect();
    let held = self.held(&units).await;

    for watch in to_relaunch(&active, &held, now_ms / 1000) {
      if !self.launch_watch(&watch, now_ms).await? {
        eprintln!("watch {} could not be launched, trying again on a later pass", watch.id);
      }
    }

    Ok(())
  }

  /// The watches standing, oldest first.
  pub async fn active_watches(&self) -> Result<Vec<Standing>, Failure> {
    let rows = all(
      self.ground.db,
      r#"SELECT "id", "chat_id", "by_chat", "kind", "unit", "armed_at", "deadline_at", "interval_seconds"
         FROM "watches" WHERE "state" = 'active' ORDER BY "id""#,
      vec![],
    )
    .await?;

    rows
      .iter()
      .map(|row| {
        Ok(Standing {
          id: row.try_get("", "id")?,
          chat_id: row.try_get("", "chat_id")?,
          by_chat: row.try_get("", "by_chat")?,
          kind: row.try_get("", "kind")?,
          unit: row.try_get("", "unit")?,
          armed_at: row.try_get("", "armed_at")?,
          deadline_at: row.try_get("", "deadline_at")?,
          interval_seconds: row.try_get("", "interval_seconds")?,
        })
      })
      .collect()
  }

  /// One watch just set put in a run of its own, by its number: whether the run came up. A watch that no longer
  /// stands has nothing to launch.
  pub async fn launch_watch_of(&self, id: i64, now_ms: i64) -> Result<bool, Failure> {
    match self.active_watches().await?.into_iter().find(|watch| watch.id == id) {
      Some(watch) => self.launch_watch(&watch, now_ms).await,
      None => Ok(false),
    }
  }

  // One watch put in a run of its own: the unit it runs under and the moment it was armed written on the watch,
  // then the run. A launch refused leaves the watch standing for a later pass.
  async fn launch_watch(&self, watch: &Standing, now_ms: i64) -> Result<bool, Failure> {
    let unit = watch_unit(watch.id, now_ms);
    let armed = run(
      self.ground.db,
      r#"UPDATE "watches" SET "unit" = $1, "run_id" = NULL, "armed_at" = $2 WHERE "id" = $3 AND "state" = 'active'"#,
      vec![unit.as_str().into(), (now_ms / 1000).into(), watch.id.into()],
    )
    .await?;

    if armed != 1 {
      return Ok(false);
    }

    let title = format!("{} watch #{}", self.room_name(watch.chat_id).await?, watch.id);
    let id = watch.id.to_string();
    let launch = self.launch(&unit, &title, &[WATCH_WORDS, &id, &unit]).await;

    if launch.outcome == LaunchOutcome::Refused {
      eprintln!("failed to launch {unit}: {}", launch.why.unwrap_or_default());
      return Ok(false);
    }

    Ok(true)
  }
}
