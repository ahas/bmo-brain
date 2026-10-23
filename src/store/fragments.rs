//! Fragments, listed and switched for the console's screens, and their kinds (docs/boundary-api.md,
//! 「Fragments」).

use std::collections::BTreeMap;

use bmo_brain_client::{
  Fragment, FragmentKind, FragmentState, FragmentSwitch, FragmentsQuery, Labels, NameCursor, Page,
};
use sqlx::types::Json;
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::{Fault, Held, Store, holding, limit, listed, now, page, publish, scope};

const FRAGMENT_COLUMNS: &str = "id, key, kind, scope, subject_key, title, path, text, version, \
  library_name, state, switched_off, labels, hit_count, last_hit_at, created_at, changed_at";

#[derive(FromRow)]
struct FragmentRow {
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
  fn member(self) -> Held<Fragment> {
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

fn members(rows: Vec<FragmentRow>) -> Held<Vec<Fragment>> {
  rows.into_iter().map(FragmentRow::member).collect()
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

  /// A page of the kinds every fragment names or the store holds a description of, by name.
  pub async fn fragment_kinds(&self, query: &NameCursor) -> Held<Page<FragmentKind>> {
    let limit = limit(query.limit)?;
    let counted: Vec<(String, i64)> = sqlx::query_as("SELECT kind, count(*) FROM fragments GROUP BY kind")
      .fetch_all(&self.pool)
      .await?;
    let described: Vec<(String, String)> = sqlx::query_as("SELECT name, description FROM fragment_kinds")
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
    for (name, description) in described {
      let kind = kinds.entry(name.clone()).or_insert_with(|| FragmentKind {
        name,
        description: String::new(),
        fragment_count: 0,
      });
      kind.description = description;
    }
    let after = query.after.clone().unwrap_or_default();
    let items: Vec<FragmentKind> = kinds
      .into_values()
      .filter(|kind| kind.name > after)
      .take(usize::try_from(limit + 1).unwrap_or(usize::MAX))
      .collect();
    Ok(page(items, limit))
  }
}
