//! The work the brain launches apart, as its binary runs it (docs/boundary-api.md, 「Runs launched apart」): the memo
//! digest of every room's pending notes or one room's, and the person book in every form a run takes — the launcher,
//! a room's worker, a cut, the sheets' rewrite, a book written again and the sort. The words a route launches a run
//! with and the words the binary reads are one grammar, [`PeopleRun`].

use std::fs::{File, OpenOptions, TryLockError};
use std::time::Duration;

use clap::Args;

use crate::Failure;
use crate::editors::archive::{read_count, unkept_rooms};
use crate::editors::documents::WholeRoomRun;
use crate::editors::ground::Ground;
use crate::editors::lock::{HOLD_POLL, brain_held, brain_lock_path, wait_for_brain};
use crate::editors::memo::MemoDigest;
use crate::editors::people::batches::RETRY_DELAY;
use crate::editors::people::launcher::{Launcher, wall_clock};
use crate::editors::people::sort::{Failures, sort_people, sort_status};
use crate::editors::people::{PeopleDigest, Stopped};
use crate::editors::recollection::{recollection_for, recollections};
use crate::editors::settle::resting;
use crate::schedules::launches::Launches;

/// The words a run of the person book is launched with and read back by: the room, the agent whose book it is,
/// and the form.
#[derive(Args, Clone, Debug, Default, PartialEq, Eq)]
pub struct PeopleRun {
  /// The room; without it, the launcher of every room with enough waiting, or the sort.
  #[arg(long, allow_negative_numbers = true)]
  pub chat: Option<i64>,
  /// The agent whose book it is; the chat agent where none is named.
  #[arg(long)]
  pub agent: Option<String>,
  /// A room worked until its backlog is gone; with the sort, every person.
  #[arg(long)]
  pub all: bool,
  /// The room's older single documents cut into pieces.
  #[arg(long)]
  pub split: bool,
  /// The room's standing sheets written again from its pieces.
  #[arg(long)]
  pub sheets: bool,
  /// The room's book written again from the messages already folded.
  #[arg(long)]
  pub reform: bool,
  /// The room pieces not yet sorted, sorted into the two parts.
  #[arg(long)]
  pub sort: bool,
  /// One person rather than the whole room, or every person.
  #[arg(long)]
  pub person: Option<i64>,
  /// How many stretches a cut takes on.
  #[arg(long)]
  pub spans: Option<i64>,
  /// How many batches a book written again may take.
  #[arg(long)]
  pub batches: Option<i64>,
  /// How many runs a sort takes on.
  #[arg(long)]
  pub groups: Option<i64>,
}

impl PeopleRun {
  /// The run's words, after the binary's own: the launcher as `people-digest` alone, the sort as `people-digest
  /// --sort (--all | --person <userId>) --agent <agent> [--groups <n>]`, and a room's work as `people-digest --chat
  /// <chatId> --agent <agent>` and its flags.
  pub fn words(&self, agent: &str) -> Vec<String> {
    let mut words = vec!["people-digest".to_owned()];
    let mut flag = |name: &str, value: Option<String>| {
      words.push(format!("--{name}"));
      words.extend(value);
    };

    if self.sort {
      flag("sort", None);

      match self.person {
        Some(person) => flag("person", Some(person.to_string())),
        None => flag("all", None),
      }

      flag("agent", Some(agent.to_owned()));

      if let Some(groups) = self.groups {
        flag("groups", Some(groups.to_string()));
      }

      return words;
    }

    let Some(chat) = self.chat else { return words };

    flag("chat", Some(chat.to_string()));
    flag("agent", Some(agent.to_owned()));

    if self.all {
      flag("all", None);
    }

    if self.split {
      flag("split", None);

      if let Some(person) = self.person {
        flag("person", Some(person.to_string()));
      }

      if let Some(spans) = self.spans {
        flag("spans", Some(spans.to_string()));
      }
    }

    if self.sheets {
      flag("sheets", None);
    }

    if self.reform {
      flag("reform", None);

      if let Some(batches) = self.batches {
        flag("batches", Some(batches.to_string()));
      }
    }

    words
  }
}

/// The agent whose memory is the brain root: the one the agency registers there, or the chat agent's key where none
/// is registered there yet.
pub async fn root_agent(ground: &Ground<'_>) -> Result<String, Failure> {
  Ok(
    recollections(ground.agency, ground.brain_root, ground.chat_key)
      .await?
      .into_iter()
      .find(|recollection| recollection.folder.is_empty())
      .map_or_else(|| ground.chat_key.to_owned(), |recollection| recollection.agent),
  )
}

/// Every agent's pending notes, room by room, or one room's: the process's exit code.
pub async fn memo(ground: Ground<'_>, chat: Option<i64>) -> Result<u8, Failure> {
  MemoDigest { ground }.run(chat).await
}

// The editing lock of the agent's folder held for a whole-room run, from outside the queue as a cut, a sheets'
// rewrite or a reform holds it: a job holding the brain waited out before it is taken, and let go and waited out
// again where a hold was set meanwhile.
async fn hold_whole_room(ground: &Ground<'_>, folder: &str, who: &str) -> std::io::Result<File> {
  let path = brain_lock_path(ground.data_root, folder);

  loop {
    wait_for_brain(ground.db, ground.agency, who, ground.own_run, HOLD_POLL).await;

    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    loop {
      match file.try_lock() {
        Ok(()) => break,
        Err(TryLockError::WouldBlock) => tokio::time::sleep(Duration::from_millis(500)).await,
        Err(TryLockError::Error(why)) => return Err(why),
      }
    }

    if !brain_held(ground.db, ground.agency, ground.own_run).await {
      return Ok(file);
    }

    let _ = file.unlock();
  }
}

/// One run of the person book, in the form its words name: whether it failed. A usage limit stops it, its agency
/// having rested the login and told the owner.
pub async fn people(launches: &Launches<'_>, run: &PeopleRun) -> Result<bool, Failure> {
  let ground = launches.ground;
  let agent = match &run.agent {
    Some(agent) => agent.clone(),
    None => root_agent(&ground).await?,
  };
  let Some(recollection) =
    recollection_for(ground.agency, ground.brain_root, ground.chat_key, &agent).await?
  else {
    eprintln!("people digest: {agent} keeps no long-term memory");
    return Ok(true);
  };
  let unkept = unkept_rooms(ground.receiver, &recollection.brain).await?;
  let whole_room = run.chat.filter(|_| run.split || run.sheets || run.reform);
  // A cut, a sheets' rewrite and a reform hold the folder's lock for their whole run.
  let held = match whole_room {
    Some(chat) => Some(
      hold_whole_room(&ground, &recollection.folder, &format!("people digest: chat {chat}"))
        .await
        .map_err(|why| Failure::Disk(format!("the person book's lock: {why}")))?,
    ),
    None => None,
  };

  // Every batch is a full high-effort session; during a rest none of them may start.
  if resting(ground.agency).await? {
    return Ok(false);
  }

  recollection
    .stand()
    .map_err(|why| Failure::Disk(format!("{}: {why}", recollection.brain.display())))?;

  let digest = PeopleDigest {
    ground,
    recollection,
    catch_up: run.all,
    unkept_rooms: unkept.clone(),
  };
  let budget = |given: Option<i64>| given.map_or(usize::MAX, |given| usize::try_from(given).unwrap_or(0));
  let worked: Result<bool, Stopped> = async {
    if run.sort {
      let people = match run.person {
        Some(person) => vec![person],
        None => {
          sort_status(&digest.recollection.brain, &mut Failures { db: ground.db }, &unkept)
            .await?
            .1
        }
      };

      return sort_people(&mut &digest, &people, budget(run.groups)).await;
    }

    let Some(chat) = run.chat else {
      return Ok(
        Launcher {
          launches,
          clock: &wall_clock,
        }
        .launch_workers()
        .await?,
      );
    };

    if run.reform {
      let folded = read_count(ground.receiver, &agent, chat).await?;
      let failed = digest.reform_chat(chat, run.batches.unwrap_or(0)).await?;

      return Ok(digest.whole_room(chat, WholeRoomRun::Rewrite { folded }, failed).await?);
    }

    if run.sheets {
      let failed = digest.sheets_for_chat(chat).await?;

      return Ok(digest.whole_room(chat, WholeRoomRun::Sheets, failed).await?);
    }

    if run.split {
      let failed = digest.split_chat(chat, run.person, budget(run.spans)).await?;

      return Ok(digest.whole_room(chat, WholeRoomRun::Cut { person: run.person }, failed).await?);
    }

    digest.digest_chat(chat, RETRY_DELAY).await
  }
  .await;

  if let Some(held) = held {
    let _ = held.unlock();
  }

  match worked {
    Ok(failed) => Ok(failed),
    Err(Stopped::Limit(limit)) => {
      eprintln!("people digest: run {} reached a usage limit, stopping", limit.run);
      Ok(true)
    }
    Err(Stopped::Failed(why)) => {
      eprintln!("people digest: stopped on an error: {why}");
      Ok(true)
    }
  }
}
