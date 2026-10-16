//! What an editor batch says of a failure (docs/knowledge.md, 「When a run fails」).

/// The cause of a failure the run could not name: that it is unknown, and where to read it.
pub fn unknown_cause(record: &str) -> String {
  format!("The cause is not known from what the run left; the whole of it is in {record}.")
}
