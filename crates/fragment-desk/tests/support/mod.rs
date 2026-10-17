//! A library of the tests' own, handed to the desk as its caller hands it the brain's: the extension point's
//! members are whatever the caller makes them.

#![allow(dead_code)]

use fragment_desk::libraries::{Library, PiecePlace};

/// The pieces under one folder, each `<prefix><slug>.md` but its index; a scoped one carries the scope its
/// writer said, every room's or the batch's room, and stands as no row where neither is known.
pub struct Under {
  pub kind: &'static str,
  pub prefix: &'static str,
  pub scoped: bool,
  pub pieces: &'static str,
  pub not_pieces: &'static [&'static str],
}

impl Under {
  fn slug(&self, path: &str) -> Option<String> {
    let slug = path.strip_prefix(self.prefix)?.strip_suffix(".md")?;

    (!slug.is_empty() && slug != "index" && !slug.contains('/')).then(|| slug.to_owned())
  }
}

impl Library for Under {
  fn kind(&self) -> &'static str {
    self.kind
  }

  fn owns(&self, path: &str) -> bool {
    self.slug(path).is_some()
  }

  fn piece_place(&self, path: &str, shared: Option<bool>, room: Option<i64>) -> Option<PiecePlace> {
    let slug = self.slug(path)?;
    let scope = if !self.scoped {
      None
    } else if shared == Some(true) {
      Some("[shared]".to_owned())
    } else {
      Some(format!("[chat:{}]", room?))
    };

    Some(PiecePlace {
      owner: None,
      slug,
      scope,
    })
  }

  fn pieces(&self) -> &'static str {
    self.pieces
  }

  fn not_pieces(&self) -> &'static [&'static str] {
    self.not_pieces
  }
}

/// A rules library and a notes library whose scope its writer chooses, in that order.
pub static RULES: Under = Under {
  kind: "room-rule",
  prefix: "rooms/-100/rules/",
  scoped: false,
  pieces: "a rule",
  not_pieces: &["an index.md"],
};

pub static NOTES: Under = Under {
  kind: "library-knowledge",
  prefix: "knowledge/",
  scoped: true,
  pieces: "a note",
  not_pieces: &["notes/candidates.md", "an index.md"],
};

/// The two, as a caller hands them in.
pub fn libraries() -> Vec<&'static dyn Library> {
  vec![&RULES, &NOTES]
}
