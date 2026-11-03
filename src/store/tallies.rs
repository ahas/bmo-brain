//! A brain's memories counted by group (docs/boundary-api.md, 「Memories」): how many stand in each status,
//! type, layer or subject under the memory list's filters, and how important, how stable and how active they
//! are on average.

use std::collections::BTreeMap;

use bmo_brain_client::{MemoriesQuery, TalliesQuery, Tally, TallyBy};
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::brains::brain_of;
use super::traces::{standing, traces_by_row};
use super::versions::Filter;
use super::{Held, Store, now};

#[derive(FromRow)]
struct Counted {
  grouped: Option<String>,
  id: i64,
  importance: i16,
  valid_from: i64,
}

/// A mean over the values given, the ones missing left out.
#[derive(Default)]
struct Mean {
  sum: f64,
  count: i64,
}

impl Mean {
  fn add(&mut self, value: Option<f64>) {
    if let Some(value) = value.filter(|value| value.is_finite()) {
      self.sum += value;
      self.count += 1;
    }
  }

  fn value(&self) -> Option<f64> {
    (self.count > 0).then(|| self.sum / self.count as f64)
  }
}

/// What a group gathers as its rows are read.
#[derive(Default)]
struct Sums {
  count: i64,
  importance: f64,
  stability: Mean,
  activation: Mean,
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
      "SELECT {grouped} AS grouped, m.id, m.importance, m.valid_from \
       FROM (SELECT * FROM memories WHERE brain_id = "
    ));
    sql.push_bind(brain_id);
    filter.push(&mut sql);
    sql.push(format!(") m{joined}"));
    let rows: Vec<Counted> = sql.build_query_as().fetch_all(&mut *conn).await?;
    let mut ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    ids.sort_unstable();
    ids.dedup();
    let traces = traces_by_row(&mut conn, &ids).await?;
    let moment = self
      .reading_moments(&mut conn, &[brain_id], now())
      .await?
      .get(&brain_id)
      .copied()
      .unwrap_or(0.0);
    let standings: BTreeMap<i64, (Option<f64>, Option<f64>)> = ids
      .iter()
      .map(|id| (*id, standing(&self.rules, traces.get(id).map_or(&[][..], Vec::as_slice), moment)))
      .collect();
    let mut groups: BTreeMap<Option<String>, Sums> = BTreeMap::new();
    for row in rows {
      let sums = groups.entry(row.grouped).or_default();
      let (activation, stability) = standings.get(&row.id).copied().unwrap_or_default();
      sums.count += 1;
      sums.importance += f64::from(row.importance);
      sums.stability.add(stability);
      sums.activation.add(activation);
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
          stability: sums.stability.value(),
          activation: sums.activation.value(),
          latest_valid_from: sums.latest_valid_from,
        }
      })
      .collect();
    tallies.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.group.cmp(&b.group)));
    Ok(tallies)
  }
}
