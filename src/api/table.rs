//! The fragment table as a person and a shipped owner keep it (docs/fragments.md, 「Writing a row by hand」, 「The
//! drift check」 and 「The shipped libraries」; docs/boundary-api.md, 「Fragments」 and 「Shipped libraries」): the
//! rows listed, a piece's row written or taken down by hand, the table checked against what stands on disk, the
//! brain's libraries reconciled, a shipped library's entries handed in, and the word a screen calls every kind by.
//! It stands on the editors' own reading of the table and the brain, so a person's reconcile, drawing and write are
//! the ones an editor's batch makes.

use std::path::Path;
use std::sync::Arc;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use fragment_desk::desk::{FragmentFilter, FragmentWrite};
use fragment_desk::meta::{body_hash, fragment_write, removal_write, title_shape_problem, words_carried};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::fragments::Fragment;
use super::{Answer, Api, ApiError, Body, reading, refusal};
use crate::editors::fragments::libraries::{LIBRARIES, Said};
use crate::editors::fragments::narrow::index_line;
use crate::editors::fragments::store::{self, ShippedEntry};
use crate::editors::fragments::{ShippedFailure, Table, Writing, kind_labels};

/// The fragment table's routes.
pub fn routes() -> Router<Arc<Api>> {
  Router::new()
    .route("/fragments", get(list))
    .route("/fragments/write", post(write))
    .route("/fragments/remove", post(remove))
    .route("/fragments/check", post(check))
    .route("/fragments/sync", post(sync))
    .route("/fragments/kinds", get(kinds))
    .route("/libraries/reconcile", post(reconcile))
}

/// What the list is narrowed by.
#[derive(Debug, Default, Deserialize)]
pub struct ListAsked {
  pub kind: Option<String>,
  pub chat: Option<i64>,
  pub person: Option<i64>,
  pub item: Option<i64>,
  pub text: Option<String>,
  #[serde(default)]
  pub down: bool,
}

async fn list(State(api): State<Arc<Api>>, asked: Result<Query<ListAsked>, QueryRejection>) -> Answer {
  let Query(asked) = asked.map_err(|why| ApiError::bad_request(why.body_text()))?;

  if let Some(kind) = &asked.kind
    && !kind_labels(&api.db).await?.iter().any(|(known, _)| known == kind)
  {
    return Err(ApiError::bad_request(format!("{kind} is no kind of fragment.")));
  }

  let filter = FragmentFilter {
    kind: asked.kind.clone(),
    chat_id: asked.chat.map(Some),
    owners: asked.person.map(|person| vec![person]),
    active: (!asked.down).then_some(true),
    ..FragmentFilter::default()
  };
  let fragments: Vec<Fragment> = store::rows_standing(&api.db, &filter)
    .await
    .map_err(crate::Failure::from)?
    .into_iter()
    .filter(|(row, _)| asked.item.is_none_or(|item| row.items.contains(&item)))
    // A word stands in a row where its title or one of its keywords carries it.
    .filter(|(row, _)| {
      asked.text.as_ref().is_none_or(|word| {
        let carried = std::iter::once(row.title.clone()).chain(row.keywords.clone()).collect::<Vec<_>>();

        words_carried(std::slice::from_ref(word), &carried.join("\n")) > 0
      })
    })
    .map(Fragment::of)
    .collect();

  Ok(Json(json!({ "fragments": fragments })))
}

/// One piece's row as a person writes it by hand.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteAsked {
  pub path: String,
  pub title: String,
  #[serde(default)]
  pub keywords: Vec<String>,
  #[serde(default)]
  pub items: Vec<i64>,
  pub shared: Option<bool>,
  pub chat_id: Option<i64>,
  pub for_every_agent: Option<bool>,
}

fn row_refused(reasons: Vec<String>) -> Json<Value> {
  let mut params = Map::new();

  params.insert("reasons".to_owned(), json!(reasons));

  refusal("row-refused", "The row checks refuse this write.", params, vec![])
}

// One write by hand held to the checks an editor's write is held to, applied, and the index files drawn after it:
// the rows it wrote, or the reasons it was refused.
async fn written(
  table: &Table<'_>,
  write: &FragmentWrite,
  room: Option<i64>,
) -> Result<Result<Vec<store::Row>, Vec<String>>, ApiError> {
  let brain = &table.brain;
  let body = |piece: &str| std::fs::read_to_string(brain.locate(piece)).ok();
  let problems = table.write_problems(&Writing {
    writes: std::slice::from_ref(write),
    changed: &[],
    allowed: &|_| true,
    body: &body,
    seen: &|piece| piece.to_owned(),
  });

  if !problems.is_empty() {
    return Ok(Err(problems));
  }

  let rows = table.apply_writes(std::slice::from_ref(write), room).await?;

  table.draw_indexes(None).await?;

  Ok(Ok(rows))
}

async fn write(State(api): State<Arc<Api>>, Body(asked): Body<WriteAsked>) -> Answer {
  if asked.title.trim().is_empty() {
    return Err(ApiError::bad_request("A row written by hand needs a title."));
  }

  let table = api.table().await?;
  let keywords: Vec<String> = asked
    .keywords
    .iter()
    .map(|word| word.trim().to_owned())
    .filter(|word| !word.is_empty())
    .collect();
  let shared = asked.shared.or(asked.chat_id.map(|_| false));
  let write =
    fragment_write(&asked.path, &asked.title, &keywords, &asked.items, shared, asked.for_every_agent);
  let rows = match written(&table, &write, asked.chat_id).await? {
    Ok(rows) => rows,
    Err(problems) => return Ok(row_refused(problems)),
  };

  Ok(match rows.first() {
    Some(row) => reading(vec![format!("{} [{}]", index_line(row), row.path)]),
    None => row_refused(vec![format!(
      "{} could not be placed: a knowledge chunk new to the table needs shared or a chatId",
      asked.path
    )]),
  })
}

#[derive(Debug, Deserialize)]
struct RemoveAsked {
  path: String,
}

async fn remove(State(api): State<Arc<Api>>, Body(asked): Body<RemoveAsked>) -> Answer {
  let table = api.table().await?;

  if table.brain.locate(&asked.path).exists() {
    let mut params = Map::new();

    params.insert("path".to_owned(), json!(asked.path));

    return Ok(refusal(
      "body-stands",
      "The piece's file still stands; a row is taken down by hand only once its file is gone.",
      params,
      vec![],
    ));
  }

  match written(&table, &removal_write(&asked.path), None).await? {
    Ok(_) => Ok(reading(vec![format!("taken down: {}", asked.path)])),
    Err(problems) => Ok(row_refused(problems)),
  }
}

/// What a row and what stands on disk disagree about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drift {
  pub trouble: &'static str,
  pub kind: String,
  pub chat_id: Option<i64>,
  pub owner: Option<i64>,
  pub slug: String,
  pub said: String,
}

// Every file under a directory, relative to the brain root, the git repository's own left out.
fn files_under(root: &Path, at: &Path, found: &mut Vec<String>) {
  let Ok(entries) = std::fs::read_dir(at) else { return };
  let mut entries: Vec<_> = entries.flatten().collect();

  entries.sort_by_key(std::fs::DirEntry::file_name);

  for entry in entries {
    let path = entry.path();

    if entry.file_name() == ".git" {
      continue;
    }

    if path.is_dir() {
      files_under(root, &path, found);
    } else if let Ok(relative) = path.strip_prefix(root) {
      found.push(relative.to_string_lossy().into_owned());
    }
  }
}

/// What the table and what stands on disk disagree about, for the brain's libraries: a file with no row, a row
/// with no file, and a body changed since the table read it. It reports and repairs nothing; the reconcile is what
/// repairs, and running this first is how a surprise is seen before it is written over.
pub async fn drift(table: &Table<'_>) -> Result<Vec<Drift>, crate::Failure> {
  let mut found = vec![];
  let mut pieces = vec![];

  files_under(&table.brain.root, &table.brain.root, &mut pieces);

  for library in LIBRARIES {
    let rows = table.rows(&store::of_kind(library.kind())).await?;
    let drift_of = |row: &store::Row, trouble, said| Drift {
      trouble,
      kind: row.kind.clone(),
      chat_id: row.chat_id,
      owner: row.owner,
      slug: row.slug.clone(),
      said,
    };

    for row in rows.iter().filter(|row| row.active) {
      match std::fs::read_to_string(table.brain.locate(&row.path)) {
        Err(_) if row.hand != Some(true) => {
          found.push(drift_of(row, "bodiless", format!("no file at {}", row.path)));
        }
        Ok(body) if row.body_hash.as_deref() != Some(body_hash(&body).as_str()) => {
          found.push(drift_of(
            row,
            "moved",
            format!("the body at {} changed since the table read it", row.path),
          ));
        }
        _ => {}
      }
    }

    for path in pieces
      .iter()
      .filter(|path| table.brain.library_owning(path).is_some_and(|owner| owner.kind() == library.kind()))
    {
      if rows.iter().any(|row| row.active && row.path == *path) {
        continue;
      }

      let standing = rows.iter().find(|row| row.path == *path);

      if standing.is_some_and(|standing| standing.hand == Some(false)) {
        continue;
      }

      let at = table.brain.place(
        library,
        path,
        &Said {
          shared: None,
          room: None,
          standing,
        },
      );
      let slug = at.as_ref().map_or_else(
        || {
          let name = path.rsplit('/').next().unwrap_or(path);

          name.strip_suffix(".md").unwrap_or(name).to_owned()
        },
        |at| at.slug.clone(),
      );

      found.push(Drift {
        trouble: "unlisted",
        kind: library.kind().to_owned(),
        chat_id: at.as_ref().and_then(|at| at.chat_id),
        owner: at.as_ref().and_then(|at| at.owner),
        slug,
        said: if standing.is_none() {
          format!("the piece at {path} has no row")
        } else {
          format!("the piece at {path} stands and its row is down")
        },
      });
    }
  }

  Ok(found)
}

#[derive(Debug, Default, Deserialize)]
struct CheckAsked {
  kind: Option<String>,
}

async fn check(State(api): State<Arc<Api>>, Body(asked): Body<CheckAsked>) -> Answer {
  let table = api.table().await?;
  let found = drift(&table).await?;
  let mut titles = vec![];
  let mut counted = vec![];

  // A standing row is held to the title shape only when it is written again, so the rows of the brain's libraries
  // whose titles the shape check refuses are counted here.
  for library in LIBRARIES
    .iter()
    .filter(|library| asked.kind.as_ref().is_none_or(|kind| kind == library.kind()))
  {
    let rows = table
      .rows(&FragmentFilter {
        active: Some(true),
        ..store::of_kind(library.kind())
      })
      .await?;
    let mut refused = 0;

    for row in &rows {
      if let Some(problem) = title_shape_problem(&row.title, &table.names) {
        refused += 1;
        titles.push(json!({
          "kind": row.kind,
          "chatId": row.chat_id,
          "owner": row.owner,
          "slug": row.slug,
          "path": row.path,
          "title": row.title,
          "problem": problem,
        }));
      }
    }

    counted.push(json!({ "kind": library.kind(), "refused": refused, "of": rows.len() }));
  }

  Ok(Json(json!({ "drift": found, "titles": titles, "counted": counted })))
}

async fn sync(State(api): State<Arc<Api>>) -> Answer {
  let table = api.table().await?;
  let mut lines: Vec<String> = table.reconcile(None).await?.iter().map(|done| done.line()).collect();

  // A person running the reconcile holds the brain for it, so the index files are drawn too.
  lines.extend(table.draw_indexes(None).await?.into_iter().map(|path| format!("drawn: {path}")));

  Ok(reading(lines))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reconcile {
  kind: String,
  label: String,
  entries: Vec<ShippedEntry>,
}

async fn reconcile(State(api): State<Arc<Api>>, Body(asked): Body<Reconcile>) -> Answer {
  if asked.kind.trim().is_empty() || asked.label.trim().is_empty() {
    return Err(ApiError::bad_request(
      "A shipped library is handed in with a kind and the word a screen calls it by.",
    ));
  }

  let table = api.table().await?;

  match table.reconcile_shipped(&asked.kind, &asked.label, &asked.entries).await {
    Ok(done) => Ok(reading(vec![done.line()])),
    Err(ShippedFailure::BrainLibrary) => {
      let mut params = Map::new();

      params.insert("kind".to_owned(), json!(asked.kind));

      Ok(refusal(
        "brain-library",
        "The kind is one of the brain's own libraries, whose rows only its editors write.",
        params,
        vec![],
      ))
    }
    Err(ShippedFailure::Failed(failure)) => Err(failure.into()),
  }
}

async fn kinds(State(api): State<Arc<Api>>) -> Answer {
  let kinds: Vec<Value> = kind_labels(&api.db)
    .await?
    .into_iter()
    .map(|(kind, label)| json!({ "kind": kind, "label": label }))
    .collect();

  Ok(Json(json!({ "kinds": kinds })))
}
