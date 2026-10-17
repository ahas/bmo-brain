//! The fragment tool's routes (docs/fragments.md, 「An editor writes the row through the fragment tool」;
//! docs/boundary-api.md, 「Fragments」): an editor run's calls on its desk. Each call reads the desk the run's session
//! names afresh; a call that writes saves the desk whole, or leaves it as it stood on a refusal. Nothing here writes
//! the table: the batch reads the desk back once the run ends.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use brain_support::ceiling::{Said, cap_items};
use fragment_desk::desk::{DeskError, Desked};
use fragment_desk::ops::{Open, Query, WriteRequest};
use fragment_desk::rows::AgentFolder;
use serde::Deserialize;
use serde_json::Map;

use super::{Answer, Api, ApiError, Body, reading, refusal};
use crate::editors::desk::desk_dir;
use crate::editors::fragments::libraries::{Brain, on_the_desk};

/// The fragment tool's routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/fragments/desk/search", post(search))
    .route("/fragments/desk/write", post(write))
    .route("/fragments/desk/remove", post(remove))
    .route("/fragments/desk/merge", post(merge))
    .route("/fragments/desk/split", post(split))
    .route("/fragments/desk/owed", post(owed))
}

const NO_DESK: &str =
  "This run has no fragment desk: the tool serves an editor batch's run, and nothing else.";

/// The open desk a call names, as the session start handed it: a desk file under the data root's desks, and only
/// one that is still there.
pub fn desk_named(data_root: &Path, named: &str) -> Option<PathBuf> {
  let path = Path::new(named);
  let dir = desk_dir(data_root);
  let name = path.file_name()?.to_str()?;
  let stem = name.strip_suffix(".json")?;
  let shaped = stem.len() == 32 && stem.bytes().all(|byte| byte.is_ascii_hexdigit());

  (shaped && path.parent() == Some(dir.as_path()) && path.is_file()).then(|| path.to_path_buf())
}

// What a desk call came to, as the route answers it: its text as a reading, its refusal by code, or the wire's
// failure.
fn answered(done: Desked<String>) -> Answer {
  match done {
    Ok(said) => Ok(reading(said.split('\n').map(str::to_owned).collect())),
    Err(DeskError::Refused(refused)) => Ok(refusal(refused.code, &refused.message, Map::new(), vec![])),
    Err(DeskError::Failed(why)) => Err(ApiError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      code: "failed",
      error: why,
      refusal: None,
    }),
  }
}

// The desk a call names, opened over every awake agent's folder as the agency registers it now.
async fn opened(api: &Api, named: &str) -> Result<Desked<(PathBuf, Open)>, ApiError> {
  let Some(path) = desk_named(&api.data_root, named) else {
    return Ok(Err(DeskError::refused("no-desk", NO_DESK)));
  };
  let brain = Brain::of(api.agency.as_ref(), &api.brain_root, &api.chat_key).await?;
  let folders = brain
    .folders
    .into_iter()
    .map(|one| AgentFolder {
      agent: one.agent,
      folder: one.folder,
    })
    .collect();

  Ok(Open::at(&path, folders, on_the_desk()).await.map(|open| (path, open)))
}

// A call that writes: the desk is read, changed and saved whole, or left as it was on a refusal.
async fn on_desk(api: &Api, named: &str, work: impl FnOnce(&mut Open) -> Desked<String>) -> Answer {
  answered(opened(api, named).await?.and_then(|(path, mut open)| {
    let said = work(&mut open)?;

    open.desk.save(&path)?;

    Ok(said)
  }))
}

// The keywords as a write keeps them: one text, separated by commas.
fn keyword_list(said: &str) -> Vec<String> {
  said.split(',').map(str::to_owned).collect()
}

// Item numbers as the tool schemas take them: each a positive whole number.
fn items(given: &[i64]) -> Result<Vec<i64>, ApiError> {
  if given.iter().any(|item| *item < 1) {
    return Err(ApiError::bad_request("items are TODO item numbers, each a positive whole number"));
  }

  Ok(given.to_vec())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Status {
  Open,
  Done,
}

#[derive(Debug, Deserialize)]
struct SearchAsked {
  desk: String,
  text: Option<String>,
  item: Option<i64>,
  person: Option<i64>,
  status: Option<Status>,
  down: Option<bool>,
}

async fn search(State(api): State<Arc<Api>>, Body(asked): Body<SearchAsked>) -> Answer {
  if asked.item.is_some_and(|item| item < 1) {
    return Err(ApiError::bad_request("item is a TODO item number, a positive whole number"));
  }

  let query = Query {
    text: asked.text,
    item: asked.item,
    person: asked.person,
    status: asked.status.map(|status| match status {
      Status::Open => "open".to_owned(),
      Status::Done => "done".to_owned(),
    }),
    down: asked.down == Some(true),
  };

  answered(opened(&api, &asked.desk).await?.and_then(|(_, open)| open.search(&query)).map(|lines| {
    if lines.is_empty() {
      return "No row matches.".to_owned();
    }

    let capped = cap_items(
      &lines,
      |kept| kept.join("\n"),
      Said::new("rows", "Narrow the search with more particular words, an item or a person."),
    );

    capped.kept.iter().cloned().chain(capped.note).collect::<Vec<_>>().join("\n")
  }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteAsked {
  desk: String,
  path: String,
  title: String,
  keywords: String,
  items: Vec<i64>,
  shared: Option<bool>,
  for_every_agent: Option<bool>,
  distinct_from: Option<Vec<String>>,
}

async fn write(State(api): State<Arc<Api>>, Body(asked): Body<WriteAsked>) -> Answer {
  let request = WriteRequest {
    path: asked.path,
    title: asked.title,
    keywords: Some(keyword_list(&asked.keywords)),
    items: Some(items(&asked.items)?),
    shared: asked.shared,
    all_agents: asked.for_every_agent,
    distinct_from: asked.distinct_from.unwrap_or_default(),
  };

  on_desk(&api, &asked.desk, |open| open.write(&request)).await
}

#[derive(Debug, Deserialize)]
struct RemoveAsked {
  desk: String,
  path: String,
}

async fn remove(State(api): State<Arc<Api>>, Body(asked): Body<RemoveAsked>) -> Answer {
  on_desk(&api, &asked.desk, |open| open.remove(&asked.path)).await
}

#[derive(Debug, Deserialize)]
struct MergeAsked {
  desk: String,
  into: String,
  from: Vec<String>,
  title: String,
  keywords: Option<String>,
  items: Option<Vec<i64>>,
  shared: Option<bool>,
}

async fn merge(State(api): State<Arc<Api>>, Body(asked): Body<MergeAsked>) -> Answer {
  if asked.from.is_empty() {
    return Err(ApiError::bad_request("from names at least one piece"));
  }

  let numbers = asked.items.as_deref().map(items).transpose()?;

  on_desk(&api, &asked.desk, |open| {
    open.merge(
      &asked.into,
      &asked.from,
      &asked.title,
      asked.keywords.as_deref().map(keyword_list),
      numbers,
      asked.shared,
    )
  })
  .await
}

#[derive(Debug, Deserialize)]
struct PieceAsked {
  path: String,
  title: String,
  keywords: String,
  items: Vec<i64>,
  shared: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SplitAsked {
  desk: String,
  from: String,
  pieces: Vec<PieceAsked>,
}

async fn split(State(api): State<Arc<Api>>, Body(asked): Body<SplitAsked>) -> Answer {
  if asked.pieces.is_empty() {
    return Err(ApiError::bad_request("pieces names at least one piece"));
  }

  let mut pieces = vec![];

  for piece in asked.pieces {
    pieces.push(WriteRequest {
      path: piece.path,
      title: piece.title,
      keywords: Some(keyword_list(&piece.keywords)),
      items: Some(items(&piece.items)?),
      shared: piece.shared,
      all_agents: None,
      distinct_from: Vec::new(),
    });
  }

  on_desk(&api, &asked.desk, |open| open.split(&asked.from, &pieces)).await
}

#[derive(Debug, Deserialize)]
struct OwedAsked {
  desk: String,
}

async fn owed(State(api): State<Arc<Api>>, Body(asked): Body<OwedAsked>) -> Answer {
  answered(opened(&api, &asked.desk).await?.map(|(_, open)| {
    let owed = open.owed();

    if owed.is_empty() {
      "Nothing is owed: every piece this run touched has its row.".to_owned()
    } else {
      ["These pieces have no row written in this run yet:".to_owned()]
        .into_iter()
        .chain(owed)
        .collect::<Vec<_>>()
        .join("\n")
    }
  }))
}
