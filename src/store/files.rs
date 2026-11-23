//! The brain file (docs/design.md, 「Export and injection」): a brain, or the layers named, written as a
//! SQLite database in the brain's own shape, and such a file read into a brain as new versions.

use std::collections::BTreeMap;

use bmo_brain_client::{Injected, Labels, MemoryStatus, SubjectKey};
use rusqlite::{Connection, params};
use sqlx::types::Json;

use super::brains::{brain_of, subject_found_or_made};
use super::orgs::{About, Marks, PERSON, person_rule};
use super::versions::{MEMORY_COLUMNS, MemoryRow, link_subjects};
use super::{Fault, Held, Store, now, publish};

/// The version of the file's shape, written in every file and checked on every read; a shape-1 file holds no
/// memory condition, its rows carrying none, a file of shape 1 or 2 holds a storage strength, which is not
/// read, a file of shape 1 to 3 holds no org memory's marks and a file of shape 4 no `shared` mark.
pub const SHAPE: i64 = 5;

/// The first shape holding an org memory's marks.
const MARKED_SHAPE: i64 = 4;

/// The first shape holding the `shared` mark.
const SHARED_SHAPE: i64 = 5;

/// The oldest shape this brain still reads.
const OLDEST_SHAPE: i64 = 1;

const FILE_TABLES: &str = "
CREATE TABLE brain_file (shape INTEGER NOT NULL, exported_at INTEGER NOT NULL, name TEXT NOT NULL);
CREATE TABLE subjects (id INTEGER PRIMARY KEY, type TEXT NOT NULL, key TEXT NOT NULL, description TEXT NOT NULL);
CREATE TABLE layers (id INTEGER PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL, position INTEGER NOT NULL);
CREATE TABLE memories (id INTEGER PRIMARY KEY, key TEXT, type TEXT NOT NULL, text TEXT NOT NULL, layer_id INTEGER, \
  position INTEGER NOT NULL, status TEXT NOT NULL, lineage_id INTEGER NOT NULL, superseded_by INTEGER, \
  valid_from INTEGER NOT NULL, valid_to INTEGER, importance INTEGER NOT NULL, hits INTEGER NOT NULL, \
  last_hit_at INTEGER, last_confirmed_at INTEGER, actor_name TEXT NOT NULL, \
  labels TEXT NOT NULL, recall_condition TEXT, release_condition TEXT, created_at INTEGER NOT NULL, \
  locked INTEGER NOT NULL, internal_only INTEGER NOT NULL, shared INTEGER NOT NULL);
CREATE TABLE memory_subjects (memory_id INTEGER NOT NULL, subject_id INTEGER NOT NULL, PRIMARY KEY (memory_id, subject_id));
";

fn failed(why: impl std::fmt::Display) -> Fault {
  Fault::Failed(format!("The brain file: {why}."))
}

/// A path of this process's own under the temporary directory.
fn scratch(what: &str) -> std::path::PathBuf {
  static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  std::env::temp_dir().join(format!("bmo-brain-{what}-{}-{n}.sqlite", std::process::id()))
}

/// What a file reads as: its subjects, its layers and its live memories.
type FileRead = (Vec<(i64, String, String, String)>, Vec<(i64, String, String, i64)>, Vec<FileMemory>);

/// A memory as the file holds it.
struct FileMemory {
  id: i64,
  key: Option<String>,
  kind: String,
  text: String,
  layer_id: Option<i64>,
  position: i64,
  status: String,
  valid_from: i64,
  importance: i64,
  actor_name: String,
  labels: Labels,
  recall_condition: Option<String>,
  release_condition: Option<String>,
  /// The marks, where the file's shape holds them.
  marks: Option<Marks>,
  subject_ids: Vec<i64>,
}

impl Store {
  /// The brain, or the layers named, as a brain file's bytes.
  pub async fn export(&self, brain_id: i64, layers: &[String]) -> Held<Vec<u8>> {
    let mut conn = self.connection().await?;
    let brain = brain_of(&mut conn, brain_id).await?;
    let layer_rows: Vec<(i64, String, String, i64)> = if layers.is_empty() {
      sqlx::query_as(
        "SELECT id, name, description, position FROM layers WHERE brain_id = $1 ORDER BY position, id",
      )
      .bind(brain_id)
      .fetch_all(&mut *conn)
      .await?
    } else {
      let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT id, name, description, position FROM layers WHERE brain_id = $1 AND name = ANY($2) ORDER BY position, id",
      )
      .bind(brain_id)
      .bind(layers)
      .fetch_all(&mut *conn)
      .await?;
      for name in layers {
        if !rows.iter().any(|(_, standing, _, _)| standing == name) {
          return Err(Fault::NotFound(format!("Brain {brain_id} has no layer {name:?}.")));
        }
      }
      rows
    };
    let memories: Vec<MemoryRow> = if layers.is_empty() {
      sqlx::query_as(&format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 ORDER BY id"))
        .bind(brain_id)
        .fetch_all(&mut *conn)
        .await?
    } else {
      let ids: Vec<i64> = layer_rows.iter().map(|(id, ..)| *id).collect();
      sqlx::query_as(&format!(
        "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND layer_id = ANY($2) \
         AND status IN ('active', 'pending') ORDER BY id"
      ))
      .bind(brain_id)
      .bind(&ids)
      .fetch_all(&mut *conn)
      .await?
    };
    let memory_ids: Vec<i64> = memories.iter().map(|row| row.id).collect();
    let links: Vec<(i64, i64)> = sqlx::query_as(
      "SELECT memory_id, subject_id FROM memory_subjects WHERE memory_id = ANY($1) ORDER BY 1, 2",
    )
    .bind(&memory_ids)
    .fetch_all(&mut *conn)
    .await?;
    let subjects: Vec<(i64, String, String, String)> = if layers.is_empty() {
      sqlx::query_as("SELECT id, type, key, description FROM subjects WHERE brain_id = $1 ORDER BY id")
        .bind(brain_id)
        .fetch_all(&mut *conn)
        .await?
    } else {
      let ids: Vec<i64> = links.iter().map(|(_, subject)| *subject).collect();
      sqlx::query_as("SELECT id, type, key, description FROM subjects WHERE id = ANY($1) ORDER BY id")
        .bind(&ids)
        .fetch_all(&mut *conn)
        .await?
    };
    drop(conn);

    let path = scratch("export");
    let written = tokio::task::spawn_blocking({
      let path = path.clone();
      let name = brain.name.clone();
      move || -> Result<Vec<u8>, Fault> {
        let file = Connection::open(&path).map_err(failed)?;
        file.execute_batch(FILE_TABLES).map_err(failed)?;
        file
          .execute("INSERT INTO brain_file (shape, exported_at, name) VALUES (?1, ?2, ?3)", params![SHAPE, now(), name])
          .map_err(failed)?;
        for (id, kind, key, description) in &subjects {
          file
            .execute("INSERT INTO subjects (id, type, key, description) VALUES (?1, ?2, ?3, ?4)", params![id, kind, key, description])
            .map_err(failed)?;
        }
        for (id, name, description, position) in &layer_rows {
          file
            .execute("INSERT INTO layers (id, name, description, position) VALUES (?1, ?2, ?3, ?4)", params![id, name, description, position])
            .map_err(failed)?;
        }
        for row in &memories {
          let labels = serde_json::to_string(&row.labels.0).map_err(failed)?;
          file
            .execute(
              "INSERT INTO memories (id, key, type, text, layer_id, position, status, lineage_id, superseded_by, \
               valid_from, valid_to, importance, hits, last_hit_at, last_confirmed_at, actor_name, labels, \
               recall_condition, release_condition, created_at, locked, internal_only, shared) \
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, \
               ?21, ?22, ?23)",
              params![
                row.id,
                row.key,
                row.kind,
                row.text,
                row.layer_id,
                row.position,
                row.status,
                row.lineage_id,
                row.superseded_by,
                row.valid_from,
                row.valid_to,
                i64::from(row.importance),
                row.hits,
                row.last_hit_at,
                row.last_confirmed_at,
                row.actor_name,
                labels,
                row.recall_condition,
                row.release_condition,
                row.created_at,
                row.locked,
                row.internal_only,
                row.shared
              ],
            )
            .map_err(failed)?;
        }
        for (memory, subject) in &links {
          file
            .execute("INSERT INTO memory_subjects (memory_id, subject_id) VALUES (?1, ?2)", params![memory, subject])
            .map_err(failed)?;
        }
        file.close().map_err(|(_, why)| failed(why))?;
        let bytes = std::fs::read(&path).map_err(failed)?;
        let _ = std::fs::remove_file(&path);
        Ok(bytes)
      }
    })
    .await
    .map_err(failed)?;
    let _ = std::fs::remove_file(&path);
    written
  }

  /// A brain file read into a brain: layers and subjects reused by name and by type and key, a keyed row
  /// superseding the live row of its key, every other live row added; nothing overwritten.
  pub async fn inject(&self, brain_id: i64, bytes: Vec<u8>) -> Held<Injected> {
    if bytes.is_empty() {
      return Err(Fault::BadRequest("The body holds no brain file.".to_owned()));
    }
    let path = scratch("inject");
    std::fs::write(&path, &bytes).map_err(failed)?;
    let read = tokio::task::spawn_blocking({
      let path = path.clone();
      move || -> Result<FileRead, Fault> {
        let file = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
          .map_err(|why| Fault::BadRequest(format!("The body is no brain file: {why}.")))?;
        let shape: i64 = file
          .query_row("SELECT shape FROM brain_file", [], |row| row.get(0))
          .map_err(|why| Fault::BadRequest(format!("The body is no brain file: {why}.")))?;
        if !(OLDEST_SHAPE..=SHAPE).contains(&shape) {
          return Err(Fault::BadRequest(format!(
            "The brain file is of shape {shape}; this brain reads shapes {OLDEST_SHAPE} to {SHAPE}."
          )));
        }
        let subjects = file
          .prepare("SELECT id, type, key, description FROM subjects ORDER BY id")
          .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<Vec<_>, _>>()
          })
          .map_err(failed)?;
        let layers = file
          .prepare("SELECT id, name, description, position FROM layers ORDER BY position, id")
          .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<Vec<_>, _>>()
          })
          .map_err(failed)?;
        let mut links: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        file
          .prepare("SELECT memory_id, subject_id FROM memory_subjects ORDER BY 1, 2")
          .and_then(|mut s| {
            for pair in s.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
              let (memory, subject) = pair?;
              links.entry(memory).or_default().push(subject);
            }
            Ok(())
          })
          .map_err(failed)?;
        let conditions = if shape >= 2 { "recall_condition, release_condition" } else { "NULL, NULL" };
        let marks = match shape {
          _ if shape >= SHARED_SHAPE => "locked, internal_only, shared",
          _ if shape >= MARKED_SHAPE => "locked, internal_only, NULL",
          _ => "NULL, NULL, NULL",
        };
        let memories = file
          .prepare(&format!(
            "SELECT id, key, type, text, layer_id, position, status, valid_from, importance, actor_name, labels, \
             {conditions}, {marks} FROM memories WHERE status IN ('active', 'pending') ORDER BY id"
          ))
          .and_then(|mut s| {
            s.query_map([], |r| {
              let labels: String = r.get(10)?;
              Ok(FileMemory {
                id: r.get(0)?,
                key: r.get(1)?,
                kind: r.get(2)?,
                text: r.get(3)?,
                layer_id: r.get(4)?,
                position: r.get(5)?,
                status: r.get(6)?,
                valid_from: r.get(7)?,
                importance: r.get(8)?,
                actor_name: r.get(9)?,
                labels: serde_json::from_str(&labels).unwrap_or_default(),
                recall_condition: r.get(11)?,
                release_condition: r.get(12)?,
                marks: match (r.get::<_, Option<bool>>(13)?, r.get::<_, Option<bool>>(14)?) {
                  (Some(locked), Some(internal_only)) => Some(Marks {
                    locked,
                    internal_only,
                    shared: r.get::<_, Option<bool>>(15)?.unwrap_or(false),
                  }),
                  _ => None,
                },
                subject_ids: vec![],
              })
            })?
            .collect::<Result<Vec<_>, _>>()
          })
          .map_err(failed)?
          .into_iter()
          .map(|mut memory| {
            memory.subject_ids = links.remove(&memory.id).unwrap_or_default();
            memory
          })
          .collect();
        Ok((subjects, layers, memories))
      }
    })
    .await
    .map_err(failed)?;
    let _ = std::fs::remove_file(&path);
    let (subjects, layers, memories) = read?;
    for memory in &memories {
      if let Some(key) = &memory.key {
        super::key("key", key)?;
      }
      if memory.kind == super::sheets::SHEET {
        return Err(Fault::BadRequest(format!(
          "The brain file holds the live sheet {}; a person's sheet is rendered from their facts.",
          memory.id
        )));
      }
    }

    let at = now();
    let mut tx = self.begin().await?;
    let brain = brain_of(&mut tx, brain_id).await?;
    let mut injected = Injected {
      subjects_added: 0,
      layers_added: 0,
      memories_added: 0,
      memories_superseded: 0,
    };
    let mut subject_map: BTreeMap<i64, i64> = BTreeMap::new();
    let mut people: Vec<i64> = vec![];
    for (file_id, kind, key, description) in subjects {
      if kind == PERSON {
        people.push(file_id);
      }
      let named = SubjectKey { kind, key };
      let (new, subject) = subject_found_or_made(&mut tx, brain_id, &named, at).await?;
      if new {
        injected.subjects_added += 1;
        if !description.is_empty() {
          sqlx::query("UPDATE subjects SET description = $2 WHERE id = $1")
            .bind(subject.id)
            .bind(&description)
            .execute(&mut *tx)
            .await?;
        }
      }
      subject_map.insert(file_id, subject.id);
    }
    let mut layer_map: BTreeMap<i64, i64> = BTreeMap::new();
    for (file_id, name, description, position) in layers {
      let standing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM layers WHERE brain_id = $1 AND name = $2")
          .bind(brain_id)
          .bind(&name)
          .fetch_optional(&mut *tx)
          .await?;
      let id = match standing {
        Some(id) => id,
        None => {
          injected.layers_added += 1;
          sqlx::query_scalar(
            "INSERT INTO layers (brain_id, name, description, position, created_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
          )
          .bind(brain_id)
          .bind(&name)
          .bind(&description)
          .bind(position)
          .bind(at)
          .fetch_one(&mut *tx)
          .await?
        }
      };
      layer_map.insert(file_id, id);
    }
    for memory in memories {
      let status = MemoryStatus::named(&memory.status).unwrap_or(MemoryStatus::Active);
      let layer_id = memory.layer_id.and_then(|id| layer_map.get(&id).copied());
      let subject_ids: Vec<i64> =
        memory.subject_ids.iter().filter_map(|id| subject_map.get(id).copied()).collect();
      let labels = Json(&memory.labels);
      let marks = Marks::injected(brain.kind, memory.marks);
      person_rule(
        &brain,
        &About {
          people: memory.subject_ids.iter().copied().filter(|id| people.contains(id)).collect(),
          shared: marks.shared,
        },
      )?;
      let holder: Option<MemoryRow> = match &memory.key {
        Some(key) => {
          sqlx::query_as(&format!(
            "SELECT {MEMORY_COLUMNS} FROM memories WHERE brain_id = $1 AND key = $2 AND status IN ('active', 'pending') FOR UPDATE"
          ))
          .bind(brain_id)
          .bind(key)
          .fetch_optional(&mut *tx)
          .await?
        }
        None => None,
      };
      match holder {
        Some(old) => {
          if old.text == memory.text
            && old.kind == memory.kind
            && old.layer_id == layer_id
            && old.recall_condition == memory.recall_condition
            && old.release_condition == memory.release_condition
            && Marks::of(&old) == marks
          {
            continue;
          }
          sqlx::query("UPDATE memories SET status = 'superseded', valid_to = $2 WHERE id = $1")
            .bind(old.id)
            .bind(at)
            .execute(&mut *tx)
            .await?;
          let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
             recall_condition, release_condition, importance, hits, hit_times, last_hit_at, last_confirmed_at, \
             actor_name, labels, created_at, locked, internal_only, shared) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, \
             $21, $22) RETURNING id",
          )
          .bind(brain_id)
          .bind(&old.key)
          .bind(&memory.kind)
          .bind(&memory.text)
          .bind(layer_id)
          .bind(memory.position)
          .bind(old.status()?.name())
          .bind(old.lineage_id)
          .bind(at)
          .bind(&memory.recall_condition)
          .bind(&memory.release_condition)
          .bind(memory.importance.clamp(1, 10) as i16)
          .bind(old.hits)
          .bind(&old.hit_times)
          .bind(old.last_hit_at)
          .bind(old.last_confirmed_at)
          .bind(&memory.actor_name)
          .bind(labels)
          .bind(at)
          .bind(marks.locked)
          .bind(marks.internal_only)
          .bind(marks.shared)
          .fetch_one(&mut *tx)
          .await?;
          sqlx::query("UPDATE memories SET superseded_by = $2, superseded_at = $3 WHERE id = $1")
            .bind(old.id)
            .bind(new_id)
            .bind(at)
            .execute(&mut *tx)
            .await?;
          link_subjects(&mut tx, new_id, &subject_ids).await?;
          injected.memories_superseded += 1;
        }
        None => {
          let id: i64 = sqlx::query_scalar(
            "INSERT INTO memories (brain_id, key, type, text, layer_id, position, status, lineage_id, valid_from, \
             recall_condition, release_condition, importance, actor_name, labels, created_at, locked, internal_only, \
             shared) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) RETURNING id",
          )
          .bind(brain_id)
          .bind(&memory.key)
          .bind(&memory.kind)
          .bind(&memory.text)
          .bind(layer_id)
          .bind(memory.position)
          .bind(status.name())
          .bind(memory.valid_from)
          .bind(&memory.recall_condition)
          .bind(&memory.release_condition)
          .bind(memory.importance.clamp(1, 10) as i16)
          .bind(&memory.actor_name)
          .bind(labels)
          .bind(at)
          .bind(marks.locked)
          .bind(marks.internal_only)
          .bind(marks.shared)
          .fetch_one(&mut *tx)
          .await?;
          sqlx::query("UPDATE memories SET lineage_id = id WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
          link_subjects(&mut tx, id, &subject_ids).await?;
          injected.memories_added += 1;
        }
      }
    }
    publish(&mut tx, "brain.injected", Some(&super::brains::event_scope(brain_id)), &injected).await?;
    self.commit(tx).await?;
    Ok(injected)
  }
}
