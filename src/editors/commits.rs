//! The brain's own git repository as the editors write it (docs/people-book.md, 「Where it lives」): what a run
//! wrote committed by the paths it names, a folder put back to its last commit, and a removed folder's removal
//! committed. The brain has no remote, so nothing is pushed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// Two writers may commit into the brain side by side, each under its own folder's lock, and git lets one command
// at a time hold the repository's index, so a command that met the index taken is run again after a pause.
const INDEX_TRIES: u32 = 5;
const INDEX_PAUSE: Duration = Duration::from_secs(1);

// How long a commit that met another writer's commit waits before it tries again.
const COMMIT_RETRY: Duration = Duration::from_secs(3);

/// Git run once in the repository, under the process's own environment with a home where it was started without
/// one, since a git with no home reads no identity and refuses every commit. Answers what it said, as success or
/// failure.
pub fn git_once(root: &Path, args: &[&str]) -> Result<String, String> {
  let mut command = Command::new("git");

  command.arg("-C").arg(root).args(args).stdin(std::process::Stdio::null());

  if std::env::var_os("HOME").is_none_or(|home| home.is_empty())
    && let Some(home) = account_home()
  {
    command.env("HOME", home);
  }

  let done = command.output().map_err(|why| why.to_string())?;
  let said = format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr))
    .trim()
    .to_owned();

  if done.status.success() { Ok(said) } else { Err(said) }
}

/// Git run in the repository, again after a pause while another command holds its index.
pub fn git(root: &Path, args: &[&str]) -> Result<String, String> {
  let mut tries = 1;

  loop {
    match git_once(root, args) {
      Err(said) if said.contains("index.lock") && tries < INDEX_TRIES => {
        tries += 1;
        std::thread::sleep(INDEX_PAUSE);
      }
      done => return done,
    }
  }
}

// The account's home, read from the password database where the environment names none.
fn account_home() -> Option<PathBuf> {
  // SAFETY: getpwuid answers a pointer into static storage or null; it is read at once.
  unsafe {
    let entry = libc::getpwuid(libc::getuid());

    if entry.is_null() || (*entry).pw_dir.is_null() {
      return None;
    }

    Some(PathBuf::from(std::ffi::CStr::from_ptr((*entry).pw_dir).to_string_lossy().into_owned()))
  }
}

/// Removes a file or a folder.
pub fn remove(path: &Path) -> std::io::Result<()> {
  if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

/// Puts a folder of the brain back to its last commit: what a stopped run wrote and left unwritten there goes, as
/// the run's own failure would have put it back.
pub fn put_back(brain: &Path, folder: &str) -> Result<(), String> {
  if !brain.join(folder).exists() {
    return Ok(());
  }

  git(brain, &["checkout", "--quiet", "HEAD", "--", folder])
    .or_else(|said| if said.contains("did not match") { Ok(said) } else { Err(said) })?;
  git(brain, &["clean", "-fdq", "--", folder])?;

  Ok(())
}

/// Commits a folder's removal in the brain. Says what came of it: `committed`, or `nothing` where the folder was
/// never kept.
pub fn commit_removal(brain: &Path, folder: &str, message: &str) -> Result<&'static str, String> {
  if !brain.join(folder).exists() && git(brain, &["ls-files", "--", folder])?.is_empty() {
    return Ok("nothing");
  }

  git(brain, &["add", "--all", "--", folder])?;

  if git(brain, &["diff", "--cached", "--quiet", "--", folder]).is_ok() {
    return Ok("nothing");
  }

  git(brain, &["commit", "--quiet", "-m", message, "--", folder])?;

  Ok("committed")
}

fn holds_a_file(path: &Path) -> bool {
  if path.is_file() {
    return true;
  }

  let Ok(entries) = fs::read_dir(path) else { return false };

  entries.flatten().any(|entry| holds_a_file(&entry.path()))
}

/// How a commit of what an editor wrote came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Committed {
  Committed,
  Nothing,
  Failed,
}

fn first_words(said: &str) -> String {
  said.chars().take(200).collect()
}

/// What an editor wrote committed into the repository at `root`. Only the paths named are staged, so a hand edit
/// beside them is left alone; a path that holds no file and nothing git tracks is left out, since git refuses a
/// whole commit on one path that matches nothing. A commit that meets another's index lock tries once more after a
/// pause. Nothing here fails the caller: a commit that cannot be made is logged, and the documents stay for the
/// next run to commit.
pub async fn commit_paths(named: &[String], message: &str, root: &Path) -> Committed {
  let paths: Vec<&str> = named
    .iter()
    .map(String::as_str)
    .filter(|path| {
      holds_a_file(&root.join(path))
        || git_once(root, &["ls-files", "--", path]).is_ok_and(|said| !said.is_empty())
    })
    .collect();

  if paths.is_empty() {
    return Committed::Nothing;
  }

  for attempt in 1..=2 {
    match git_once(root, &[&["add", "--"][..], &paths].concat()) {
      Ok(_) => {
        if git_once(root, &[&["diff", "--cached", "--quiet", "--"][..], &paths].concat()).is_ok() {
          return Committed::Nothing;
        }

        match git_once(root, &[&["commit", "--quiet", "-m", message, "--"][..], &paths].concat()) {
          Ok(_) => return Committed::Committed,
          Err(said) => eprintln!("commit failed (attempt {attempt}): {}", first_words(&said)),
        }
      }
      Err(said) => eprintln!("git add failed (attempt {attempt}): {}", first_words(&said)),
    }

    tokio::time::sleep(COMMIT_RETRY).await;
  }

  Committed::Failed
}
