//! Fragments: written, switched, removed, merged, split and hit, their kinds, and the knowledge read over them
//! with the standing rows (docs/boundary-api.md, 「Fragments」 and 「Knowledge」).

use std::collections::BTreeMap;

use bmo_brain_client::{
  Fragment, FragmentHits, FragmentHitsRecorded, FragmentKind, FragmentMerge, FragmentSplit, FragmentState,
  FragmentSwitch, FragmentWrite, FragmentsQuery, KindWords, Knowledge, KnowledgeQuery, Labels, NameCursor,
  Page,
};
use serde_json::json;
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::memories::{SCOPED_MEMORY_COLUMNS, ScopedMemoryRow};
use super::{
  FRAGMENT_MOST, Fault, Held, Store, holding, key, limit, listed, name, now, page, publish, scope, version,
};

pub(crate) const FRAGMENT_COLUMNS: &str = "id, key, kind, scope, subject_key, title, path, text, version, \
  library_name, state, switched_off, labels, hit_count, last_hit_at, created_at, changed_at";

#[derive(FromRow)]
pub(crate) struct FragmentRow {
  id: i64,
  key: String,
  kind: String,
  scope: Option<String>,
  subject_key: Option<String>,
  title: Option<String>,
  path: Option<String>,
  text: String,
  version: String,
  library_name: Option<String>,
  state: String,
  switched_off: Vec<String>,
  labels: Json<Labels>,
  hit_count: i64,
  last_hit_at: Option<i64>,
  created_at: i64,
  changed_at: i64,
}

impl FragmentRow {
  pub(crate) fn member(self) -> Held<Fragment> {
    let state = FragmentState::named(&self.state).ok_or_else(|| {
      Fault::Failed(format!("Fragment {} holds no known state: {}.", self.key, self.state))
    })?;
    Ok(Fragment {
      id: self.id,
      key: self.key,
      kind: self.kind,
      scope: self.scope,
      subject_key: self.subject_key,
      title: self.title,
      path: self.path,
      text: self.text,
      version: self.version,
      library_name: self.library_name,
      state,
      switched_off: self.switched_off,
      labels: self.labels.0,
      hit_count: self.hit_count,
      last_hit_at: self.last_hit_at,
      created_at: self.created_at,
      changed_at: self.changed_at,
    })
  }
}

pub(crate) fn members(rows: Vec<FragmentRow>) -> Held<Vec<Fragment>> {
  rows.into_iter().map(FragmentRow::member).collect()
}

#[derive(FromRow)]
struct Written {
  #[sqlx(flatten)]
  row: FragmentRow,
  created: bool,
}

/// What a write holds to: its kind, its text, and every scope and name it gives.
pub(crate) fn checked(fragment_key: &str, body: &FragmentWrite) -> Held<()> {
  key("fragmentKey", fragment_key)?;
  name("kind", &body.kind)?;
  super::text("text", &body.text, FRAGMENT_MOST)?;
  if let Some(given) = &body.scope {
    scope("scope", given)?;
  }
  if let Some(library) = &body.library_name {
    key("libraryName", library)?;
  }
  for off in body.switched_off.iter().flatten() {
    scope("switchedOff", off)?;
  }
  Ok(())
}

/// A fragment written whole in the open transaction: whether it is new, and the fragment.
pub(crate) async fn write(
  conn: &mut PgConnection,
  fragment_key: &str,
  body: &FragmentWrite,
) -> Held<(bool, Fragment)> {
  let at = now();
  let written: Written = sqlx::query_as(&format!(
    "INSERT INTO fragments (key, kind, scope, subject_key, title, path, text, version, library_name, state, \
     switched_off, labels, created_at, changed_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, COALESCE($10, 'active'), COALESCE($11, '{{}}'), $12, $13, $13) \
     ON CONFLICT (key) DO UPDATE SET kind = EXCLUDED.kind, scope = EXCLUDED.scope, \
     subject_key = EXCLUDED.subject_key, title = EXCLUDED.title, path = EXCLUDED.path, text = EXCLUDED.text, \
     version = EXCLUDED.version, library_name = EXCLUDED.library_name, \
     state = COALESCE($10, fragments.state), switched_off = COALESCE($11, fragments.switched_off), \
     labels = EXCLUDED.labels, changed_at = EXCLUDED.changed_at \
     RETURNING {FRAGMENT_COLUMNS}, (xmax = 0) AS created"
  ))
  .bind(fragment_key)
  .bind(&body.kind)
  .bind(&body.scope)
  .bind(&body.subject_key)
  .bind(&body.title)
  .bind(&body.path)
  .bind(&body.text)
  .bind(version(&body.text))
  .bind(&body.library_name)
  .bind(body.state.map(FragmentState::name))
  .bind(&body.switched_off)
  .bind(Json(body.labels.clone().unwrap_or_default()))
  .bind(at)
  .fetch_one(&mut *conn)
  .await?;
  let fragment = written.row.member()?;
  publish(conn, "fragment.changed", fragment.scope.as_deref(), &fragment).await?;
  Ok((written.created, fragment))
}

/// A fragment removed in the open transaction, where it stands; whether it stood.
pub(crate) async fn remove(conn: &mut PgConnection, fragment_key: &str) -> Held<bool> {
  let scope: Option<Option<String>> =
    sqlx::query_scalar("DELETE FROM fragments WHERE key = $1 RETURNING scope")
      .bind(fragment_key)
      .fetch_optional(&mut *conn)
      .await?;
  let Some(scope) = scope else {
    return Ok(false);
  };
  publish(conn, "fragment.deleted", scope.as_deref(), &json!({ "key": fragment_key })).await?;
  Ok(true)
}

/// The fragments of these keys that stand, locked for the transaction, by key.
async fn locked(conn: &mut PgConnection, keys: &[String]) -> Held<BTreeMap<String, Fragment>> {
  let rows: Vec<FragmentRow> = sqlx::query_as(&format!(
    "SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE key = ANY($1) ORDER BY key FOR UPDATE"
  ))
  .bind(keys)
  .fetch_all(&mut *conn)
  .await?;
  Ok(members(rows)?.into_iter().map(|fragment| (fragment.key.clone(), fragment)).collect())
}

/// A fragment as a write would give it again, with another text, title or key's fields.
fn rewritten(fragment: &Fragment) -> FragmentWrite {
  FragmentWrite {
    kind: fragment.kind.clone(),
    text: fragment.text.clone(),
    scope: fragment.scope.clone(),
    subject_key: fragment.subject_key.clone(),
    title: fragment.title.clone(),
    path: fragment.path.clone(),
    library_name: fragment.library_name.clone(),
    state: Some(fragment.state),
    switched_off: Some(fragment.switched_off.clone()),
    labels: Some(fragment.labels.clone()),
  }
}

impl Store {
  /// A page of fragments.
  pub async fn fragments(&self, query: &FragmentsQuery) -> Held<Page<Fragment>> {
    let limit = limit(query.limit)?;
    let mut sql =
      QueryBuilder::<Postgres>::new(format!("SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE TRUE"));
    if let Some(kind) = &query.kind {
      sql.push(" AND kind = ").push_bind(kind.clone());
    }
    if let Some(scope) = &query.scope {
      sql.push(" AND scope = ").push_bind(scope.clone());
    }
    if let Some(subject) = &query.subject_key {
      sql.push(" AND subject_key = ").push_bind(subject.clone());
    }
    if let Some(library) = &query.library_name {
      sql.push(" AND library_name = ").push_bind(library.clone());
    }
    if let Some(state) = query.state {
      sql.push(" AND state = ").push_bind(state.name());
    }
    if let Some(words) = &query.q {
      let pattern = holding(words);
      sql.push(" AND (text ILIKE ").push_bind(pattern.clone());
      sql.push(" OR title ILIKE ").push_bind(pattern).push(")");
    }
    if query.ids.is_some() {
      let ids = listed(query.ids.as_deref())
        .iter()
        .map(|id| id.parse::<i64>().map_err(|_| Fault::BadRequest(format!("ids holds {id:?}, no id."))))
        .collect::<Held<Vec<i64>>>()?;
      sql.push(" AND id = ANY(").push_bind(ids).push(")");
    }
    if query.keys.is_some() {
      sql.push(" AND key = ANY(").push_bind(listed(query.keys.as_deref())).push(")");
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<FragmentRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    Ok(page(members(rows)?, limit))
  }

  /// A fragment written whole: whether it is new, and the fragment.
  pub async fn fragment_put(&self, fragment_key: &str, body: &FragmentWrite) -> Held<(bool, Fragment)> {
    checked(fragment_key, body)?;
    let mut tx = self.begin().await?;
    let written = write(&mut tx, fragment_key, body).await?;
    self.commit(tx).await?;
    Ok(written)
  }

  /// A fragment switched off or on.
  pub async fn fragment_switch(&self, fragment_key: &str, body: &FragmentSwitch) -> Held<Fragment> {
    for off in body.switched_off.iter().flatten() {
      scope("switchedOff", off)?;
    }
    let mut tx = self.begin().await?;
    let row: Option<FragmentRow> = sqlx::query_as(&format!(
      "UPDATE fragments SET switched_off = COALESCE($2, switched_off), state = COALESCE($3, state), \
       changed_at = $4 WHERE key = $1 RETURNING {FRAGMENT_COLUMNS}"
    ))
    .bind(fragment_key)
    .bind(&body.switched_off)
    .bind(body.state.map(FragmentState::name))
    .bind(now())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
      return Err(Fault::NotFound(format!("There is no fragment {fragment_key:?}.")));
    };
    let fragment = row.member()?;
    publish(&mut tx, "fragment.changed", fragment.scope.as_deref(), &fragment).await?;
    self.commit(tx).await?;
    Ok(fragment)
  }

  /// A fragment removed.
  pub async fn fragment_remove(&self, fragment_key: &str) -> Held<()> {
    let mut tx = self.begin().await?;
    if !remove(&mut tx, fragment_key).await? {
      return Err(Fault::NotFound(format!("There is no fragment {fragment_key:?}.")));
    }
    self.commit(tx).await
  }

  /// Fragments folded into one; the one kept.
  pub async fn fragment_merge(&self, body: &FragmentMerge) -> Held<Fragment> {
    key("into", &body.into)?;
    for folded in &body.keys {
      key("keys", folded)?;
    }
    if !body.keys.iter().any(|folded| *folded != body.into) {
      return Err(Fault::BadRequest("The merge names no fragment to fold into the one kept.".to_owned()));
    }
    let mut asked = body.keys.clone();
    asked.push(body.into.clone());
    let mut tx = self.begin().await?;
    let standing = locked(&mut tx, &asked).await?;
    let missing: Vec<&String> = body.keys.iter().filter(|folded| !standing.contains_key(*folded)).collect();
    if !missing.is_empty() {
      return Err(Fault::BadRequest(format!("There is no fragment {missing:?} to fold.")));
    }
    let first = &standing[&body.keys[0]];
    let mut kept = rewritten(standing.get(&body.into).unwrap_or(first));
    if !standing.contains_key(&body.into) {
      kept.state = None;
      kept.switched_off = None;
    }
    let mut seen = Vec::new();
    let texts: Vec<&str> = body
      .keys
      .iter()
      .filter(|folded| {
        let new = !seen.contains(folded);
        seen.push(folded);
        new
      })
      .map(|folded| standing[folded].text.as_str())
      .collect();
    kept.text = body.text.clone().unwrap_or_else(|| texts.join("\n\n"));
    if body.title.is_some() {
      kept.title = body.title.clone();
    }
    checked(&body.into, &kept)?;
    for folded in body.keys.iter().filter(|folded| **folded != body.into) {
      remove(&mut tx, folded).await?;
    }
    let (_, fragment) = write(&mut tx, &body.into, &kept).await?;
    self.commit(tx).await?;
    Ok(fragment)
  }

  /// A fragment cut into pieces; the pieces.
  pub async fn fragment_split(&self, body: &FragmentSplit) -> Held<Page<Fragment>> {
    key("key", &body.key)?;
    if body.pieces.len() < 2 {
      return Err(Fault::BadRequest("A split names at least two pieces.".to_owned()));
    }
    let mut keys: Vec<String> = body.pieces.iter().map(|piece| piece.key.clone()).collect();
    keys.sort();
    if keys.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(Fault::BadRequest("Two pieces of the split share one key.".to_owned()));
    }
    let mut asked = keys.clone();
    asked.push(body.key.clone());
    let mut tx = self.begin().await?;
    let standing = locked(&mut tx, &asked).await?;
    let Some(cut) = standing.get(&body.key) else {
      return Err(Fault::BadRequest(format!("There is no fragment {:?} to split.", body.key)));
    };
    let taken: Vec<&String> = keys
      .iter()
      .filter(|piece| **piece != body.key && standing.contains_key(*piece))
      .collect();
    if !taken.is_empty() {
      return Err(Fault::BadRequest(format!(
        "The fragments {taken:?} stand already; a piece takes a new key."
      )));
    }
    let writes: Vec<(String, FragmentWrite)> = body
      .pieces
      .iter()
      .map(|piece| {
        let mut write = rewritten(cut);
        write.text = piece.text.clone();
        if let Some(kind) = &piece.kind {
          write.kind = kind.clone();
        }
        if piece.subject_key.is_some() {
          write.subject_key = piece.subject_key.clone();
        }
        if piece.title.is_some() {
          write.title = piece.title.clone();
        }
        if piece.path.is_some() {
          write.path = piece.path.clone();
        }
        if piece.labels.is_some() {
          write.labels = piece.labels.clone();
        }
        (piece.key.clone(), write)
      })
      .collect();
    for (piece, write) in &writes {
      checked(piece, write)?;
    }
    if !keys.contains(&body.key) {
      remove(&mut tx, &body.key).await?;
    }
    let mut pieces = Vec::new();
    for (piece, write) in &writes {
      pieces.push(self::write(&mut tx, piece, write).await?.1);
    }
    self.commit(tx).await?;
    Ok(Page {
      items: pieces,
      more: false,
    })
  }

  /// The fragments a reading attached, counted; how many were.
  pub async fn fragment_hits(&self, body: &FragmentHits) -> Held<FragmentHitsRecorded> {
    name("actorName", &body.actor_name)?;
    if let Some(given) = &body.scope {
      scope("scope", given)?;
    }
    let mut keys = body.keys.clone();
    keys.sort();
    keys.dedup();
    let at = now();
    let mut tx = self.begin().await?;
    let hit: Vec<String> = sqlx::query_scalar(
      "UPDATE fragments SET hit_count = hit_count + 1, last_hit_at = $2 WHERE key = ANY($1) RETURNING key",
    )
    .bind(&keys)
    .bind(at)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(
      "INSERT INTO fragment_hits (fragment_key, actor_name, scope, created_at) \
       SELECT key, $2, $3, $4 FROM unnest($1::text[]) AS key",
    )
    .bind(&hit)
    .bind(&body.actor_name)
    .bind(&body.scope)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    self.commit(tx).await?;
    Ok(FragmentHitsRecorded {
      recorded_count: i64::try_from(hit.len()).unwrap_or(i64::MAX),
    })
  }

  /// A page of the kinds every fragment and library names, by name.
  pub async fn fragment_kinds(&self, query: &NameCursor) -> Held<Page<FragmentKind>> {
    let limit = limit(query.limit)?;
    let counted: Vec<(String, i64)> = sqlx::query_as("SELECT kind, count(*) FROM fragments GROUP BY kind")
      .fetch_all(&self.pool)
      .await?;
    let said: Vec<Json<Vec<KindWords>>> =
      sqlx::query_scalar("SELECT kinds FROM libraries ORDER BY changed_at, name")
        .fetch_all(&self.pool)
        .await?;
    let mut kinds: BTreeMap<String, FragmentKind> = BTreeMap::new();
    for (kind, count) in counted {
      kinds.insert(
        kind.clone(),
        FragmentKind {
          name: kind,
          description: String::new(),
          fragment_count: count,
        },
      );
    }
    for words in said.into_iter().flat_map(|said| said.0) {
      let kind = kinds.entry(words.name.clone()).or_insert_with(|| FragmentKind {
        name: words.name.clone(),
        description: String::new(),
        fragment_count: 0,
      });
      kind.description = words.description;
    }
    let after = query.after.clone().unwrap_or_default();
    let items: Vec<FragmentKind> = kinds
      .into_values()
      .filter(|kind| kind.name > after)
      .take(usize::try_from(limit + 1).unwrap_or(usize::MAX))
      .collect();
    Ok(page(items, limit))
  }

  /// What a reader should know for a set of scopes.
  pub async fn knowledge(&self, query: &KnowledgeQuery) -> Held<Knowledge> {
    let scopes = listed(Some(&query.scopes));
    if scopes.is_empty() {
      return Err(Fault::BadRequest("scopes names no scope.".to_owned()));
    }
    let slugs = query.slugs.as_ref().map(|slugs| listed(Some(slugs)));
    let kinds = query.kinds.as_ref().map(|kinds| listed(Some(kinds)));
    let mut sql = QueryBuilder::<Postgres>::new(format!(
      "SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE state = 'active' AND (scope IS NULL OR scope = ANY("
    ));
    sql
      .push_bind(scopes.clone())
      .push(")) AND NOT (switched_off && ")
      .push_bind(scopes.clone())
      .push(")");
    if let Some(slugs) = &slugs {
      sql.push(" AND key = ANY(").push_bind(slugs.clone()).push(")");
    }
    if let Some(kinds) = &kinds {
      sql.push(" AND kind = ANY(").push_bind(kinds.clone()).push(")");
    }
    if let Some(subject) = &query.subject_key {
      sql.push(" AND subject_key = ").push_bind(subject.clone());
    }
    sql.push(" ORDER BY id");
    let mut conn = self.connection().await?;
    let mut fragments = members(sql.build_query_as().fetch_all(&mut *conn).await?)?;
    if let Some(slugs) = &slugs {
      fragments.sort_by_key(|fragment| slugs.iter().position(|slug| *slug == fragment.key));
    }
    let mut rows = QueryBuilder::<Postgres>::new(format!(
      "SELECT {SCOPED_MEMORY_COLUMNS} FROM scoped_memories WHERE state = 'standing' AND scope = ANY("
    ));
    rows.push_bind(scopes.clone()).push(")");
    if let Some(subject) = &query.subject_key {
      rows.push(" AND subject_key = ").push_bind(subject.clone());
    }
    rows.push(" ORDER BY id");
    let rows: Vec<ScopedMemoryRow> = rows.build_query_as().fetch_all(&mut *conn).await?;
    let memories = super::memories::members(&mut conn, rows).await?;
    Ok(Knowledge {
      scopes,
      fragments,
      memories,
    })
  }
}
