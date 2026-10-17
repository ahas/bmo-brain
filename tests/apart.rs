//! A run apart started as a transient unit of the user's service manager (docs/boundary-api.md, 「Runs launched
//! apart」), against stand-ins for the programs that start a unit and say how one stands: the words it is started
//! with, a unit of the name already standing answered as running, and a start refused for any other reason.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use bmo_brain::schedules::apart::{Launch, LaunchOutcome, Starter, UnitLaunch, UserUnits};

// A stand-in program: it writes the words it was given into `<name>-args` beside it, then runs `body`.
fn program(dir: &Path, name: &str, body: &str) -> PathBuf {
  let path = dir.join(name);
  let args = dir.join(format!("{name}-args"));

  fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{body}\n", args.display())).unwrap();
  fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
  path
}

// The service manager stood in for: a start that fails where `run-fails` stands beside it, and a unit's state read
// from `state`, inactive where none is written.
fn manager(dir: &Path) -> UserUnits {
  let fails = dir.join("run-fails");
  let state = dir.join("state");

  UserUnits {
    run: program(
      dir,
      "systemd-run",
      &format!(
        "if [ -e '{}' ]; then echo 'Failed to start transient service unit: Unit bmo-digest.service was already loaded.' >&2; exit 1; fi",
        fails.display()
      ),
    ),
    ctl: program(dir, "systemctl", &format!("cat '{}' 2>/dev/null || echo inactive", state.display())),
  }
}

fn digest() -> UnitLaunch {
  UnitLaunch {
    unit: "bmo-digest".to_owned(),
    title: "All pending notes".to_owned(),
    command: vec!["brain".to_owned(), "digest".to_owned(), "--unit".to_owned(), "bmo-digest".to_owned()],
  }
}

fn said(dir: &Path, name: &str) -> Vec<String> {
  fs::read_to_string(dir.join(format!("{name}-args")))
    .unwrap()
    .lines()
    .map(str::to_owned)
    .collect()
}

#[tokio::test]
async fn a_run_is_started_as_a_transient_unit_named_and_described_by_its_launch() {
  let dir = tempfile::tempdir().unwrap();
  let units = manager(dir.path());

  let launch = units.start(&digest()).await;

  assert_eq!(
    launch,
    Launch {
      outcome: LaunchOutcome::Launched,
      why: None
    }
  );
  assert_eq!(
    said(dir.path(), "systemd-run"),
    [
      "--user",
      "--quiet",
      "--collect",
      "--unit=bmo-digest",
      "--description=All pending notes",
      "--",
      "brain",
      "digest",
      "--unit",
      "bmo-digest"
    ]
  );
}

#[tokio::test]
async fn a_unit_of_the_name_already_standing_is_answered_as_running() {
  let dir = tempfile::tempdir().unwrap();
  let units = manager(dir.path());

  fs::write(dir.path().join("run-fails"), "").unwrap();
  fs::write(dir.path().join("state"), "active\n").unwrap();

  let launch = units.start(&digest()).await;

  assert_eq!(launch.outcome, LaunchOutcome::Running);
  assert_eq!(said(dir.path(), "systemctl"), ["--user", "is-active", "bmo-digest.service"]);

  fs::write(dir.path().join("state"), "deactivating\n").unwrap();
  assert_eq!(units.start(&digest()).await.outcome, LaunchOutcome::Running);
}

#[tokio::test]
async fn a_start_refused_with_no_unit_standing_is_refused_with_what_the_manager_said() {
  let dir = tempfile::tempdir().unwrap();
  let units = manager(dir.path());

  fs::write(dir.path().join("run-fails"), "").unwrap();
  fs::write(dir.path().join("state"), "failed\n").unwrap();

  assert_eq!(
    units.start(&digest()).await,
    Launch {
      outcome: LaunchOutcome::Refused,
      why: Some(
        "Failed to start transient service unit: Unit bmo-digest.service was already loaded.".to_owned()
      ),
    }
  );

  let missing = UserUnits {
    run: dir.path().join("no-such-program"),
    ctl: dir.path().join("no-such-program"),
  };
  let refused = missing.start(&digest()).await;

  assert_eq!(refused.outcome, LaunchOutcome::Refused);
  assert!(refused.why.unwrap().contains("no-such-program"));
}
