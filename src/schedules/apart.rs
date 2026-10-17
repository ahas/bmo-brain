//! How the brain starts a run apart (docs/boundary-api.md, 「Runs launched apart」): one run of its own binary as a
//! transient unit of the user's service manager, named by its unit and described by its title, and collected once
//! it ends. The unit's name is the run's mutex: where a unit of that name already stands, nothing is started.

use std::path::PathBuf;
use std::process::Stdio;

use futures_util::future::BoxFuture;
use tokio::process::Command;

/// A run to start apart: the unit it runs under, what it is titled, and its command's words in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitLaunch {
  pub unit: String,
  pub title: String,
  pub command: Vec<String>,
}

/// What came of a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchOutcome {
  Launched,
  /// A unit of that name stands, so nothing was started.
  Running,
  Refused,
}

/// A start's answer, and why it was refused where it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
  pub outcome: LaunchOutcome,
  pub why: Option<String>,
}

/// What starts the brain's runs apart.
pub trait Starter: Send + Sync {
  fn start<'a>(&'a self, launch: &'a UnitLaunch) -> BoxFuture<'a, Launch>;
}

/// The user's service manager, through the programs that start a transient unit and say how a unit stands.
#[derive(Debug, Clone)]
pub struct UserUnits {
  /// The program that starts a transient unit.
  pub run: PathBuf,
  /// The program that says how a unit stands.
  pub ctl: PathBuf,
}

impl Default for UserUnits {
  fn default() -> Self {
    Self {
      run: PathBuf::from("systemd-run"),
      ctl: PathBuf::from("systemctl"),
    }
  }
}

// The states in which a unit of the name stands, so the name is held.
fn holds_name(state: &str) -> bool {
  !matches!(state, "inactive" | "failed" | "unknown" | "")
}

impl UserUnits {
  // How a unit stands, as the service manager says it: one word, empty where it could not be asked.
  async fn state(&self, unit: &str) -> String {
    let asked = Command::new(&self.ctl)
      .args(["--user", "is-active", &format!("{unit}.service")])
      .stdin(Stdio::null())
      .stderr(Stdio::null())
      .output()
      .await;

    asked
      .map(|asked| String::from_utf8_lossy(&asked.stdout).trim().to_owned())
      .unwrap_or_default()
  }

  async fn started(&self, launch: &UnitLaunch) -> Launch {
    let ran = Command::new(&self.run)
      .args(["--user", "--quiet", "--collect"])
      .arg(format!("--unit={}", launch.unit))
      .arg(format!("--description={}", launch.title))
      .arg("--")
      .args(&launch.command)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .output()
      .await;

    match ran {
      Ok(ran) if ran.status.success() => Launch {
        outcome: LaunchOutcome::Launched,
        why: None,
      },
      Ok(ran) => {
        if holds_name(&self.state(&launch.unit).await) {
          return Launch {
            outcome: LaunchOutcome::Running,
            why: None,
          };
        }

        let said = String::from_utf8_lossy(&ran.stderr).trim().to_owned();

        Launch {
          outcome: LaunchOutcome::Refused,
          why: Some(if said.is_empty() {
            format!("{} ended {}", self.run.display(), ran.status)
          } else {
            said
          }),
        }
      }
      Err(why) => Launch {
        outcome: LaunchOutcome::Refused,
        why: Some(format!("{}: {why}", self.run.display())),
      },
    }
  }
}

impl Starter for UserUnits {
  fn start<'a>(&'a self, launch: &'a UnitLaunch) -> BoxFuture<'a, Launch> {
    Box::pin(self.started(launch))
  }
}
