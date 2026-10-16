//! The Seoul clock the scheduled work and the rows it fires from are read on. Seoul keeps no summer time,
//! so its day is a fixed nine hours ahead of UTC.

use chrono::{DateTime, NaiveDateTime};

/// How far Seoul's clock stands ahead of UTC.
pub const OFFSET_SECONDS: i64 = 9 * 60 * 60;

/// A moment as a person in Seoul reads it, `YYYY-MM-DD HH:MM KST`, out of epoch seconds.
pub fn clock(epoch_seconds: i64) -> String {
  let wall = DateTime::from_timestamp(epoch_seconds + OFFSET_SECONDS, 0).unwrap_or_default();

  format!("{} KST", wall.format("%Y-%m-%d %H:%M"))
}

/// A wall-clock moment in Seoul as epoch seconds.
pub fn epoch_of(wall: NaiveDateTime) -> i64 {
  wall.and_utc().timestamp() - OFFSET_SECONDS
}
