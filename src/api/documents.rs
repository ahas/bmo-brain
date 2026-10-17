//! The brain's documents as a turn's gate reads them, and what a session already holds of them
//! (docs/boundary-api.md, 「Brain documents」 and 「Session document loads」): the knowledge chunks and person-book
//! pieces the gate picked, each among those the room may be offered, the standing sheets of the turn's senders, and
//! each session's record of the documents it was handed in full, by path and version.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use brain_support::database::{all, run};
use brain_support::js;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::fragments::{Fragment, offer};
use super::{Answer, Api, Body, refusal};
use crate::Failure;
use crate::editors::archive::keeps_room_book;
use crate::editors::book::{common_sheet_path_of, people_dir_of};
use crate::editors::fragments::store::now_seconds;
use crate::editors::recollection::recollection_for;
use crate::policy::BRAIN;

/// The documents' and the session loads' routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/knowledge/read", post(knowledge_read))
    .route("/people/read", post(people_read))
    .route("/sessions/loads", post(loads))
    .route("/sessions/loads/keep", post(loads_keep))
    .route("/sessions/loads/forget", post(loads_forget))
}

// How long a session's loads are kept untouched before they go: a session that old has long been replaced.
const LOADS_KEPT_SECONDS: i64 = 7 * 86_400;

/// A text's version, as a session holds a document by it.
pub fn version_of(text: &str) -> String {
  js::bun_hash(text).to_string()
}

// A document under the brain root, trimmed, or none where it is gone.
fn read_trimmed(api: &Api, path: &str) -> Option<String> {
  std::fs::read_to_string(api.brain_root.join(path))
    .ok()
    .map(|text| js::trim(&text).to_owned())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadAsked {
  agent: String,
  chat_id: i64,
  #[serde(default)]
  senders: Vec<i64>,
  #[serde(default)]
  slugs: Vec<String>,
}

// The rows of a kind the room may be offered that the slugs name, in the order asked, or the slugs that are none of
// them.
async fn named(
  api: &Api,
  asked: &ReadAsked,
  kind: &str,
) -> Result<Result<Vec<Fragment>, Vec<String>>, Failure> {
  let offered =
    offer(&api.db, api.receiver.as_ref(), &asked.agent, asked.chat_id, &[kind.to_owned()]).await?;
  let mut found = vec![];
  let mut outside = vec![];

  for slug in &asked.slugs {
    match offered.iter().find(|row| &row.slug == slug) {
      Some(row) => found.push(row.clone()),
      None => outside.push(slug.clone()),
    }
  }

  Ok(if outside.is_empty() { Ok(found) } else { Err(outside) })
}

fn out_of_room(slugs: Vec<String>) -> Json<Value> {
  let mut params = Map::new();

  params.insert("slugs".to_owned(), json!(slugs));

  refusal(
    "out-of-room",
    "These documents are not among those this room may be offered.",
    params,
    vec![],
  )
}

async fn knowledge_read(State(api): State<Arc<Api>>, Body(asked): Body<ReadAsked>) -> Answer {
  let rows = match named(&api, &asked, "library-knowledge").await? {
    Ok(rows) => rows,
    Err(outside) => return Ok(out_of_room(outside)),
  };
  let documents: Vec<Value> = rows
    .iter()
    .filter_map(|row| {
      let text = read_trimmed(&api, &row.path)?;

      Some(json!({ "slug": row.slug, "path": row.path, "version": version_of(&text), "text": text }))
    })
    .collect();

  Ok(Json(json!({ "documents": documents })))
}

/// One standing sheet as the people read answers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Sheet {
  pub person: i64,
  pub part: &'static str,
  pub path: String,
  pub text: String,
  pub version: String,
}

// A sheet read from its place, or none where it is missing, empty or over its ceiling.
fn sheet(api: &Api, person: i64, part: &'static str, path: String, ceiling: usize) -> Option<Sheet> {
  let text = read_trimmed(api, &path).filter(|text| !text.is_empty())?;

  if js::units(&text) > ceiling {
    eprintln!(
      "people read: {path} is passed over: {} characters is over its {ceiling}",
      js::units(&text)
    );
    return None;
  }

  Some(Sheet {
    person,
    part,
    version: version_of(&text),
    path,
    text,
  })
}

async fn people_read(State(api): State<Arc<Api>>, Body(asked): Body<ReadAsked>) -> Answer {
  let rows = match named(&api, &asked, "person-book").await? {
    Ok(rows) => rows,
    Err(outside) => return Ok(out_of_room(outside)),
  };
  let mut sheets = vec![];

  // An agent keeping no long-term memory is given no sheet.
  if let Some(recollection) =
    recollection_for(api.agency.as_ref(), &api.brain_root, &api.chat_key, &asked.agent).await?
  {
    let room_book = keeps_room_book(api.receiver.as_ref(), asked.chat_id).await?;
    let ceilings = &BRAIN.sheets;
    let mut seen = vec![];

    for person in asked.senders.iter().copied().filter(|person| {
      let new = !seen.contains(person);

      seen.push(*person);
      new
    }) {
      let common = recollection.outward(&common_sheet_path_of(person));

      sheets.extend(sheet(&api, person, "common", common, ceilings.common_ceiling));

      if room_book {
        let own = recollection.outward(&format!("{}/{person}.md", people_dir_of(asked.chat_id)));

        sheets.extend(sheet(&api, person, "room", own, ceilings.room_ceiling));
      }
    }
  }

  let pieces: Vec<Value> = rows
    .iter()
    .filter_map(|row| {
      let text = read_trimmed(&api, &row.path)?;

      Some(json!({ "slug": row.slug, "path": row.path, "version": version_of(&text), "owner": row.owner, "text": text }))
    })
    .collect();

  Ok(Json(json!({ "sheets": sheets, "pieces": pieces })))
}

#[derive(Debug, Deserialize)]
struct SessionAsked {
  session: String,
}

/// One document a session holds, by its path and version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Load {
  pub path: String,
  pub version: String,
}

async fn loads(State(api): State<Arc<Api>>, Body(asked): Body<SessionAsked>) -> Answer {
  let rows = all(
    &api.db,
    r#"SELECT "path", "version" FROM "session_people" WHERE "session_id" = $1 ORDER BY "path""#,
    vec![asked.session.into()],
  )
  .await
  .map_err(Failure::from)?;
  let loads = rows
    .iter()
    .map(|row| {
      Ok(Load {
        path: row.try_get("", "path")?,
        version: row.try_get("", "version")?,
      })
    })
    .collect::<Result<Vec<_>, sea_orm::DbErr>>()
    .map_err(Failure::from)?;

  Ok(Json(json!({ "loads": loads })))
}

#[derive(Debug, Deserialize)]
struct KeepAsked {
  session: String,
  loads: Vec<Load>,
  at: Option<i64>,
}

async fn loads_keep(State(api): State<Arc<Api>>, Body(asked): Body<KeepAsked>) -> Answer {
  let at = asked.at.unwrap_or_else(now_seconds);

  for load in &asked.loads {
    run(
      &api.db,
      r#"INSERT INTO "session_people" ("session_id", "path", "version", "loaded_at") VALUES ($1, $2, $3, $4)
         ON CONFLICT ("session_id", "path") DO UPDATE SET "version" = excluded."version", "loaded_at" = excluded."loaded_at""#,
      vec![asked.session.clone().into(), load.path.clone().into(), load.version.clone().into(), at.into()],
    )
    .await
    .map_err(Failure::from)?;
  }

  // The sessions untouched for a week go at the same stroke: a session that old has long been replaced.
  run(
    &api.db,
    r#"DELETE FROM "session_people" WHERE "session_id" IN
         (SELECT "session_id" FROM "session_people" GROUP BY "session_id" HAVING MAX("loaded_at") < $1)"#,
    vec![(at - LOADS_KEPT_SECONDS).into()],
  )
  .await
  .map_err(Failure::from)?;

  Ok(Json(json!({ "ok": true })))
}

async fn loads_forget(State(api): State<Arc<Api>>, Body(asked): Body<SessionAsked>) -> Answer {
  run(
    &api.db,
    r#"DELETE FROM "session_people" WHERE "session_id" = $1"#,
    vec![asked.session.into()],
  )
  .await
  .map_err(Failure::from)?;

  Ok(Json(json!({ "ok": true })))
}
