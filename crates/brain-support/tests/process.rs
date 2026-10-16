//! A process's start time read from the kernel: this process's falls before now and not long before, and a pid
//! with no process has none.

use std::time::{SystemTime, UNIX_EPOCH};

use brain_support::process::{process_birth, process_started_ms};

fn now_ms() -> i64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

#[test]
fn this_process_began_before_now() {
  let pid = i32::try_from(std::process::id()).unwrap();
  let began = process_started_ms(pid).expect("this process has a start time");
  let now = now_ms();

  assert!(began <= now, "began {began} after now {now}");
  assert!(now - began < 3_600_000, "began {began} more than an hour before now {now}");
  assert!(process_birth(pid).is_some_and(|ticks| ticks.parse::<u64>().is_ok()));
}

#[test]
fn a_pid_with_no_process_has_no_start_time() {
  assert_eq!(process_birth(-1), None);
  assert_eq!(process_started_ms(-1), None);
}
