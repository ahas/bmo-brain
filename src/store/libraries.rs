//! Libraries, handed in whole, and the sync that brings their fragments in line (docs/boundary-api.md,
//! 「Libraries」).

use std::collections::{BTreeMap, BTreeSet};

use bmo_brain_client::{
  Drift, DriftChange, Fragment, FragmentWrite, KindWords, Library, LibraryDocument, LibraryPut,
  LibrarySync, NameCursor, Page,
};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection};

use super::fragments::{FRAGMENT_COLUMNS, FragmentRow, checked, members, remove, write};
use super::{FRAGMENT_MOST, Fault, Held, Store, key, limit, name, now, page, publish, scope, version};

#[derive(FromRow)]
struct LibraryRow {
  name: String,
  documents: Json<Vec<LibraryDocument>>,
  kinds: Json<Vec<KindWords>>,
  changed_at: i64,
}

impl LibraryRow {
  fn member(self) -> Library {
    Library {
      name: self.name,
      documents: self.documents.0,
      kinds: self.kinds.0,
      changed_at: self.changed_at,
    }
  }
}

async fn read(conn: &mut PgConnection, library_name: &str) -> Held<Library> {
  let row: Option<LibraryRow> =
    sqlx::query_as("SELECT name, documents, kinds, changed_at FROM libraries WHERE name = $1")
      .bind(library_name)
      .fetch_optional(&mut *conn)
      .await?;
  row
    .map(LibraryRow::member)
    .ok_or_else(|| Fault::NotFound(format!("There is no library {library_name:?}.")))
}

/// The fragment a document makes, keeping what only the fragment holds: its state, switches and labels.
fn made(library_name: &str, document: &LibraryDocument, fragment: Option<&Fragment>) -> FragmentWrite {
  FragmentWrite {
    kind: document.kind.clone(),
    text: document.text.clone(),
    scope: document.scope.clone(),
    subject_key: document.subject_key.clone(),
    title: document.title.clone(),
    path: document.path.clone(),
    library_name: Some(library_name.to_owned()),
    state: fragment.map(|fragment| fragment.state),
    switched_off: fragment.map(|fragment| fragment.switched_off.clone()),
    labels: fragment.map(|fragment| fragment.labels.clone()),
  }
}

/// How a fragment differs from the document of its key, where it does.
fn drift(
  library_name: &str,
  document: &LibraryDocument,
  fragment: Option<&Fragment>,
) -> Option<DriftChange> {
  let Some(fragment) = fragment else {
    return Some(DriftChange::Added);
  };
  if fragment.library_name.as_deref() != Some(library_name) {
    return Some(DriftChange::Claimed);
  }
  let same = fragment.version == document.version
    && fragment.text == document.text
    && fragment.kind == document.kind
    && fragment.scope == document.scope
    && fragment.subject_key == document.subject_key
    && fragment.title == document.title
    && fragment.path == document.path;
  (!same).then_some(DriftChange::Changed)
}

impl Store {
  /// A page of libraries, by name.
  pub async fn libraries(&self, query: &NameCursor) -> Held<Page<Library>> {
    let limit = limit(query.limit)?;
    let rows: Vec<LibraryRow> = sqlx::query_as(
      "SELECT name, documents, kinds, changed_at FROM libraries WHERE name > $1 ORDER BY name LIMIT $2",
    )
    .bind(query.after.clone().unwrap_or_default())
    .bind(limit + 1)
    .fetch_all(&self.pool)
    .await?;
    Ok(page(rows.into_iter().map(LibraryRow::member).collect(), limit))
  }

  /// One library.
  pub async fn library(&self, library_name: &str) -> Held<Library> {
    read(&mut *self.connection().await?, library_name).await
  }

  /// A library put whole: whether it is new, and the library.
  pub async fn library_put(&self, library_name: &str, body: &LibraryPut) -> Held<(bool, Library)> {
    key("libraryName", library_name)?;
    let mut keys = BTreeSet::new();
    let mut documents = Vec::with_capacity(body.documents.len());
    for document in &body.documents {
      key("documents.key", &document.key)?;
      name("documents.kind", &document.kind)?;
      super::text("documents.text", &document.text, FRAGMENT_MOST)?;
      if let Some(given) = &document.scope {
        scope("documents.scope", given)?;
      }
      if !keys.insert(document.key.clone()) {
        return Err(Fault::BadRequest(format!("Two documents share the key {:?}.", document.key)));
      }
      documents.push(LibraryDocument {
        key: document.key.clone(),
        kind: document.kind.clone(),
        scope: document.scope.clone(),
        subject_key: document.subject_key.clone(),
        title: document.title.clone(),
        path: document.path.clone(),
        text: document.text.clone(),
        version: version(&document.text),
      });
    }
    for words in &body.kinds {
      name("kinds.name", &words.name)?;
    }
    let mut tx = self.begin().await?;
    let created: bool = sqlx::query_scalar(
      "INSERT INTO libraries (name, documents, kinds, changed_at) VALUES ($1, $2, $3, $4) \
       ON CONFLICT (name) DO UPDATE SET documents = EXCLUDED.documents, kinds = EXCLUDED.kinds, \
       changed_at = EXCLUDED.changed_at RETURNING (xmax = 0)",
    )
    .bind(library_name)
    .bind(Json(&documents))
    .bind(Json(&body.kinds))
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    let library = read(&mut tx, library_name).await?;
    publish(&mut tx, "library.changed", None, &library).await?;
    self.commit(tx).await?;
    Ok((created, library))
  }

  /// The fragments of a library brought in line with its documents, or with `dry_run` only what would change.
  pub async fn library_sync(&self, library_name: &str, dry_run: bool) -> Held<LibrarySync> {
    key("libraryName", library_name)?;
    let mut tx = self.begin().await?;
    let library = read(&mut tx, library_name).await?;
    let keys: Vec<String> = library.documents.iter().map(|document| document.key.clone()).collect();
    let rows: Vec<FragmentRow> = sqlx::query_as(&format!(
      "SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE library_name = $1 OR key = ANY($2) ORDER BY key FOR UPDATE"
    ))
    .bind(library_name)
    .bind(&keys)
    .fetch_all(&mut *tx)
    .await?;
    let standing: BTreeMap<String, Fragment> =
      members(rows)?.into_iter().map(|fragment| (fragment.key.clone(), fragment)).collect();
    let mut drifts = Vec::new();
    let mut writes = Vec::new();
    for document in &library.documents {
      let fragment = standing.get(&document.key);
      if let Some(change) = drift(library_name, document, fragment) {
        drifts.push(Drift {
          key: document.key.clone(),
          change,
        });
        writes.push((document.key.clone(), made(library_name, document, fragment)));
      }
    }
    let gone: Vec<String> = standing
      .values()
      .filter(|fragment| {
        fragment.library_name.as_deref() == Some(library_name) && !keys.contains(&fragment.key)
      })
      .map(|fragment| fragment.key.clone())
      .collect();
    drifts.extend(gone.iter().map(|key| Drift {
      key: key.clone(),
      change: DriftChange::Removed,
    }));
    drifts.sort_by(|one, other| one.key.cmp(&other.key));
    if !dry_run {
      for (fragment_key, made) in &writes {
        checked(fragment_key, made)?;
        write(&mut tx, fragment_key, made).await?;
      }
      for fragment_key in &gone {
        remove(&mut tx, fragment_key).await?;
      }
      self.commit(tx).await?;
    }
    Ok(LibrarySync {
      library_name: library_name.to_owned(),
      drift: drifts,
      applied: !dry_run,
    })
  }
}
