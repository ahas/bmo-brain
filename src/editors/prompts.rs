//! What the brain's editors are handed to read (docs/boundary-api.md, 「Its prompts」): an editor that writes the
//! brain's pieces reads the document-writing rulebooks, the fragment tool's, the recollectors' and the one against
//! destroying the system before its task. The rulebooks every document writer shares are the agency's; the
//! fragment tool's, the recollection's and the tasks are under this repository's prompts.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::ports::agency::Agency;
use crate::ports::http::PortFailure;

// The rulebooks a writer reads, in order, and whose each is: the agency's, or this repository's own.
const RULEBOOKS: [(&str, bool); 5] = [
  ("document-writing", true),
  ("plain-writing", true),
  ("fragment-tool", false),
  ("recollection", false),
  ("self-preservation", true),
];

/// Why a prompt could not be composed.
#[derive(Debug)]
pub enum PromptFailure {
  /// The agency did not hand over its rulebooks.
  Port(PortFailure),
  /// A document the prompt cannot go out without could not be read.
  Unread { path: PathBuf, why: String },
}

impl fmt::Display for PromptFailure {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Port(failure) => write!(formatter, "the rulebooks could not be read: {failure}"),
      Self::Unread { path, why } => write!(formatter, "{}: {why}", path.display()),
    }
  }
}

impl std::error::Error for PromptFailure {}

impl From<PortFailure> for PromptFailure {
  fn from(failure: PortFailure) -> Self {
    Self::Port(failure)
  }
}

/// Where a writer's prompts come from: the agency for the shared rulebooks, and this repository's prompts folder
/// for the rest.
#[derive(Clone, Copy)]
pub struct Prompts<'a> {
  pub agency: &'a dyn Agency,
  pub root: &'a Path,
}

/// A document a prompt cannot go out without, read whole and trimmed.
pub fn read_required(path: &Path) -> Result<String, PromptFailure> {
  Ok(
    std::fs::read_to_string(path)
      .map_err(|why| PromptFailure::Unread {
        path: path.to_path_buf(),
        why: why.to_string(),
      })?
      .trim()
      .to_owned(),
  )
}

/// A document's `{{name}}` marks filled with what is named, any other left as it stands.
pub fn render(template: &str, values: &[(&str, &str)]) -> String {
  static MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(\w+)\}\}").expect("the mark"));

  MARK
    .replace_all(template, |found: &Captures| {
      values
        .iter()
        .find(|(name, _)| *name == &found[1])
        .map_or_else(|| found[0].to_owned(), |(_, value)| (*value).to_owned())
    })
    .into_owned()
}

/// A task an editor that writes the brain's pieces is handed: the editor's task document under the prompts
/// folder with its marks filled, behind the rulebooks every such editor reads, each headed by its name.
pub async fn fragment_writer(
  prompts: Prompts<'_>,
  editor: &str,
  file: &str,
  values: &[(&str, &str)],
) -> Result<String, PromptFailure> {
  let task = render(&read_required(&prompts.root.join(editor).join(file))?, values);
  let shared: Vec<&str> = RULEBOOKS.iter().filter(|(_, agency)| *agency).map(|(name, _)| *name).collect();
  let handed = prompts.agency.rulebooks(&shared).await?;
  let mut parts = vec![];

  for (name, agency) in RULEBOOKS {
    let text = if agency {
      handed
        .iter()
        .find(|rulebook| rulebook.name == name)
        .map(|rulebook| rulebook.text.trim().to_owned())
        .ok_or_else(|| PromptFailure::Unread {
          path: PathBuf::from(format!("rules/{name}.md")),
          why: "the agency did not hand it over".to_owned(),
        })?
    } else {
      read_required(&prompts.root.join("rules").join(format!("{name}.md")))?
    };

    parts.push(format!("RULEBOOK prompts/rules/{name}.md:\n{text}"));
  }

  parts.push(task);

  Ok(parts.join("\n\n"))
}

/// The moment a message or a note was written, as the editors are shown it: Korean time with its offset, so the
/// day an editor copies onto an item is the day the owner and the rooms lived it.
pub fn seoul_timestamp(epoch_seconds: i64) -> String {
  chrono::DateTime::from_timestamp(epoch_seconds + 9 * 3600, 0)
    .map_or_else(|| epoch_seconds.to_string(), |at| format!("{}+09:00", at.format("%Y-%m-%dT%H:%M:%S")))
}

/// The room titles a common document may not carry, as a writer is told them: each quoted, or "none".
pub fn forbidden_titles_text(titles: &[String]) -> String {
  let mut named: Vec<&str> = vec![];

  for title in titles {
    let title = brain_support::js::trim(title);

    if !title.is_empty() && !named.contains(&title) {
      named.push(title);
    }
  }

  if named.is_empty() {
    "none".to_owned()
  } else {
    named.iter().map(|title| format!("\"{title}\"")).collect::<Vec<_>>().join(", ")
  }
}
