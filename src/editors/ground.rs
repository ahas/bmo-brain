//! What one editor process stands on (docs/boundary-api.md, 「Configuration」): the brain's database and the
//! address the fragment tool opens it by, the agency, the receiver and the gate, the brain root, the data root and
//! the prompts folder, the chat agent's registration key, the unit the service runs as, and — where the process was
//! launched apart — the unit it runs in and the job run it works for.

use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use super::desk::desk_dir;
use super::fragments::Table;
use super::prompts::Prompts;
use super::settle::journal_record;
use crate::Failure;
use crate::ports::agency::Agency;
use crate::ports::gate::Gate;
use crate::ports::receiver::Receiver;

/// Everything an editor run reaches beside the folder it writes.
#[derive(Clone, Copy)]
pub struct Ground<'a> {
  pub db: &'a DatabaseConnection,
  /// The database's address, which the fragment tool opens a run's desk against.
  pub database: &'a str,
  pub agency: &'a dyn Agency,
  pub receiver: &'a dyn Receiver,
  pub gate: &'a dyn Gate,
  /// The brain root, the chat agent's folder and the one every other agent's folder stands under.
  pub brain_root: &'a Path,
  /// Where the editing locks, the fragment desks and the failure logs live.
  pub data_root: &'a Path,
  /// This repository's prompts: the recollectors' tasks and the rulebooks only they read.
  pub prompts_root: &'a Path,
  /// The chat agent's registration key, whose memory is the brain root.
  pub chat_key: &'a str,
  /// The unit the service runs as, whose journal holds a run that ran in no unit of its own.
  pub service_unit: &'a str,
  /// The unit this process was launched in, none where it runs inside the service or by hand.
  pub unit: Option<&'a str>,
  /// The job run this process works for, whose own hold on the brain never holds it off.
  pub own_run: Option<&'a str>,
}

impl<'a> Ground<'a> {
  /// Where a writer's prompts come from.
  pub fn prompts(&self) -> Prompts<'a> {
    Prompts {
      agency: self.agency,
      root: self.prompts_root,
    }
  }

  /// The fragment table over the brain as the agency registers its folders now.
  pub async fn table(&self) -> Result<Table<'a>, Failure> {
    Table::of(self.db, self.agency, self.receiver, self.brain_root, self.chat_key).await
  }

  /// Where the desks of the runs in flight stand.
  pub fn desks(&self) -> PathBuf {
    desk_dir(self.data_root)
  }

  /// Where the whole account of this process's runs is.
  pub fn record(&self) -> String {
    journal_record(self.unit, self.service_unit)
  }

  /// The name a run of this process is asked about and waited behind under: its unit, or the process itself
  /// where it was started by hand, named for its work.
  pub fn clock_run(&self, work: &str) -> String {
    self.unit.map_or_else(|| format!("{work}:{}", std::process::id()), str::to_owned)
  }

  /// A failure log under the data root.
  pub fn failure_log(&self, name: &str) -> PathBuf {
    self.data_root.join(name)
  }
}
