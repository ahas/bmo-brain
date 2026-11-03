//! What a brain counts of its asks by words (docs/design.md, 「What the brain counts」): each ask counted, each
//! that answered no row kept as a miss with its words and stems, the newest kept to the rules' number.

use bmo_brain_client::{MissesQuery, Page, SearchCount, SearchMiss};
use sqlx::{FromRow, PgConnection};

use super::brains::brain_of;
use super::words::stems;
use super::{Fault, Held, Store, limit, now, page};

/// Which ask by words a miss came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
  /// A similar ask: the candidate's text.
  Similar,
  /// A memory list by `q`.
  Memories,
}

impl Asked {
  fn name(self) -> &'static str {
    match self {
      Self::Similar => "similar",
      Self::Memories => "memories",
    }
  }
}

#[derive(FromRow)]
struct MissRow {
  id: i64,
  brain_id: i64,
  at: i64,
  route: String,
  words: String,
  stems: Vec<String>,
}

impl Store {
  /// An ask by words counted, and kept as a miss where it answered no row; an ask with no words is neither.
  pub(crate) async fn count_ask(
    &self,
    conn: &mut PgConnection,
    brain_id: i64,
    asked: Asked,
    words: &str,
    answered: bool,
  ) -> Held<()> {
    if words.trim().is_empty() {
      return Ok(());
    }
    let missed = i64::from(!answered);
    sqlx::query(
      "INSERT INTO search_counts (brain_id, asks, misses) VALUES ($1, 1, $2) ON CONFLICT (brain_id) \
       DO UPDATE SET asks = search_counts.asks + 1, misses = search_counts.misses + $2",
    )
    .bind(brain_id)
    .bind(missed)
    .execute(&mut *conn)
    .await?;
    if answered {
      return Ok(());
    }
    let stemmed: Vec<String> = stems(words).into_iter().collect();
    sqlx::query(
      "INSERT INTO search_misses (brain_id, at, route, words, stems) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(brain_id)
    .bind(now())
    .bind(asked.name())
    .bind(words)
    .bind(&stemmed)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
      "DELETE FROM search_misses WHERE brain_id = $1 AND id <= (SELECT id FROM search_misses WHERE brain_id = $1 \
       ORDER BY id DESC OFFSET $2 LIMIT 1)",
    )
    .bind(brain_id)
    .bind(self.rules.misses_kept.max(0))
    .execute(&mut *conn)
    .await?;
    Ok(())
  }

  /// A page of a brain's kept misses, newest first.
  pub async fn misses(&self, brain_id: i64, query: &MissesQuery) -> Held<Page<SearchMiss>> {
    let limit = limit(query.limit)?;
    let mut conn = self.connection().await?;
    brain_of(&mut conn, brain_id).await?;
    let rows: Vec<MissRow> = sqlx::query_as(
      "SELECT id, brain_id, at, route, words, stems FROM search_misses WHERE brain_id = $1 \
       AND ($2::bigint IS NULL OR id < $2) ORDER BY id DESC LIMIT $3",
    )
    .bind(brain_id)
    .bind(query.after)
    .bind(limit + 1)
    .fetch_all(&mut *conn)
    .await?;
    let items = rows
      .into_iter()
      .map(|row| SearchMiss {
        id: row.id,
        brain_id: row.brain_id,
        at: row.at,
        route: row.route,
        words: row.words,
        stems: row.stems,
      })
      .collect();
    Ok(page(items, limit))
  }

  /// Every asked brain's count of its asks by words and of its misses; none where the database does not answer.
  pub async fn search_counts(&self) -> Vec<SearchCount> {
    let counted: Result<Vec<(i64, i64, i64)>, Fault> =
      sqlx::query_as("SELECT brain_id, asks, misses FROM search_counts ORDER BY brain_id")
        .fetch_all(&self.pool)
        .await
        .map_err(Fault::from);
    counted
      .unwrap_or_default()
      .into_iter()
      .map(|(brain_id, asks, misses)| SearchCount {
        brain_id,
        asks,
        misses,
      })
      .collect()
  }
}
