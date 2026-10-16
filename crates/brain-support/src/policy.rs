//! A crate's declared policy, one TOML file to a part under that crate's own `policy/` folder, compiled in
//! and read once, on first use, into its type; and the one way a policy line's marks are filled.

use serde::de::DeserializeOwned;

/// A line of the policy with each `{name}` it holds filled in, every one at once so a value that itself
/// holds a mark is left as it is; a mark nothing names stays.
pub fn fill_marks(line: &str, values: &[(&str, &str)]) -> String {
  let mut out = String::new();
  let mut rest = line;

  while let Some(open) = rest.find('{') {
    out.push_str(&rest[..open]);

    let after = &rest[open + 1..];
    let name_end = after
      .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
      .unwrap_or(after.len());
    let value = (name_end > 0 && after[name_end..].starts_with('}'))
      .then(|| values.iter().find(|(name, _)| *name == &after[..name_end]))
      .flatten();

    match value {
      Some((_, value)) => {
        out.push_str(value);
        rest = &after[name_end + 1..];
      }
      None => {
        out.push('{');
        rest = after;
      }
    }
  }

  out + rest
}

/// One part read from its file's text. A file that does not read as its type is a fault of the
/// build, which every part's own test meets before anything runs on it.
pub fn read<T: DeserializeOwned>(file: &str, text: &str) -> T {
  toml::from_str(text).unwrap_or_else(|error| panic!("the policy's {file} does not read: {error}"))
}

/// A part of the policy, compiled in from the calling crate's `policy/` folder and read once on first use:
/// `brain_support::part!(PLAY: Play = "play.toml");`
#[macro_export]
macro_rules! part {
  ($vis:vis $name:ident: $type:ty = $file:literal) => {
    $vis static $name: std::sync::LazyLock<$type> = std::sync::LazyLock::new(|| {
      $crate::policy::read($file, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/policy/", $file)))
    });
  };
}
