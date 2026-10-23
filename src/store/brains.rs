//! The brains, their subjects and their layers (docs/design.md, 「Whose a brain is」 and 「The three tables」).

use bmo_brain_client::{
  Brain, BrainsQuery, Layer, LayerChange, LayersQuery, NewBrain, NewLayer, NewSubject, Page, Subject,
  SubjectChange, SubjectKey, SubjectsQuery,
};
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};

use super::{Fault, Held, Store, holding, limit, now, page, publish, text};

/// The longest a brain's name, a subject's or a layer's description is.
const DESCRIPTION_MOST: usize = 2_000;

/// The scope a brain's events are published under, so a reader narrows to one brain.
pub(crate) fn event_scope(brain_id: i64) -> String {
  format!("brain:{brain_id}")
}

#[derive(FromRow)]
struct BrainRow {
  id: i64,
  account_key: String,
  agent_key: String,
  name: String,
  created_at: i64,
}

impl From<BrainRow> for Brain {
  fn from(row: BrainRow) -> Self {
    Self {
      id: row.id,
      account_key: row.account_key,
      agent_key: row.agent_key,
      name: row.name,
      created_at: row.created_at,
    }
  }
}

#[derive(FromRow)]
pub(crate) struct SubjectRow {
  pub id: i64,
  pub brain_id: i64,
  #[sqlx(rename = "type")]
  pub kind: String,
  pub key: String,
  pub description: String,
  pub created_at: i64,
}

impl From<SubjectRow> for Subject {
  fn from(row: SubjectRow) -> Self {
    Self {
      id: row.id,
      brain_id: row.brain_id,
      kind: row.kind,
      key: row.key,
      description: row.description,
      created_at: row.created_at,
    }
  }
}

#[derive(FromRow)]
struct LayerRow {
  id: i64,
  brain_id: i64,
  name: String,
  description: String,
  position: i64,
  created_at: i64,
}

impl From<LayerRow> for Layer {
  fn from(row: LayerRow) -> Self {
    Self {
      id: row.id,
      brain_id: row.brain_id,
      name: row.name,
      description: row.description,
      position: row.position,
      created_at: row.created_at,
    }
  }
}

/// A description: at most the ceiling, no control character but a line break; empty is fine.
pub(crate) fn description(value: &str) -> Held<()> {
  let length = value.chars().count();
  if length > DESCRIPTION_MOST {
    return Err(Fault::BadRequest(format!(
      "description is {length} characters; it is at most {DESCRIPTION_MOST}."
    )));
  }
  if value.chars().any(|c| c.is_control() && c != '\n') {
    return Err(Fault::BadRequest("description holds a control character.".to_owned()));
  }
  Ok(())
}

/// A key a caller gives: an account's, an agent's, a subject's type or key, a layer's name. Not blank, at
/// most 200 characters, no control character.
pub(crate) fn given(field: &str, value: &str) -> Held<()> {
  text(field, value, 200)?;
  if value.chars().any(char::is_control) {
    return Err(Fault::BadRequest(format!("{field} holds a control character.")));
  }
  Ok(())
}

/// The brain the path names, or why there is none.
pub(crate) async fn brain_of(conn: &mut PgConnection, brain_id: i64) -> Held<Brain> {
  let row: Option<BrainRow> =
    sqlx::query_as("SELECT id, account_key, agent_key, name, created_at FROM brains WHERE id = $1")
      .bind(brain_id)
      .fetch_optional(&mut *conn)
      .await?;
  row
    .map(Brain::from)
    .ok_or_else(|| Fault::NotFound(format!("There is no brain {brain_id}.")))
}

/// A layer of the brain, or why there is none.
pub(crate) async fn layer_of(conn: &mut PgConnection, brain_id: i64, layer_id: i64) -> Held<Layer> {
  let row: Option<LayerRow> = sqlx::query_as(
    "SELECT id, brain_id, name, description, position, created_at FROM layers WHERE brain_id = $1 AND id = $2",
  )
  .bind(brain_id)
  .bind(layer_id)
  .fetch_optional(&mut *conn)
  .await?;
  row
    .map(Layer::from)
    .ok_or_else(|| Fault::NotFound(format!("Brain {brain_id} has no layer {layer_id}.")))
}

/// The subject the type and key name, made where it does not stand yet.
pub(crate) async fn subject_found_or_made(
  conn: &mut PgConnection,
  brain_id: i64,
  named: &SubjectKey,
  at: i64,
) -> Held<(bool, Subject)> {
  given("type", &named.kind)?;
  given("key", &named.key)?;
  let standing: Option<SubjectRow> = sqlx::query_as(
    "SELECT id, brain_id, type, key, description, created_at FROM subjects \
     WHERE brain_id = $1 AND type = $2 AND key = $3",
  )
  .bind(brain_id)
  .bind(&named.kind)
  .bind(&named.key)
  .fetch_optional(&mut *conn)
  .await?;
  if let Some(row) = standing {
    return Ok((false, row.into()));
  }
  let row: SubjectRow = sqlx::query_as(
    "INSERT INTO subjects (brain_id, type, key, description, created_at) VALUES ($1, $2, $3, '', $4) \
     RETURNING id, brain_id, type, key, description, created_at",
  )
  .bind(brain_id)
  .bind(&named.kind)
  .bind(&named.key)
  .bind(at)
  .fetch_one(&mut *conn)
  .await?;
  let subject = Subject::from(row);
  publish(conn, "subject.created", Some(&event_scope(brain_id)), &subject).await?;
  Ok((true, subject))
}

/// The subjects these ids name, each of the brain, or why one is not.
pub(crate) async fn subjects_of(conn: &mut PgConnection, brain_id: i64, ids: &[i64]) -> Held<Vec<Subject>> {
  if ids.is_empty() {
    return Ok(vec![]);
  }
  let rows: Vec<SubjectRow> = sqlx::query_as(
    "SELECT id, brain_id, type, key, description, created_at FROM subjects \
     WHERE brain_id = $1 AND id = ANY($2) ORDER BY id",
  )
  .bind(brain_id)
  .bind(ids)
  .fetch_all(&mut *conn)
  .await?;
  for id in ids {
    if !rows.iter().any(|row| row.id == *id) {
      return Err(Fault::BadRequest(format!("Brain {brain_id} has no subject {id}.")));
    }
  }
  Ok(rows.into_iter().map(Subject::from).collect())
}

impl Store {
  /// A page of brains.
  pub async fn brains(&self, query: &BrainsQuery) -> Held<Page<Brain>> {
    let limit = limit(query.limit)?;
    let mut sql = QueryBuilder::<Postgres>::new(
      "SELECT id, account_key, agent_key, name, created_at FROM brains WHERE TRUE",
    );
    if let Some(account) = &query.account_key {
      sql.push(" AND account_key = ").push_bind(account.clone());
    }
    if let Some(agent) = &query.agent_key {
      sql.push(" AND agent_key = ").push_bind(agent.clone());
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<BrainRow> = sql.build_query_as().fetch_all(&self.pool).await?;
    Ok(page(rows.into_iter().map(Brain::from).collect(), limit))
  }

  /// A brain made for an account and an agent that have none.
  pub async fn brain_add(&self, body: &NewBrain) -> Held<Brain> {
    given("accountKey", &body.account_key)?;
    given("agentKey", &body.agent_key)?;
    let name = body.name.clone().unwrap_or_else(|| body.agent_key.clone());
    given("name", &name)?;
    let mut tx = self.begin().await?;
    let standing: Option<i64> =
      sqlx::query_scalar("SELECT id FROM brains WHERE account_key = $1 AND agent_key = $2")
        .bind(&body.account_key)
        .bind(&body.agent_key)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(id) = standing {
      return Err(Fault::Conflict(format!("Brain {id} is this account's brain of this agent already.")));
    }
    let row: BrainRow = sqlx::query_as(
      "INSERT INTO brains (account_key, agent_key, name, created_at) VALUES ($1, $2, $3, $4) \
       RETURNING id, account_key, agent_key, name, created_at",
    )
    .bind(&body.account_key)
    .bind(&body.agent_key)
    .bind(&name)
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    let brain = Brain::from(row);
    publish(&mut tx, "brain.created", Some(&event_scope(brain.id)), &brain).await?;
    self.commit(tx).await?;
    Ok(brain)
  }

  /// One brain.
  pub async fn brain(&self, brain_id: i64) -> Held<Brain> {
    brain_of(&mut *self.connection().await?, brain_id).await
  }

  /// A brain let go of whole, with everything it held.
  pub async fn brain_let_go(&self, brain_id: i64) -> Held<Brain> {
    let mut tx = self.begin().await?;
    let brain = brain_of(&mut tx, brain_id).await?;
    sqlx::query("DELETE FROM brains WHERE id = $1").bind(brain_id).execute(&mut *tx).await?;
    publish(
      &mut tx,
      "brain.deleted",
      Some(&event_scope(brain_id)),
      &serde_json::json!({ "id": brain_id }),
    )
    .await?;
    self.commit(tx).await?;
    Ok(brain)
  }

  /// A page of a brain's subjects.
  pub async fn subjects(&self, brain_id: i64, query: &SubjectsQuery) -> Held<Page<Subject>> {
    let limit = limit(query.limit)?;
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let mut sql = QueryBuilder::<Postgres>::new(
      "SELECT id, brain_id, type, key, description, created_at FROM subjects WHERE brain_id = ",
    );
    sql.push_bind(brain_id);
    if let Some(kind) = &query.kind {
      sql.push(" AND type = ").push_bind(kind.clone());
    }
    if let Some(key) = &query.key {
      sql.push(" AND key = ").push_bind(key.clone());
    }
    if let Some(words) = &query.q {
      sql.push(" AND description ILIKE ").push_bind(holding(words));
    }
    if let Some(after) = query.after {
      sql.push(" AND id > ").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows: Vec<SubjectRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    Ok(page(rows.into_iter().map(Subject::from).collect(), limit))
  }

  /// A subject added, or the one standing at its type and key with its description set where one is given.
  pub async fn subject_add(&self, brain_id: i64, body: &NewSubject) -> Held<(bool, Subject)> {
    description(&body.description)?;
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let named = SubjectKey {
      kind: body.kind.clone(),
      key: body.key.clone(),
    };
    let (new, mut subject) = subject_found_or_made(&mut tx, brain_id, &named, now()).await?;
    if !body.description.is_empty() && subject.description != body.description {
      sqlx::query("UPDATE subjects SET description = $2 WHERE id = $1")
        .bind(subject.id)
        .bind(&body.description)
        .execute(&mut *tx)
        .await?;
      subject.description = body.description.clone();
      if !new {
        publish(&mut tx, "subject.changed", Some(&event_scope(brain_id)), &subject).await?;
      }
    }
    self.commit(tx).await?;
    Ok((new, subject))
  }

  /// One subject.
  pub async fn subject(&self, brain_id: i64, subject_id: i64) -> Held<Subject> {
    let mut conn = self.connection().await?;
    let mut found = subjects_of(&mut conn, brain_id, &[subject_id])
      .await
      .map_err(|_| Fault::NotFound(format!("Brain {brain_id} has no subject {subject_id}.")))?;
    Ok(found.remove(0))
  }

  /// A subject's description changed.
  pub async fn subject_change(
    &self,
    brain_id: i64,
    subject_id: i64,
    body: &SubjectChange,
  ) -> Held<Subject> {
    description(&body.description)?;
    let mut tx = self.begin().await?;
    let row: Option<SubjectRow> = sqlx::query_as(
      "UPDATE subjects SET description = $3 WHERE brain_id = $1 AND id = $2 \
       RETURNING id, brain_id, type, key, description, created_at",
    )
    .bind(brain_id)
    .bind(subject_id)
    .bind(&body.description)
    .fetch_optional(&mut *tx)
    .await?;
    let subject = Subject::from(
      row.ok_or_else(|| Fault::NotFound(format!("Brain {brain_id} has no subject {subject_id}.")))?,
    );
    publish(&mut tx, "subject.changed", Some(&event_scope(brain_id)), &subject).await?;
    self.commit(tx).await?;
    Ok(subject)
  }

  /// A brain's layers, by position.
  pub async fn layers(&self, brain_id: i64, query: &LayersQuery) -> Held<Page<Layer>> {
    let limit = limit(query.limit)?;
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let mut sql = QueryBuilder::<Postgres>::new(
      "SELECT id, brain_id, name, description, position, created_at FROM layers WHERE brain_id = ",
    );
    sql.push_bind(brain_id);
    // The page walks the order the list stands in, position then id, from past the layer last seen.
    if let Some(after) = query.after {
      sql
        .push(" AND (position, id) > (SELECT position, id FROM layers WHERE brain_id = ")
        .push_bind(brain_id)
        .push(" AND id = ")
        .push_bind(after)
        .push(")");
    }
    sql.push(" ORDER BY position, id LIMIT ").push_bind(limit + 1);
    let rows: Vec<LayerRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    Ok(page(rows.into_iter().map(Layer::from).collect(), limit))
  }

  /// A layer added, or the one standing at its name.
  pub async fn layer_add(&self, brain_id: i64, body: &NewLayer) -> Held<(bool, Layer)> {
    given("name", &body.name)?;
    description(&body.description)?;
    let mut tx = self.begin().await?;
    brain_of(&mut tx, brain_id).await?;
    let standing: Option<LayerRow> = sqlx::query_as(
      "SELECT id, brain_id, name, description, position, created_at FROM layers \
       WHERE brain_id = $1 AND name = $2",
    )
    .bind(brain_id)
    .bind(&body.name)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = standing {
      return Ok((false, row.into()));
    }
    let position = match body.position {
      Some(position) => position,
      None => {
        let last: Option<i64> = sqlx::query_scalar("SELECT MAX(position) FROM layers WHERE brain_id = $1")
          .bind(brain_id)
          .fetch_one(&mut *tx)
          .await?;
        last.map_or(0, |last| last + 1)
      }
    };
    let row: LayerRow = sqlx::query_as(
      "INSERT INTO layers (brain_id, name, description, position, created_at) VALUES ($1, $2, $3, $4, $5) \
       RETURNING id, brain_id, name, description, position, created_at",
    )
    .bind(brain_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(position)
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    let layer = Layer::from(row);
    publish(&mut tx, "layer.changed", Some(&event_scope(brain_id)), &layer).await?;
    self.commit(tx).await?;
    Ok((true, layer))
  }

  /// One layer.
  pub async fn layer(&self, brain_id: i64, layer_id: i64) -> Held<Layer> {
    layer_of(&mut *self.connection().await?, brain_id, layer_id).await
  }

  /// A layer's description or position changed.
  pub async fn layer_change(&self, brain_id: i64, layer_id: i64, body: &LayerChange) -> Held<Layer> {
    if let Some(words) = &body.description {
      description(words)?;
    }
    let mut tx = self.begin().await?;
    layer_of(&mut tx, brain_id, layer_id).await?;
    let row: LayerRow = sqlx::query_as(
      "UPDATE layers SET description = COALESCE($3, description), position = COALESCE($4, position) \
       WHERE brain_id = $1 AND id = $2 RETURNING id, brain_id, name, description, position, created_at",
    )
    .bind(brain_id)
    .bind(layer_id)
    .bind(&body.description)
    .bind(body.position)
    .fetch_one(&mut *tx)
    .await?;
    let layer = Layer::from(row);
    publish(&mut tx, "layer.changed", Some(&event_scope(brain_id)), &layer).await?;
    self.commit(tx).await?;
    Ok(layer)
  }
}
