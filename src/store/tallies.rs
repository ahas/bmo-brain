//! A brain's memories counted by group (docs/boundary-api.md, 「Memories」): how many stand in each status,
//! type, layer or subject under the memory list's filters, and how important, how strong and how warm they
//! are on average.

use std::collections::BTreeMap;

use bmo_brain_client::{MemoriesQuery, TalliesQuery, Tally, TallyBy};
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::brains::brain_of;
use super::rules::Reckoning as _;
use super::versions::Filter;
use super::{Held, Store, now};

#[derive(FromRow)]
struct Counted {
  grouped: Option<String>,
  importance: i16,
  strength: f64,
  valid_from: i64,
  hit_times: Vec<i64>,
}

/// What a group gathers as its rows are read.
#[derive(Default)]
struct Sums {
  count: i64,
  importance: f64,
  strength: f64,
  retrieval_strength: f64,
  latest_valid_from: i64,
}

impl Store {
  /// The groups of a brain's memories under the filters the query names, the largest first.
  pub async fn tallies(&self, brain_id: i64, query: &TalliesQuery) -> Held<Vec<Tally>> {
    let filter = Filter::of(&MemoriesQuery {
      kind: query.kind.clone(),
      status: query.status.clone(),
      exclude_types: query.exclude_types.clone(),
      layer_id: query.layer_id,
      unfiled: query.unfiled,
      subject_id: query.subject_id,
      key: query.key.clone(),
      lineage_id: query.lineage_id,
      ids: query.ids.clone(),
      superseded_by: query.superseded_by.clone(),
      label: query.label.clone(),
      q: query.q.clone(),
      ..MemoriesQuery::default()
    })?;
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let (grouped, joined) = match query.by {
      TallyBy::Status => ("m.status", ""),
      TallyBy::Type => ("m.type", ""),
      TallyBy::Layer => ("m.layer_id::text", ""),
      TallyBy::Subject => ("ms.subject_id::text", " LEFT JOIN memory_subjects ms ON ms.memory_id = m.id"),
    };
    let mut sql = QueryBuilder::<Postgres>::new(format!(
      "SELECT {grouped} AS grouped, m.importance, m.strength, m.valid_from, m.hit_times \
       FROM (SELECT * FROM memories WHERE brain_id = "
    ));
    sql.push_bind(brain_id);
    filter.push(&mut sql);
    sql.push(format!(") m{joined}"));
    let rows: Vec<Counted> = sql.build_query_as().fetch_all(&mut *conn).await?;
    let at = now();
    let mut groups: BTreeMap<Option<String>, Sums> = BTreeMap::new();
    for row in rows {
      let sums = groups.entry(row.grouped).or_default();
      sums.count += 1;
      sums.importance += f64::from(row.importance);
      sums.strength += row.strength;
      sums.retrieval_strength += self.rules.retrieval_strength(&row.hit_times, at);
      sums.latest_valid_from = sums.latest_valid_from.max(row.valid_from);
    }
    let mut tallies: Vec<Tally> = groups
      .into_iter()
      .map(|(group, sums)| {
        let count = sums.count as f64;
        Tally {
          group,
          count: sums.count,
          importance: sums.importance / count,
          strength: sums.strength / count,
          retrieval_strength: sums.retrieval_strength / count,
          latest_valid_from: sums.latest_valid_from,
        }
      })
      .collect();
    tallies.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.group.cmp(&b.group)));
    Ok(tallies)
  }
}
