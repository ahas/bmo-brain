//! Files as the editing lock writes its fence: readable by the owner alone, and published whole by a rename,
//! so a reader never sees one half written.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// A file created or emptied, readable by the owner alone.
pub fn private_file(path: &Path) -> io::Result<fs::File> {
  OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)
}

/// Bytes written beside `path` and renamed onto it.
pub fn publish_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
  let mut staged = path.as_os_str().to_owned();

  staged.push(".new");

  let staged = Path::new(&staged);

  private_file(staged)?.write_all(bytes)?;
  fs::rename(staged, path)
}
