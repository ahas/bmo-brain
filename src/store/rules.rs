//! What the brain reckons by its rules (docs/design.md, 「Strength and decay」 and 「Recall」): a memory's
//! activation from its traces, the decay of a trace laid now, the stability a memory has left, the
//! probability of its coming to mind, the psychological clock's moments, and a text's tokens. The rules
//! themselves are the client's `Rules`, read from the configuration and named in the health answer.

pub use bmo_brain_client::Rules;

/// The shortest age a trace is reckoned at, in psychological seconds, so a trace laid at the moment read
/// counts as one a second old.
const AGE_LEAST_S: f64 = 1.0;

/// The longest span, in psychological seconds, the stability is sought within; a memory still above the
/// threshold past it never falls.
const STABILITY_MOST_S: f64 = 1.0e15;

/// Seconds in a day.
const DAY_S: f64 = 86_400.0;

/// One trace as the reckoning reads it: its moment on the psychological clock in milliseconds, its decay and
/// its salience.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trace {
  pub psych_at: f64,
  pub decay: f64,
  pub salience: f64,
}

/// A brain's psychological clock: the real moment of the brain's latest call, none before its first, and
/// the clock's reading then, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
  pub real_at: Option<i64>,
  pub psych: f64,
}

/// What the rules reckon.
pub trait Reckoning {
  /// The psychological span a real gap between two calls counts as: whole within a session, the
  /// between-sessions share of itself past it.
  fn psych_span(&self, gap_ms: i64) -> f64;
  /// The clock's moment for the real moment `at`: past its latest call by the span of the gap, or behind it
  /// by the span of the gap back. A clock with no call yet reads zero.
  fn psych_moment(&self, clock: Clock, at: i64) -> f64;
  /// The clock after a call at `at`: moved on where `at` is later than its latest call, as it was otherwise.
  fn advanced(&self, clock: Clock, at: i64) -> Clock;
  /// The moment a reading at the real moment `now` reads at: the clock's moment for `now`, never earlier
  /// than the clock's latest call.
  fn reading_moment(&self, clock: Clock, now: i64) -> f64;
  /// The activation at the psychological moment `at`: the logarithm of the sum over the traces laid by then
  /// of each one's salience times its age in seconds raised to its negated decay; none where no trace was.
  fn activation(&self, traces: &[Trace], at: f64) -> Option<f64>;
  /// The decay of a trace laid at `at` over these traces: the decay scale times the exponential of the
  /// activation then, plus the decay intercept; the intercept alone where there is no activation.
  fn decay_at(&self, traces: &[Trace], at: f64) -> f64;
  /// The psychological days from `at` until the activation falls to the threshold with no trace laid; zero
  /// where it is below already or there is none, none where it never falls.
  fn stability(&self, traces: &[Trace], at: f64) -> Option<f64>;
  /// The probability the model gives a memory of this activation of coming to mind.
  fn recall_probability(&self, activation: f64) -> f64;
}

/// The sum the activation is the logarithm of, at `at`, over the traces laid by then.
fn trace_sum(traces: &[Trace], at: f64) -> Option<f64> {
  let mut counted = false;
  let mut sum = 0.0;
  for trace in traces.iter().filter(|trace| trace.psych_at <= at) {
    let age = ((at - trace.psych_at) / 1_000.0).max(AGE_LEAST_S);
    sum += trace.salience * age.powf(-trace.decay);
    counted = true;
  }
  (counted && sum > 0.0).then_some(sum)
}

impl Reckoning for Rules {
  fn psych_span(&self, gap_ms: i64) -> f64 {
    if gap_ms <= self.same_session_ms {
      gap_ms as f64
    } else {
      self.between_sessions * gap_ms as f64
    }
  }

  fn psych_moment(&self, clock: Clock, at: i64) -> f64 {
    match clock.real_at {
      None => 0.0,
      Some(real_at) if at >= real_at => clock.psych + self.psych_span(at - real_at),
      Some(real_at) => clock.psych - self.psych_span(real_at - at),
    }
  }

  fn advanced(&self, clock: Clock, at: i64) -> Clock {
    match clock.real_at {
      Some(real_at) if at <= real_at => clock,
      _ => Clock {
        real_at: Some(at),
        psych: self.psych_moment(clock, at),
      },
    }
  }

  fn reading_moment(&self, clock: Clock, now: i64) -> f64 {
    match clock.real_at {
      Some(real_at) if now < real_at => clock.psych,
      _ => self.psych_moment(clock, now),
    }
  }

  fn activation(&self, traces: &[Trace], at: f64) -> Option<f64> {
    trace_sum(traces, at).map(f64::ln)
  }

  fn decay_at(&self, traces: &[Trace], at: f64) -> f64 {
    match trace_sum(traces, at) {
      Some(sum) => self.decay_scale * sum + self.decay_intercept,
      None => self.decay_intercept,
    }
  }

  fn stability(&self, traces: &[Trace], at: f64) -> Option<f64> {
    let above = |span_s: f64| {
      self
        .activation(traces, at + span_s * 1_000.0)
        .is_some_and(|activation| activation >= self.threshold)
    };
    if !above(0.0) {
      return Some(0.0);
    }
    // The activation only falls as time passes, so the span is bracketed by doubling and then halved down.
    let mut low = 0.0;
    let mut high = AGE_LEAST_S;
    while above(high) {
      if high >= STABILITY_MOST_S {
        return None;
      }
      low = high;
      high *= 2.0;
    }
    while high - low > 1.0e-6 * high.max(1.0) {
      let middle = (low + high) / 2.0;
      if above(middle) {
        low = middle;
      } else {
        high = middle;
      }
    }
    Some(high / DAY_S)
  }

  fn recall_probability(&self, activation: f64) -> f64 {
    1.0 / (1.0 + ((self.threshold - activation) / self.noise).exp())
  }
}

/// The tokens a text is reckoned at: one for every three characters, rounded up.
pub fn tokens(text: &str) -> i64 {
  let chars = text.chars().count() as i64;
  (chars + 2) / 3
}
