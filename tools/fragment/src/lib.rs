//! The editors' fragment desk, the entries of the `fragment` tool in the `fragments` namespace (docs/fragments.md,
//! 「An editor writes the row through the fragment tool」). Each call names the desk the session's record holds and
//! is answered through the brain's desk routes, which read the desk afresh and save it whole, or leave it as it
//! stood on a refusal. Nothing here writes the table.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tool_call::{Answer, Asked, AtLeastOne, Brain, Entry, Handed, Positive, Said, Session, input_schema};

const ON_THE_DESK: &[&str] = &["fragments.desk"];

const NO_DESK: &str =
  "This run has no fragment desk: the tool serves an editor batch's run, and nothing else.";

const fn entry(
  name: &'static str,
  title: &'static str,
  description: &'static str,
  input: &'static str,
  input_schema: fn() -> Value,
  hints: &'static [(&'static str, bool)],
) -> Entry {
  Entry {
    namespace: "fragments",
    name,
    title: Some(title),
    description,
    input,
    input_schema,
    output: None,
    capabilities: ON_THE_DESK,
    hints,
    grant: None,
  }
}

/// Every entry the executable answers, as its manifest declares them.
pub const ENTRIES: &[Entry] = &[
  entry(
    "fragment_search",
    "Search the fragment rows",
    "The rows of the pieces this run may reach, each as a line with its slug, title, path, keywords, items, period and status, with this run's own writes laid over them. Search before you create a piece: a matter a row already holds goes into that piece.",
    "fragment_search.json",
    input_schema::<SearchInput>,
    &[("read_only", true)],
  ),
  entry(
    "fragment_write",
    "Write a piece's row",
    "Writes the row of a piece whose body you have just created or changed with your file tools. Call it for every piece you create or change, again whenever its body changes. A new piece whose matter a row already holds is refused with those rows named.",
    "fragment_write.json",
    input_schema::<WriteInput>,
    &[("idempotent", true)],
  ),
  entry(
    "fragment_remove",
    "Remove a piece",
    "Deletes a piece's file and takes its row down. Your file tools cannot delete a file; this is how a piece left with nothing in it goes.",
    "fragment_remove.json",
    input_schema::<RemoveInput>,
    &[("destructive", true)],
  ),
  entry(
    "fragment_merge",
    "Merge pieces",
    "Folds pieces into one after you have written the merged body into the kept piece: writes the kept piece's row and removes every piece folded into it. Keywords and items left out are those of all the pieces together.",
    "fragment_merge.json",
    input_schema::<MergeInput>,
    &[("destructive", true)],
  ),
  entry(
    "fragment_split",
    "Split a piece",
    "Cuts one piece into several after you have written their bodies: writes a row for each piece that comes out of it and removes the original unless it is one of them. What comes out of a standing piece is not held against other rows.",
    "fragment_split.json",
    input_schema::<SplitInput>,
    &[("destructive", true)],
  ),
  entry(
    "fragment_owed",
    "Pieces still owed a row",
    "The pieces this run created or changed on disk and has written no row for yet. A run that ends with any of them fails, so ask before you finish.",
    "fragment_owed.json",
    input_schema::<OwedInput>,
    &[("read_only", true)],
  ),
];

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
  Open,
  Done,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchInput {
  /// Words any of which the row's slug, path, title or keywords carry. Left out, every row matches.
  text: Option<String>,
  /// Only rows tied to this TODO item.
  item: Option<Positive>,
  /// Only the rows of this person's book, by user id.
  person: Option<i64>,
  /// Only rows whose matter is still open, or finished.
  status: Option<Status>,
  /// Show the rows that are down as well.
  down: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WriteInput {
  /// The piece's path relative to the folder you work in, exactly as you wrote it.
  path: String,
  /// One or two sentences, 40 to 320 characters: what kind of document the piece is and the situations a turn needs it for. Never a list of nouns.
  title: String,
  /// One text of three to ten words, separated by commas, that somebody would ask for this piece by and that are particular to it: the Korean words the room uses for the matter, names, product and system names, identifiers. None of the words every turn carries.
  keywords: String,
  /// The TODO item numbers the piece is about (#758 is 758); [] for none.
  items: Vec<Positive>,
  /// On a knowledge chunk only: true for a chunk every room may read, false for one of this room alone.
  shared: Option<bool>,
  /// On a knowledge chunk or a person-book piece: true for a fact every agent is better for knowing, which every agent may be offered; false for a memory of this agent's own. Left out, the row keeps what it says, and a new one is this agent's own.
  for_every_agent: Option<bool>,
  /// Paths of the rows a refusal named that you have read and that hold another matter than this new piece.
  distinct_from: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveInput {
  /// The piece's path relative to the folder you work in.
  path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MergeInput {
  /// The kept piece's path, whose body now holds the merged matter.
  into: String,
  /// The pieces folded into it, which are removed.
  from: AtLeastOne<String>,
  /// One or two sentences, 40 to 320 characters: what kind of document the piece is and the situations a turn needs it for. Never a list of nouns.
  title: String,
  /// One text of three to ten words, separated by commas, that somebody would ask for this piece by and that are particular to it: the Korean words the room uses for the matter, names, product and system names, identifiers. None of the words every turn carries.
  keywords: Option<String>,
  /// The TODO item numbers the piece is about (#758 is 758); [] for none.
  items: Option<Vec<Positive>>,
  /// On a knowledge chunk only: true for a chunk every room may read, false for one of this room alone.
  shared: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PieceInput {
  path: String,
  /// One or two sentences, 40 to 320 characters: what kind of document the piece is and the situations a turn needs it for. Never a list of nouns.
  title: String,
  /// One text of three to ten words, separated by commas, that somebody would ask for this piece by and that are particular to it: the Korean words the room uses for the matter, names, product and system names, identifiers. None of the words every turn carries.
  keywords: String,
  /// The TODO item numbers the piece is about (#758 is 758); [] for none.
  items: Vec<Positive>,
  /// On a knowledge chunk only: true for a chunk every room may read, false for one of this room alone.
  shared: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SplitInput {
  /// The piece that was cut.
  from: String,
  /// Every piece that comes out of it, the original included where it stays.
  pieces: AtLeastOne<PieceInput>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OwedInput {}

// What a desk route answered, as the tool answers it: its text, its refusal headed as one, or why the brain could
// not answer.
fn answered(said: Result<Said, String>) -> Answer {
  match said {
    Ok(Said::Reading(lines)) => Answer::text(lines.join("\n")),
    Ok(Said::Refused { message, .. }) => Answer::refusal(format!("REFUSED: {message}")),
    Ok(Said::Object(_)) => Answer::failure("the brain answered no reading"),
    Err(why) => Answer::failure(why),
  }
}

fn numbers(items: &[Positive]) -> Vec<i64> {
  items.iter().map(|item| item.0).collect()
}

fn read<T: serde::de::DeserializeOwned>(name: &str, arguments: Value) -> Result<T, Answer> {
  serde_json::from_value(arguments)
    .map_err(|why| Answer::failure(format!("invalid arguments: {name}: {why}")))
}

/// One call of an entry, by its name, with its arguments, answered through the brain the session's record reaches.
pub async fn call(session: &Session, name: &str, arguments: Value) -> Answer {
  let brain = match Brain::of(session) {
    Ok(brain) => brain,
    Err(why) => return Answer::failure(why),
  };

  call_on(&brain, session, name, arguments).await
}

/// One call of an entry answered through this brain, on the desk the session's record names.
pub async fn call_on(brain: &Brain, session: &Session, name: &str, arguments: Value) -> Answer {
  let Some(desk) = session.fragment_desk.as_ref().filter(|path| !path.as_os_str().is_empty()) else {
    return Answer::refusal(format!("REFUSED: {NO_DESK}"));
  };
  let desk = desk.to_string_lossy().into_owned();
  let asked = async {
    let (route, body) = match name {
      "fragment_search" => {
        let input: SearchInput = read(name, arguments)?;

        (
          "/fragments/desk/search",
          json!({
            "desk": desk, "text": input.text, "item": input.item.map(|item| item.0), "person": input.person,
            "status": input.status, "down": input.down,
          }),
        )
      }
      "fragment_write" => {
        let input: WriteInput = read(name, arguments)?;

        (
          "/fragments/desk/write",
          json!({
            "desk": desk, "path": input.path, "title": input.title, "keywords": input.keywords,
            "items": numbers(&input.items), "shared": input.shared, "forEveryAgent": input.for_every_agent,
            "distinctFrom": input.distinct_from,
          }),
        )
      }
      "fragment_remove" => {
        let input: RemoveInput = read(name, arguments)?;

        ("/fragments/desk/remove", json!({ "desk": desk, "path": input.path }))
      }
      "fragment_merge" => {
        let input: MergeInput = read(name, arguments)?;

        (
          "/fragments/desk/merge",
          json!({
            "desk": desk, "into": input.into, "from": input.from.0, "title": input.title,
            "keywords": input.keywords, "items": input.items.as_deref().map(numbers), "shared": input.shared,
          }),
        )
      }
      "fragment_split" => {
        let input: SplitInput = read(name, arguments)?;
        let pieces: Vec<Value> = input
          .pieces
          .0
          .iter()
          .map(|piece| {
            json!({
              "path": piece.path, "title": piece.title, "keywords": piece.keywords,
              "items": numbers(&piece.items), "shared": piece.shared,
            })
          })
          .collect();

        ("/fragments/desk/split", json!({ "desk": desk, "from": input.from, "pieces": pieces }))
      }
      "fragment_owed" => {
        let _: OwedInput = read(name, arguments)?;

        ("/fragments/desk/owed", json!({ "desk": desk }))
      }
      other => return Err(Answer::failure(format!("this executable holds no entry {other}"))),
    };

    Ok::<Answer, Answer>(answered(brain.post(route, &body).await))
  };

  asked.await.unwrap_or_else(|refused| refused)
}

/// One request answered: a call of an entry by its full name, resource requests answered as a tool that serves
/// none, and any other kind refused.
pub async fn answer(handed: Handed) -> Result<Value, String> {
  let answered = match handed.asked {
    Asked::Call {
      entry: asked,
      arguments,
    } => match asked.strip_prefix("fragments/") {
      Some(name) if ENTRIES.iter().any(|entry| entry.name == name) => {
        call(&handed.session, name, arguments).await
      }
      _ => Answer::failure(format!("this executable holds no entry {asked}")),
    },
    Asked::ResourcesList => return Ok(json!({ "resources": [] })),
    Asked::ResourcesRead { .. } => return Ok(json!({ "error": "this executable serves no resources" })),
    Asked::Other => return Err("this executable answers calls alone".to_owned()),
  };

  serde_json::to_value(answered).map_err(|why| format!("the answer could not be written: {why}"))
}
