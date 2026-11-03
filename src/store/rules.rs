//! What the brain reckons by its rules (docs/design.md, 「Recall」): a text's tokens. The rules themselves are
//! the client's `Rules`, read from the configuration and named in the health answer.

pub use bmo_brain_client::Rules;

/// The tokens a text is reckoned at: one for every three characters, rounded up.
pub fn tokens(text: &str) -> i64 {
  let chars = text.chars().count() as i64;
  (chars + 2) / 3
}
