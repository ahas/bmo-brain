//! A process as the editing lock names it: its pid and its start time as the kernel counts it, which tells a
//! pid reused by another process apart from the one that stood a ticket.

use std::fs;

// The fields after the command name, which may itself hold spaces and parentheses.
fn stat_fields(pid: i32) -> Option<Vec<String>> {
  let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
  let (_, rest) = stat.rsplit_once(") ")?;

  Some(rest.split(' ').map(str::to_owned).collect())
}

/// A process's start time, field 22 of its stat line, or none where there is no such process.
pub fn process_birth(pid: i32) -> Option<String> {
  stat_fields(pid)?.get(19).cloned()
}

/// When a process began, in milliseconds since the epoch: the machine's boot time and the process's
/// start in clock ticks after it. The boot time is kept to the second, so the answer may fall up to a
/// second early and never late.
pub fn process_started_ms(pid: i32) -> Option<i64> {
  let boot: i64 = fs::read_to_string("/proc/stat")
    .ok()?
    .lines()
    .find_map(|line| line.strip_prefix("btime ").and_then(|seconds| seconds.trim().parse().ok()))?;
  let ticks: i64 = process_birth(pid)?.parse().ok()?;
  // SAFETY: sysconf has no preconditions.
  let per_second = match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
    asked if asked > 0 => asked,
    _ => 100,
  };

  Some(boot * 1000 + ticks * 1000 / per_second)
}
