//! The brain's `fragments` table as an editor batch and a shipped library's reconcile read and write it
//! (docs/fragments.md): one library's rows in the order their entries stand, a shipped library's rows brought to
//! the entries its owner handed in, one piece's row written as its writer said it, a row taken down, what a body
//! says about itself noted, and each row's status settled — with the hand a person laid on a row's standing
//! honoured throughout.

use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use brain_support::database::{Fault, all, one, run};
use fragment_desk::desk::FragmentFilter;
use fragment_desk::meta::BodyFacts;
use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, TransactionTrait};
use serde_json::Value;

/// One row of the table, whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
  pub id: i64,
  pub kind: String,
  pub chat_id: Option<i64>,
  pub owner: Option<i64>,
  pub agent: String,
  pub all_agents: bool,
  pub slug: String,
  pub title: String,
  pub path: String,
  pub scope: Option<String>,
  pub position: i64,
  pub active: bool,
  pub keywords: Vec<String>,
  pub items: Vec<i64>,
  pub written_on: Option<String>,
  pub confirmed_on: Option<String>,
  pub status: String,
  pub body_hash: Option<String>,
  /// The standing a person gave the row, which a reconcile and a write leave where the hand put it.
  pub hand: Option<bool>,
}

/// A piece's row as its writer said it stands, with nothing of the row's own standing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewRow {
  pub kind: String,
  pub chat_id: Option<i64>,
  pub owner: Option<i64>,
  /// The agent whose memory it is.
  pub agent: String,
  pub all_agents: Option<bool>,
  pub slug: String,
  pub title: String,
  pub path: String,
  pub scope: Option<String>,
  pub position: i64,
  pub keywords: Vec<String>,
  pub items: Vec<i64>,
}

/// One entry of a shipped library as its owner hands it in: everything its row says, read off the owner's index
/// and the body, which the brain never reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippedEntry {
  pub slug: String,
  pub title: String,
  pub path: String,
  pub position: i64,
  pub scope: Option<String>,
  pub items: Vec<i64>,
  pub written_on: Option<String>,
  pub confirmed_on: Option<String>,
  pub body_hash: Option<String>,
}

/// What one reconcile did to a library's rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sync {
  pub added: u64,
  pub updated: u64,
  pub retired: u64,
  pub revived: u64,
}

// A column holding a JSON list, read as the values of the kind it should hold; anything else as none.
fn list<T>(text: &str, keep: impl Fn(&Value) -> Option<T>) -> Vec<T> {
  match serde_json::from_str::<Value>(text) {
    Ok(Value::Array(values)) => values.iter().filter_map(keep).collect(),
    _ => vec![],
  }
}

fn whole(value: &Value) -> Option<i64> {
  value
    .as_i64()
    .or_else(|| value.as_f64().filter(|number| number.fract() == 0.0).map(|number| number as i64))
}

fn row_of(row: &QueryResult) -> Result<Row, Fault> {
  let text = |column: &str| row.try_get::<String>("", column);
  let maybe = |column: &str| row.try_get::<Option<String>>("", column);
  let number = |column: &str| row.try_get::<Option<i64>>("", column);

  Ok(Row {
    id: row.try_get("", "id")?,
    kind: text("kind")?,
    chat_id: number("chat_id")?,
    owner: number("owner")?,
    agent: text("agent")?,
    all_agents: number("all_agents")?.unwrap_or(0) != 0,
    slug: text("slug")?,
    title: text("title")?,
    path: text("path")?,
    scope: maybe("scope")?,
    position: row.try_get("", "position")?,
    active: number("active")?.unwrap_or(0) != 0,
    keywords: list(&text("keywords")?, |value| value.as_str().map(str::to_owned)),
    items: list(&text("items")?, whole),
    written_on: maybe("written_on")?,
    confirmed_on: maybe("confirmed_on")?,
    status: text("status")?,
    body_hash: maybe("body_hash")?,
    hand: number("hand")?.map(|hand| hand != 0),
  })
}

/// What a row has come to beside what it says: how often and when a turn last attached it, who set it and when,
/// and when a person's hand was laid on its standing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Standing {
  pub last_hit_at: Option<i64>,
  pub hits: i64,
  pub set_by: Option<i64>,
  pub set_at: i64,
  pub hand_at: Option<i64>,
}

fn standing_of(row: &QueryResult) -> Result<Standing, Fault> {
  Ok(Standing {
    last_hit_at: row.try_get("", "last_hit_at")?,
    hits: row.try_get("", "hits")?,
    set_by: row.try_get("", "set_by")?,
    set_at: row.try_get("", "set_at")?,
    hand_at: row.try_get("", "hand_at")?,
  })
}

/// The rows a filter asks for, in the order their entries stand: by kind, room, position and id.
pub async fn rows<C: ConnectionTrait>(db: &C, filter: &FragmentFilter) -> Result<Vec<Row>, Fault> {
  filtered(db, filter).await?.iter().map(row_of).collect()
}

/// The rows a filter asks for as [`rows`] reads them, each with what it has come to.
pub async fn rows_standing<C: ConnectionTrait>(
  db: &C,
  filter: &FragmentFilter,
) -> Result<Vec<(Row, Standing)>, Fault> {
  filtered(db, filter)
    .await?
    .iter()
    .map(|row| Ok((row_of(row)?, standing_of(row)?)))
    .collect()
}

/// A turn's attaching of one row counted on it: the active row of that kind, room, owner and slug that the agent
/// may read, its own or one marked for every agent.
pub async fn count_hit<C: ConnectionTrait>(
  db: &C,
  agent: &str,
  hit: (&str, Option<i64>, Option<i64>, &str),
  now: i64,
) -> Result<(), Fault> {
  let (kind, chat_id, owner, slug) = hit;

  run(
    db,
    r#"UPDATE "fragments" SET "hits" = "hits" + 1, "last_hit_at" = $1
       WHERE "kind" = $2 AND "chat_id" IS NOT DISTINCT FROM $3 AND "owner" IS NOT DISTINCT FROM $4
         AND "slug" = $5 AND "active" = 1 AND ("agent" = $6 OR "all_agents" = 1)"#,
    vec![now.into(), kind.into(), chat_id.into(), owner.into(), slug.into(), agent.into()],
  )
  .await?;

  Ok(())
}

async fn filtered<C: ConnectionTrait>(db: &C, filter: &FragmentFilter) -> Result<Vec<QueryResult>, Fault> {
  let mut terms = vec![];
  let mut values: Vec<sea_orm::Value> = vec![];
  let next = |value: sea_orm::Value, values: &mut Vec<sea_orm::Value>| {
    values.push(value);
    format!("${}", values.len())
  };

  if let Some(kind) = &filter.kind {
    terms.push(format!(r#""kind" = {}"#, next(kind.clone().into(), &mut values)));
  }

  match filter.chat_id {
    Some(None) => terms.push(r#""chat_id" IS NULL"#.to_owned()),
    Some(Some(chat_id)) => terms.push(format!(r#""chat_id" = {}"#, next(chat_id.into(), &mut values))),
    None => {}
  }

  if let Some(owners) = &filter.owners {
    if owners.is_empty() {
      terms.push("FALSE".to_owned());
    } else {
      let marks: Vec<String> = owners.iter().map(|owner| next((*owner).into(), &mut values)).collect();

      terms.push(format!(r#""owner" IN ({})"#, marks.join(", ")));
    }
  }

  if let Some(agent) = &filter.agent {
    terms.push(format!(r#""agent" = {}"#, next(agent.clone().into(), &mut values)));
  }

  if let Some(reader) = &filter.readable_by {
    terms.push(format!(
      r#"("agent" = {} OR "all_agents" = 1)"#,
      next(reader.clone().into(), &mut values)
    ));
  }

  if let Some(active) = filter.active {
    terms.push(format!(r#""active" = {}"#, next(i64::from(active).into(), &mut values)));
  }

  let filtered = if terms.is_empty() { String::new() } else { format!("WHERE {}", terms.join(" AND ")) };
  let sql = format!(
    r#"SELECT * FROM "fragments" {filtered} ORDER BY "kind", COALESCE("chat_id", 0), "position", "id""#
  );

  all(db, &sql, values).await
}

/// A filter for one kind.
pub fn of_kind(kind: &str) -> FragmentFilter {
  FragmentFilter {
    kind: Some(kind.to_owned()),
    ..FragmentFilter::default()
  }
}

// The row answering to a name, whether or not it is still offered.
async fn named<C: ConnectionTrait>(db: &C, entry: &NewRow) -> Result<Option<Row>, Fault> {
  one(
    db,
    r#"SELECT * FROM "fragments" WHERE "kind" = $1 AND "agent" = $2 AND COALESCE("chat_id", 0) = $3
         AND COALESCE("owner", 0) = $4 AND "slug" = $5"#,
    vec![
      entry.kind.as_str().into(),
      entry.agent.as_str().into(),
      entry.chat_id.unwrap_or(0).into(),
      entry.owner.unwrap_or(0).into(),
      entry.slug.as_str().into(),
    ],
  )
  .await?
  .as_ref()
  .map(row_of)
  .transpose()
}

fn json_list<T: serde::Serialize>(values: &[T]) -> String {
  serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

async fn insert<C: ConnectionTrait>(db: &C, entry: &NewRow, now: i64) -> Result<Row, Fault> {
  let made = one(
    db,
    r#"INSERT INTO "fragments"
         ("kind", "agent", "all_agents", "chat_id", "owner", "slug", "title", "path", "scope", "position", "active",
          "last_hit_at", "hits", "set_by", "set_at", "keywords", "items")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, NULL, 0, NULL, $11, $12, $13) RETURNING *"#,
    vec![
      entry.kind.as_str().into(),
      entry.agent.as_str().into(),
      i64::from(entry.all_agents.unwrap_or(false)).into(),
      entry.chat_id.into(),
      entry.owner.into(),
      entry.slug.as_str().into(),
      entry.title.as_str().into(),
      entry.path.as_str().into(),
      entry.scope.clone().into(),
      entry.position.into(),
      now.into(),
      json_list(&entry.keywords).into(),
      json_list(&entry.items).into(),
    ],
  )
  .await?
  .ok_or_else(|| Fault::new("the fragment row was not written"))?;

  row_of(&made)
}

// What a shipped entry's body says about itself, as its owner read it, written onto its row.
async fn note_shipped<C: ConnectionTrait>(db: &C, id: i64, entry: &ShippedEntry) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "fragments" SET "body_hash" = $1, "written_on" = $2, "confirmed_on" = $3 WHERE "id" = $4"#,
    vec![
      entry.body_hash.clone().into(),
      entry.written_on.clone().into(),
      entry.confirmed_on.clone().into(),
      id.into(),
    ],
  )
  .await?;

  Ok(())
}

/// A shipped library's rows, the given agent's, brought to the entries its owner handed in: an entry with no row
/// becomes one, a changed title, path, scope tag, position, item, day or hash is written onto its row, a row the
/// entries no longer name goes down, and one they name again is raised. A row a person's hand holds stays where
/// the hand put it while the library disagrees, and is let go once it agrees. Nothing is deleted. A shipped row
/// carries no keywords. Answers what the rows went through, and how many had what their body says about itself
/// noted again.
pub async fn set_rows(
  db: &DatabaseConnection,
  kind: &str,
  agent: &str,
  entries: &[ShippedEntry],
  now: i64,
) -> Result<(Sync, u64), Fault> {
  let name = |chat_id: Option<i64>, owner: Option<i64>, slug: &str| {
    format!("{}\u{0}{}\u{0}{slug}", chat_id.unwrap_or(0), owner.unwrap_or(0))
  };
  let held = db.begin().await?;
  let mut standing: BTreeMap<String, Row> = rows(
    &held,
    &FragmentFilter {
      agent: Some(agent.to_owned()),
      ..of_kind(kind)
    },
  )
  .await?
  .into_iter()
  .map(|row| (name(row.chat_id, row.owner, &row.slug), row))
  .collect();
  let mut sync = Sync::default();
  let mut reread = 0;

  for entry in entries {
    let Some(already) = standing.remove(&name(None, None, &entry.slug)) else {
      let made = insert(
        &held,
        &NewRow {
          kind: kind.to_owned(),
          agent: agent.to_owned(),
          slug: entry.slug.clone(),
          title: entry.title.clone(),
          path: entry.path.clone(),
          scope: entry.scope.clone(),
          position: entry.position,
          items: entry.items.clone(),
          ..NewRow::default()
        },
        now,
      )
      .await?;

      if entry.body_hash.is_some() || entry.written_on.is_some() || entry.confirmed_on.is_some() {
        note_shipped(&held, made.id, entry).await?;
        reread += 1;
      }

      sync.added += 1;
      continue;
    };
    let moved = already.title != entry.title
      || already.path != entry.path
      || already.scope != entry.scope
      || already.position != entry.position;
    let reworded = already.items != entry.items;

    if already.body_hash != entry.body_hash
      || already.written_on != entry.written_on
      || already.confirmed_on != entry.confirmed_on
    {
      note_shipped(&held, already.id, entry).await?;
      reread += 1;
    }

    if moved || reworded {
      run(
        &held,
        r#"UPDATE "fragments" SET "title" = $1, "path" = $2, "scope" = $3, "position" = $4, "items" = $5
           WHERE "id" = $6"#,
        vec![
          entry.title.as_str().into(),
          entry.path.as_str().into(),
          entry.scope.clone().into(),
          entry.position.into(),
          json_list(&entry.items).into(),
          already.id.into(),
        ],
      )
      .await?;
    }

    // A row a person took down stays down while its library still names it.
    if already.hand == Some(false) {
      if moved {
        sync.updated += 1;
      }

      continue;
    }

    if !already.active || already.hand.is_some() {
      run(
        &held,
        r#"UPDATE "fragments" SET "active" = 1, "hand" = NULL, "hand_at" = NULL WHERE "id" = $1"#,
        vec![already.id.into()],
      )
      .await?;
    }

    if !already.active {
      sync.revived += 1;
    } else if moved {
      sync.updated += 1;
    }
  }

  for gone in standing.values() {
    match gone.hand {
      Some(true) => {}
      Some(false) => {
        run(
          &held,
          r#"UPDATE "fragments" SET "hand" = NULL, "hand_at" = NULL WHERE "id" = $1"#,
          vec![gone.id.into()],
        )
        .await?;
      }
      None if gone.active => {
        run(&held, r#"UPDATE "fragments" SET "active" = 0 WHERE "id" = $1"#, vec![gone.id.into()]).await?;
        sync.retired += 1;
      }
      None => {}
    }
  }

  held.commit().await?;

  Ok((sync, reread))
}

/// One piece an editor wrote, as its row write and its path say it stands: a row made where none answers to its
/// name, and otherwise its title, keywords, items, path and scope tag the write's from now on. A row that was down
/// is raised, unless a person took it down. The position is the row's own once it has one.
pub async fn write_row<C: ConnectionTrait>(db: &C, entry: &NewRow, now: i64) -> Result<Row, Fault> {
  let Some(standing) = named(db, entry).await? else {
    return insert(db, entry, now).await;
  };
  let raise = i64::from(standing.hand != Some(false));
  let written = one(
    db,
    r#"UPDATE "fragments" SET "title" = $1, "path" = $2, "scope" = $3, "keywords" = $4, "items" = $5, "all_agents" = $6,
         "active" = CASE WHEN $7 = 1 THEN 1 ELSE "active" END,
         "hand" = CASE WHEN $7 = 1 AND "hand" = 1 THEN NULL ELSE "hand" END,
         "hand_at" = CASE WHEN $7 = 1 AND "hand" = 1 THEN NULL ELSE "hand_at" END
       WHERE "id" = $8 RETURNING *"#,
    vec![
      entry.title.as_str().into(),
      entry.path.as_str().into(),
      entry.scope.clone().into(),
      json_list(&entry.keywords).into(),
      json_list(&entry.items).into(),
      i64::from(entry.all_agents.unwrap_or(standing.all_agents)).into(),
      raise.into(),
      standing.id.into(),
    ],
  )
  .await?
  .ok_or_else(|| Fault::new("the fragment row was not written"))?;

  row_of(&written)
}

/// A piece whose body is gone, taken down with its counts: a row a person held up stays up, and one a person
/// took down has its mark let go.
pub async fn retire_row<C: ConnectionTrait>(db: &C, id: i64) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "fragments" SET
         "active" = CASE WHEN "hand" = 1 THEN "active" ELSE 0 END,
         "hand" = CASE WHEN "hand" = 0 THEN NULL ELSE "hand" END,
         "hand_at" = CASE WHEN "hand" = 0 THEN NULL ELSE "hand_at" END
       WHERE "id" = $1"#,
    vec![id.into()],
  )
  .await?;

  Ok(())
}

/// What a row's body says about itself, read off the file: its digest, its days, and the items it names, which
/// join the ones its writer gave.
pub async fn note_body<C: ConnectionTrait>(
  db: &C,
  id: i64,
  hash: &str,
  facts: &BodyFacts,
) -> Result<Vec<i64>, Fault> {
  let Some(row) = one(db, r#"SELECT "items" FROM "fragments" WHERE "id" = $1"#, vec![id.into()]).await?
  else {
    return Ok(vec![]);
  };
  let mut items = list(&row.try_get::<String>("", "items")?, whole);

  for item in &facts.items {
    if !items.contains(item) {
      items.push(*item);
    }
  }

  run(
    db,
    r#"UPDATE "fragments" SET "body_hash" = $1, "written_on" = $2, "confirmed_on" = $3, "items" = $4 WHERE "id" = $5"#,
    vec![
      hash.into(),
      facts.written_on.clone().into(),
      facts.confirmed_on.clone().into(),
      json_list(&items).into(),
      id.into(),
    ],
  )
  .await?;

  Ok(items)
}

/// Whether each of these rows' matter is finished. Answers how many changed.
pub async fn settle_status(
  db: &DatabaseConnection,
  statuses: &HashMap<i64, &'static str>,
) -> Result<u64, Fault> {
  if statuses.is_empty() {
    return Ok(0);
  }

  let held = db.begin().await?;
  let mut changed = 0;

  for (id, status) in statuses {
    changed += run(
      &held,
      r#"UPDATE "fragments" SET "status" = $1 WHERE "id" = $2 AND "status" <> $1"#,
      vec![(*status).into(), (*id).into()],
    )
    .await?;
  }

  held.commit().await?;

  Ok(changed)
}

/// Now, in the seconds the table's own columns keep.
/// The word a screen calls a shipped kind by kept, the last one handed in standing.
pub async fn set_kind_label<C: ConnectionTrait>(
  db: &C,
  kind: &str,
  label: &str,
  now: i64,
) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "fragment_kinds" ("kind", "label", "set_at") VALUES ($1, $2, $3)
       ON CONFLICT ("kind") DO UPDATE SET "label" = EXCLUDED."label", "set_at" = EXCLUDED."set_at""#,
    vec![kind.into(), label.into(), now.into()],
  )
  .await?;

  Ok(())
}

/// Every shipped kind's label as it was last handed in, by kind.
pub async fn kind_labels<C: ConnectionTrait>(db: &C) -> Result<Vec<(String, String)>, Fault> {
  all(db, r#"SELECT "kind", "label" FROM "fragment_kinds" ORDER BY "kind""#, vec![])
    .await?
    .iter()
    .map(|row| Ok((row.try_get("", "kind")?, row.try_get("", "label")?)))
    .collect()
}

/// A person's standing laid on one row of an agent, named by its kind, room, owner and slug: its standing and its
/// hand both set to `active`, nothing else written. The row as it then stands, or none where no row is named so.
pub async fn set_hand<C: ConnectionTrait>(
  db: &C,
  agent: &str,
  named: (&str, Option<i64>, Option<i64>, &str),
  active: bool,
  now: i64,
) -> Result<Option<(Row, Standing)>, Fault> {
  let (kind, chat_id, owner, slug) = named;
  let row = one(
    db,
    r#"UPDATE "fragments" SET "active" = $1, "hand" = $1, "hand_at" = $2
       WHERE "kind" = $3 AND "agent" = $4 AND COALESCE("chat_id", 0) = $5 AND COALESCE("owner", 0) = $6
         AND "slug" = $7
       RETURNING *"#,
    vec![
      i64::from(active).into(),
      now.into(),
      kind.into(),
      agent.into(),
      chat_id.unwrap_or(0).into(),
      owner.unwrap_or(0).into(),
      slug.into(),
    ],
  )
  .await?;

  row.map(|row| Ok((row_of(&row)?, standing_of(&row)?))).transpose()
}

/// The moment now, in whole seconds.
pub fn now_seconds() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
}
