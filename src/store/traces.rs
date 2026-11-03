//! A memory's traces and its brain's psychological clock (docs/design.md, 「Strength and decay」): the clock
//! advanced by every call that touches the brain, a trace laid on each presentation of a memory with the
//! decay its activation then gives it, the traces a new version carries over, and the activation and the
//! stability a reading answers.

use std::collections::BTreeMap;

use sqlx::PgConnection;

use super::rules::{Clock, Reckoning as _, Rules, Trace};
use super::{Held, Store};

/// What laid a trace (docs/design.md, 「Strength and decay」).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
  /// The memory was written: its first encoding.
  Written,
  /// Recall handed it to a turn.
  Handed,
  /// The agent reported it used.
  Used,
  /// A source apart from the reading confirmed it.
  Confirmed,
  /// An explicit read by key, subject, type or words answered it.
  Read,
}

impl TraceKind {
  pub fn name(self) -> &'static str {
    match self {
      Self::Written => "written",
      Self::Handed => "handed",
      Self::Used => "used",
      Self::Confirmed => "confirmed",
      Self::Read => "read",
    }
  }
}

/// The brain's clock as it stands, locked for the rest of the transaction where `for_update` is set.
async fn clock_of(conn: &mut PgConnection, brain_id: i64, for_update: bool) -> Held<Clock> {
  let lock = if for_update { " FOR UPDATE" } else { "" };
  let (real_at, psych): (Option<i64>, f64) =
    sqlx::query_as(&format!("SELECT clock_real_at, clock_psych FROM brains WHERE id = $1{lock}"))
      .bind(brain_id)
      .fetch_one(&mut *conn)
      .await?;
  Ok(Clock { real_at, psych })
}

/// The traces of each of these rows, by row id, oldest first.
pub(crate) async fn traces_by_row(conn: &mut PgConnection, ids: &[i64]) -> Held<BTreeMap<i64, Vec<Trace>>> {
  let rows: Vec<(i64, f64, f64, f64)> = sqlx::query_as(
    "SELECT memory_id, psych_at, decay, salience FROM memory_traces WHERE memory_id = ANY($1) \
     ORDER BY memory_id, psych_at, id",
  )
  .bind(ids)
  .fetch_all(&mut *conn)
  .await?;
  let mut by_row: BTreeMap<i64, Vec<Trace>> = BTreeMap::new();
  for (memory_id, psych_at, decay, salience) in rows {
    by_row.entry(memory_id).or_default().push(Trace {
      psych_at,
      decay,
      salience,
    });
  }
  Ok(by_row)
}

/// The salience of a memory's written trace: its importance over five.
pub(crate) fn written_salience(importance: i16) -> f64 {
  f64::from(importance) / 5.0
}

impl Store {
  /// The brain's clock advanced by a call at `at`; the clock's moment for `at`. The brain's row stays locked
  /// to the transaction, so a transaction advances the clock before it locks any memory.
  pub(crate) async fn tick(&self, conn: &mut PgConnection, brain_id: i64, at: i64) -> Held<f64> {
    let clock = clock_of(conn, brain_id, true).await?;
    let moment = self.rules.psych_moment(clock, at);
    let advanced = self.rules.advanced(clock, at);
    if advanced != clock {
      sqlx::query("UPDATE brains SET clock_real_at = $2, clock_psych = $3 WHERE id = $1")
        .bind(brain_id)
        .bind(advanced.real_at)
        .bind(advanced.psych)
        .execute(&mut *conn)
        .await?;
    }
    Ok(moment)
  }

  /// The moment a reading at `now` reads at on each of these brains' clocks, the clocks left as they stand.
  pub(crate) async fn reading_moments(
    &self,
    conn: &mut PgConnection,
    brain_ids: &[i64],
    now: i64,
  ) -> Held<BTreeMap<i64, f64>> {
    let rows: Vec<(i64, Option<i64>, f64)> =
      sqlx::query_as("SELECT id, clock_real_at, clock_psych FROM brains WHERE id = ANY($1)")
        .bind(brain_ids)
        .fetch_all(&mut *conn)
        .await?;
    Ok(
      rows
        .into_iter()
        .map(|(id, real_at, psych)| (id, self.rules.reading_moment(Clock { real_at, psych }, now)))
        .collect(),
    )
  }

  /// A trace of `kind` laid on each of these rows at the real moment `at`, the clock advanced to it, each
  /// trace's decay given by its row's activation then, and each row's traces kept to the rules' number.
  pub(crate) async fn lay(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
    memory_ids: &[i64],
    kind: TraceKind,
    session: Option<&str>,
    at: i64,
  ) -> Held<()> {
    let moment = self.tick(conn, brain_id, at).await?;
    self.lay_at(conn, memory_ids, kind, session, at, moment, |_| 1.0).await
  }

  /// A memory's written trace, its first encoding, laid at the real moment `at` with its importance as its
  /// salience.
  pub(crate) async fn lay_written(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
    memory_id: i64,
    importance: i16,
    at: i64,
  ) -> Held<()> {
    let moment = self.tick(conn, brain_id, at).await?;
    let salience = written_salience(importance);
    self
      .lay_at(conn, &[memory_id], TraceKind::Written, None, at, moment, |_| salience)
      .await
  }

  #[allow(clippy::too_many_arguments)]
  async fn lay_at(
    &self,
    conn: &mut PgConnection,
    memory_ids: &[i64],
    kind: TraceKind,
    session: Option<&str>,
    at: i64,
    moment: f64,
    salience: impl Fn(i64) -> f64,
  ) -> Held<()> {
    if memory_ids.is_empty() {
      return Ok(());
    }
    let traces = traces_by_row(conn, memory_ids).await?;
    let decays: Vec<f64> = memory_ids
      .iter()
      .map(|id| self.rules.decay_at(traces.get(id).map_or(&[][..], Vec::as_slice), moment))
      .collect();
    let saliences: Vec<f64> = memory_ids.iter().map(|id| salience(*id)).collect();
    sqlx::query(
      "INSERT INTO memory_traces (memory_id, kind, real_at, psych_at, decay, salience, session_key) \
       SELECT id, $2, $3, $4, decay, salience, $5 \
       FROM UNNEST($1::bigint[], $6::float8[], $7::float8[]) AS laid (id, decay, salience)",
    )
    .bind(memory_ids)
    .bind(kind.name())
    .bind(at)
    .bind(moment)
    .bind(session)
    .bind(&decays)
    .bind(&saliences)
    .execute(&mut *conn)
    .await?;
    self.keep_traces(conn, memory_ids).await
  }

  /// Each of these rows' traces kept to the rules' number: its written trace and the newest of the rest.
  async fn keep_traces(&self, conn: &mut PgConnection, memory_ids: &[i64]) -> Held<()> {
    let others = (self.rules.hit_times_kept - 1).max(0);
    sqlx::query(
      "DELETE FROM memory_traces WHERE id IN (SELECT id FROM (SELECT id, ROW_NUMBER() OVER \
       (PARTITION BY memory_id ORDER BY psych_at DESC, id DESC) AS newest FROM memory_traces \
       WHERE memory_id = ANY($1) AND kind <> 'written') ranked WHERE newest > $2)",
    )
    .bind(memory_ids)
    .bind(others)
    .execute(&mut *conn)
    .await?;
    Ok(())
  }

  /// The traces of the row `from` carried over to the new row `to`, its written trace's salience taken from
  /// the new row's importance.
  pub(crate) async fn carry_traces(
    &self,
    conn: &mut PgConnection,
    from: i64,
    to: i64,
    importance: i16,
  ) -> Held<()> {
    sqlx::query(
      "INSERT INTO memory_traces (memory_id, kind, real_at, psych_at, decay, salience, session_key) \
       SELECT $2, kind, real_at, psych_at, decay, CASE WHEN kind = 'written' THEN $3 ELSE salience END, \
       session_key FROM memory_traces WHERE memory_id = $1 ORDER BY psych_at, id",
    )
    .bind(from)
    .bind(to)
    .bind(written_salience(importance))
    .execute(&mut *conn)
    .await?;
    Ok(())
  }
}

/// The activation and the stability of a row with these traces, read at `moment`.
pub(crate) fn standing(rules: &Rules, traces: &[Trace], moment: f64) -> (Option<f64>, Option<f64>) {
  (rules.activation(traces, moment), rules.stability(traces, moment))
}
