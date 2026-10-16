//! A moment read on Seoul's clock, and a Seoul wall-clock moment read back as epoch seconds.

use brain_support::clock::{OFFSET_SECONDS, clock, epoch_of};
use chrono::NaiveDate;

// 2026-10-07 00:30 KST is 2026-10-06 15:30 UTC.
const LATE_EVENING_UTC: i64 = 1_791_300_600;

#[test]
fn a_moment_is_read_on_seouls_clock() {
  assert_eq!(clock(LATE_EVENING_UTC), "2026-10-07 00:30 KST");
  assert_eq!(clock(-OFFSET_SECONDS), "1970-01-01 00:00 KST");
}

#[test]
fn a_seoul_wall_clock_moment_is_nine_hours_ahead_of_utc() {
  let wall = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap().and_hms_opt(0, 30, 0).unwrap();

  assert_eq!(epoch_of(wall), LATE_EVENING_UTC);
  assert_eq!(clock(epoch_of(wall)), "2026-10-07 00:30 KST");
}
