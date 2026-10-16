//! Why a piece of the brain's own work stopped: its database, or a call on another unit.

use std::fmt;

use brain_support::database::Fault;
use sea_orm::DbErr;

use crate::ports::http::PortFailure;

/// The brain's database failed, or a port came to no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
  Database(Fault),
  Port(PortFailure),
}

impl fmt::Display for Failure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Database(fault) => fault.fmt(f),
      Self::Port(failure) => failure.fmt(f),
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
