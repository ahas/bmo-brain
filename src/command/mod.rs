//! The brain's command (docs/boundary-api.md, 「The command」): the nouns `bmo` dispatches to this binary,
//! each noun's words, and the route its words call.

pub mod fragment;
pub mod memory;

use std::io::Read as _;

use bmo_brain_client::Labels;

/// The nouns `bmo` dispatches to this command, each printed one a line as it asks.
pub const NOUNS: &[&str] = &["memory", "fragment"];

/// A moment as the command reads one: an epoch millisecond count, or an RFC 3339 time.
pub fn moment(text: &str) -> Result<i64, String> {
  let text = text.trim();
  if let Ok(ms) = text.parse::<i64>() {
    return Ok(ms);
  }
  chrono::DateTime::parse_from_rfc3339(text)
    .map(|at| at.timestamp_millis())
    .map_err(|_| format!("'{text}' is no moment: give epoch milliseconds or an RFC 3339 time"))
}

/// A moment as the command prints one: RFC 3339 in UTC, to the millisecond.
pub fn moment_text(ms: i64) -> String {
  chrono::DateTime::from_timestamp_millis(ms)
    .map(|at| at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
    .unwrap_or_else(|| format!("{ms} milliseconds"))
}

/// The labels its `key=value` words name.
pub fn labels(words: &[String]) -> Result<Labels, String> {
  words
    .iter()
    .map(|word| {
      let (key, value) =
        word.split_once('=').ok_or_else(|| format!("'{word}' is no label: give key=value"))?;
      if key.is_empty() {
        return Err(format!("'{word}' names no label key"));
      }
      Ok((key.to_owned(), value.to_owned()))
    })
    .collect()
}

/// The labels as one line writes them.
pub fn labels_text(labels: &Labels) -> String {
  labels.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join(",")
}

/// The text a flag gives, or standard input where the flag is `-`.
pub fn text_of(given: &str) -> Result<String, String> {
  if given != "-" {
    return Ok(given.to_owned());
  }
  let mut read = String::new();
  std::io::stdin()
    .read_to_string(&mut read)
    .map_err(|why| format!("standard input did not read: {why}"))?;
  Ok(read)
}
