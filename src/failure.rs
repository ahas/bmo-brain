//! Why a piece of the brain's own work stopped: its database, a call on another unit that came to no answer or
//! was refused, or a file it had to write.

use std::fmt;

use brain_support::database::Fault;
use sea_orm::DbErr;

use crate::ports::http::PortFailure;

/// The brain's database failed, a port came to no answer or refused, or a file under the brain could not be
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
  Database(Fault),
  Port(PortFailure),
  /// The file and why it could not be written.
  Disk(String),
  /// Another unit answered and would not do what was asked: what it was asked.
  Refused(String),
}

impl fmt::Display for Failure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Database(fault) => fault.fmt(f),
      Self::Port(failure) => failure.fmt(f),
      Self::Disk(why) => f.write_str(why),
      Self::Refused(what) => write!(f, "{what} was refused"),
    }
  }
}

impl std::error::Error for Failure {}

impl From<Fault> for Failure {
  fn from(fault: Fault) -> Self {
    Self::Database(fault)
  }
}

impl From<DbErr> for Failure {
  fn from(error: DbErr) -> Self {
    Self::Database(error.into())
  }
}

impl From<PortFailure> for Failure {
  fn from(failure: PortFailure) -> Self {
    Self::Port(failure)
  }
}
