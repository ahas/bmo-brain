//! What the brain reckons by its rules (docs/design.md, 「Strength and decay」 and 「Recall」): the retrieval
//! strength of a row from its hits, what a confirmation adds to the storage strength, and a text's tokens. The
//! rules themselves are the client's `Rules`, read from the configuration and named in the health answer.

pub use bmo_brain_client::Rules;

/// What the rules reckon.
pub trait Reckoning {
  /// The retrieval strength of a row hit at these moments, read at `now`: the logarithm of the sum over its
  /// hits of the days since each, raised to the negated decay. No hit is a strength of zero.
  fn retrieval_strength(&self, hit_times: &[i64], now: i64) -> f64;
  /// What a confirmation at `at` adds to the storage strength of a row last confirmed (or written) at
  /// `since`: nothing within the same session, else the gain times the logarithm of the gap in days.
  fn strength_gained(&self, since: i64, at: i64) -> f64;
}

impl Reckoning for Rules {
  fn retrieval_strength(&self, hit_times: &[i64], now: i64) -> f64 {
    if hit_times.is_empty() {
      return 0.0;
    }
    let sum: f64 = hit_times
      .iter()
      .map(|&at| {
        let days = (now - at).max(60_000) as f64 / 86_400_000.0;
        days.powf(-self.decay)
      })
      .sum();
    sum.ln()
  }

  fn strength_gained(&self, since: i64, at: i64) -> f64 {
    let gap = at - since;
    if gap < self.same_session_ms {
      return 0.0;
    }
    self.strength_gain * (1.0 + gap as f64 / 86_400_000.0).ln()
  }
}

/// The tokens a text is reckoned at: one for every three characters, rounded up.
pub fn tokens(text: &str) -> i64 {
  let chars = text.chars().count() as i64;
  (chars + 2) / 3
}
