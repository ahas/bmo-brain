//! A memory's traces and its brain's psychological clock (docs/design.md, 「Strength and decay」): the
//! activation reckoned from a known set of traces, the decay a trace takes from the activation at its laying,
//! the clock across a gap between sessions, the stability solved, the hits a row kept made traces by the
//! migration, explicit reads laying traces, and a used trace joined to the hand-out before it.

use crate::support;

use bmo_brain::store::rules::{Clock, Reckoning, Rules, Trace};
use bmo_brain_client::{BrainClient, Hits, MemoriesQuery, NewBrain, NewMemory, RecallAsk, SubjectKey};
use support::{Bare, Served, labels};

const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

fn memory(kind: &str, text: &str) -> NewMemory {
  NewMemory {
    key: None,
    kind: kind.to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects: vec![],
    importance: None,
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
  }
}

async fn brain_of(client: &BrainClient) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
    })
    .await
    .unwrap()
    .id
}

fn trace(psych_at: f64, decay: f64, salience: f64) -> Trace {
  Trace {
    psych_at,
    decay,
    salience,
  }
}

fn near(value: f64, expected: f64, tolerance: f64) -> bool {
  (value - expected).abs() <= tolerance
}

#[test]
fn the_activation_is_the_log_of_each_traces_salience_times_its_age_to_its_negated_decay() {
  let rules = Rules::default();
  // Two traces read at 100 psychological seconds: one laid at 0 with decay 0.5 and salience 1, 100 seconds
  // old; one laid at 90 seconds with decay 0.4 and salience 2, 10 seconds old.
  let traces = [trace(0.0, 0.5, 1.0), trace(90_000.0, 0.4, 2.0)];
  let expected = (100_f64.powf(-0.5) + 2.0 * 10_f64.powf(-0.4)).ln();
  let activation = rules.activation(&traces, 100_000.0).unwrap();
  assert!(near(activation, expected, 1e-12), "{activation} against {expected}");
  assert!(near(activation, -0.109_576, 1e-5), "{activation}");
  // A trace laid after the moment read does not count yet, and no trace is no activation.
  let first = rules.activation(&traces, 50_000.0).unwrap();
  assert!(near(first, 50_f64.powf(-0.5).ln(), 1e-12), "{first}");
  assert_eq!(rules.activation(&[], 1_000.0), None);
  // A trace read at its own moment is reckoned a second old.
  assert!(near(rules.activation(&[trace(5_000.0, 0.3, 1.0)], 5_000.0).unwrap(), 0.0, 1e-12));
  // The probability of coming to mind is even at the threshold.
  assert!(near(rules.recall_probability(rules.threshold), 0.5, 1e-12));
}

#[test]
fn a_trace_laid_right_after_another_decays_faster_than_one_laid_apart() {
  let rules = Rules::default();
  let first = [trace(0.0, rules.decay_intercept, 1.0)];
  // The first trace's decay is the intercept alone.
  assert_eq!(rules.decay_at(&[], 0.0), rules.decay_intercept);
  // Laid a second after: the activation then is ln 1, so the decay is c + a.
  let massed = rules.decay_at(&first, 1_000.0);
  assert!(near(massed, 0.232 + 0.167, 1e-12), "{massed}");
  // Laid a day after: the first trace has faded to 86400 ^ -0.167.
  let spaced = rules.decay_at(&first, 86_400_000.0);
  let expected = 0.232 * 86_400_f64.powf(-0.167) + 0.167;
  assert!(near(spaced, expected, 1e-12), "{spaced} against {expected}");
  assert!(massed > spaced);
  // A week after the second trace, the memory studied apart stands above the one studied at once.
  let week = 7.0 * 86_400_000.0;
  let crammed = [first[0], trace(1_000.0, massed, 1.0)];
  let apart = [first[0], trace(86_400_000.0, spaced, 1.0)];
  let crammed_then = rules.activation(&crammed, 1_000.0 + week).unwrap();
  let apart_then = rules.activation(&apart, 86_400_000.0 + week).unwrap();
  assert!(apart_then > crammed_then, "{apart_then} > {crammed_then}");
}

#[test]
fn the_psychological_clock_counts_a_gap_within_a_session_whole_and_a_longer_one_by_its_share() {
  let rules = Rules::default();
  let fresh = Clock {
    real_at: None,
    psych: 0.0,
  };
  assert_eq!(rules.psych_moment(fresh, 1_000), 0.0);
  let started = rules.advanced(fresh, 1_000);
  assert_eq!(
    started,
    Clock {
      real_at: Some(1_000),
      psych: 0.0
    }
  );
  // Half an hour later counts whole; two hours after that count 0.031 of themselves.
  let within = rules.advanced(started, 1_000 + HOUR / 2);
  assert_eq!(within.psych, (HOUR / 2) as f64);
  let after = rules.advanced(within, 1_000 + HOUR / 2 + 2 * HOUR);
  assert!(
    near(after.psych, (HOUR / 2) as f64 + 0.031 * (2 * HOUR) as f64, 1e-6),
    "{}",
    after.psych
  );
  // A moment before the clock's latest call is placed behind it, the clock not moving back.
  assert_eq!(rules.psych_moment(within, 1_000 + HOUR / 4), (HOUR / 4) as f64);
  assert_eq!(rules.advanced(within, 1_000), within);
  // A reading before the clock's latest call reads at the clock's moment.
  assert_eq!(rules.reading_moment(after, 0), after.psych);
}

#[tokio::test]
async fn a_hit_days_later_reads_its_age_on_the_psychological_clock() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let row = client.memory_add(brain, &memory("fact", "A fact")).await.unwrap();
  // The write started the clock at the row's moment; a hit three days later moves it on by 0.031 of the gap.
  client
    .hits_record(
      brain,
      &Hits {
        memory_ids: vec![row.id],
        session_key: None,
        actor_name: "reader-a".to_owned(),
        at: Some(row.created_at + 3 * DAY),
      },
    )
    .await
    .unwrap();
  let read = client.memory(brain, row.id).await.unwrap();
  // Read now, before the clock's latest call, it reads at the clock's moment: the written trace is 0.031 of
  // three days old in seconds, and the used trace a second old.
  let age = 0.031 * (3 * DAY) as f64 / 1_000.0;
  let expected = (age.powf(-0.167) + 1.0).ln();
  let activation = read.activation.unwrap();
  assert!(near(activation, expected, 1e-9), "{activation} against {expected}");
}

#[test]
fn the_stability_is_the_time_until_the_activation_falls_to_the_threshold() {
  let rules = Rules::default();
  // One trace of salience 1 and decay a: -a ln t = tau at t = e^(-tau / a) seconds.
  let one = [trace(0.0, 0.167, 1.0)];
  let expected = (0.669_f64 / 0.167).exp() / 86_400.0;
  let stability = rules.stability(&one, 0.0).unwrap();
  assert!(near(stability, expected, expected * 1e-5), "{stability} against {expected}");
  // Ten seconds on, ten seconds fewer are left.
  let later = rules.stability(&one, 10_000.0).unwrap();
  assert!(near(later, expected - 10.0 / 86_400.0, expected * 1e-5), "{later}");
  // A salience of two: ln 2 - a ln t = tau.
  let salient = [trace(0.0, 0.167, 2.0)];
  let expected = ((2_f64.ln() + 0.669) / 0.167).exp() / 86_400.0;
  let stability = rules.stability(&salient, 0.0).unwrap();
  assert!(near(stability, expected, expected * 1e-5), "{stability} against {expected}");
  // Below the threshold already, nothing is left; with no trace, nothing either; with no decay, it never falls.
  assert_eq!(rules.stability(&one, 3_600_000.0), Some(0.0));
  assert_eq!(rules.stability(&[], 0.0), Some(0.0));
  assert_eq!(rules.stability(&[trace(0.0, 0.0, 1.0)], 0.0), None);
}

#[tokio::test]
async fn the_migration_makes_each_kept_hit_a_trace_on_the_brains_clock() {
  let bare = Bare::through(7).await;
  let mut db = bare.connect().await;
  let start: i64 = 1_700_000_000_000;
  let brain: i64 = sqlx::query_scalar(
    "INSERT INTO brains (account_key, agent_key, name, created_at) VALUES ('account-a', 'agent-a', 'a', $1) \
     RETURNING id",
  )
  .bind(start)
  .fetch_one(&mut db)
  .await
  .unwrap();
  let insert = "INSERT INTO memories (brain_id, type, text, status, lineage_id, valid_from, importance, hits, \
    hit_times, actor_name, labels, created_at) VALUES ($1, 'fact', $2, 'active', 0, $3, $4, $5, $6, 'writer-a', \
    '{}', $3) RETURNING id";
  // One row written at the start and hit half an hour and three hours on; one written half an hour on.
  let hits = vec![start + HOUR / 2, start + 3 * HOUR];
  let first: i64 = sqlx::query_scalar(insert)
    .bind(brain)
    .bind("Hit twice")
    .bind(start)
    .bind(10_i16)
    .bind(2_i64)
    .bind(&hits)
    .fetch_one(&mut db)
    .await
    .unwrap();
  let second: i64 = sqlx::query_scalar(insert)
    .bind(brain)
    .bind("Never hit")
    .bind(start + HOUR / 2)
    .bind(5_i16)
    .bind(0_i64)
    .bind(Vec::<i64>::new())
    .fetch_one(&mut db)
    .await
    .unwrap();
  bmo_brain::store::MIGRATOR.run(&mut db).await.unwrap();

  let traces: Vec<(i64, String, i64, f64, f64, f64)> = sqlx::query_as(
    "SELECT memory_id, kind, real_at, psych_at, decay, salience FROM memory_traces ORDER BY memory_id, psych_at, id",
  )
  .fetch_all(&mut db)
  .await
  .unwrap();
  // The moments in order: the start, half an hour on (whole), three hours on (2.5 hours at 0.031).
  let half = (HOUR / 2) as f64;
  let late = half + 0.031 * (5 * HOUR / 2) as f64;
  assert_eq!(
    traces,
    [
      (first, "written".to_owned(), start, 0.0, 0.167, 2.0),
      (first, "handed".to_owned(), start + HOUR / 2, half, 0.167, 1.0),
      (first, "handed".to_owned(), start + 3 * HOUR, late, 0.167, 1.0),
      (second, "written".to_owned(), start + HOUR / 2, half, 0.167, 1.0),
    ]
  );
  let clock: (Option<i64>, f64) =
    sqlx::query_as("SELECT clock_real_at, clock_psych FROM brains WHERE id = $1")
      .bind(brain)
      .fetch_one(&mut db)
      .await
      .unwrap();
  assert_eq!(clock, (Some(start + 3 * HOUR), late));
  let gone: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'memories' \
     AND column_name IN ('hit_times', 'strength')",
  )
  .fetch_one(&mut db)
  .await
  .unwrap();
  assert_eq!(gone, 0);
}

#[tokio::test]
async fn an_explicit_read_lays_a_trace_and_the_screens_list_does_not() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let row = client
    .memory_add(brain, &memory("fact", "The harvest starts in autumn."))
    .await
    .unwrap();
  let before = client.memory(brain, row.id).await.unwrap().activation.unwrap();
  // A list by status alone is no explicit read, and a read that says not to record lays nothing.
  for query in [
    MemoriesQuery {
      status: Some("active".to_owned()),
      ..MemoriesQuery::default()
    },
    MemoriesQuery {
      q: Some("harvest".to_owned()),
      record: Some(false),
      ..MemoriesQuery::default()
    },
  ] {
    client.memories(brain, &query).await.unwrap();
    let after = client.memory(brain, row.id).await.unwrap().activation.unwrap();
    assert!(after <= before + 1e-9, "{after} against {before}");
  }
  // A read by words lays a read trace, which raises the activation.
  let read = client
    .memories(
      brain,
      &MemoriesQuery {
        q: Some("harvest".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(read.items.len(), 1);
  let after = client.memory(brain, row.id).await.unwrap().activation.unwrap();
  assert!(after > before + 0.3, "{after} against {before}");
  let kinds: Vec<String> =
    sqlx::query_scalar("SELECT kind FROM memory_traces WHERE memory_id = $1 ORDER BY id")
      .bind(row.id)
      .fetch_all(&served.store.pool)
      .await
      .unwrap();
  assert_eq!(kinds, ["written", "read"]);
}

#[tokio::test]
async fn a_used_trace_is_joined_to_the_hand_out_that_preceded_it_in_its_session() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let row = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![SubjectKey {
          kind: "room".to_owned(),
          key: "room:a".to_owned(),
        }],
        ..memory("fact", "The harvest starts in autumn.")
      },
    )
    .await
    .unwrap();
  let recall = client
    .recall(
      brain,
      &RecallAsk {
        room: Some(SubjectKey {
          kind: "room".to_owned(),
          key: "room:a".to_owned(),
        }),
        words: "when is the harvest".to_owned(),
        session_key: Some("session-a".to_owned()),
        ..RecallAsk::default()
      },
    )
    .await
    .unwrap();
  assert!(recall.chosen.iter().any(|choice| choice.memory_id == row.id), "{:?}", recall.chosen);
  client
    .hits_record(
      brain,
      &Hits {
        memory_ids: vec![row.id],
        session_key: Some("session-a".to_owned()),
        actor_name: "agent-session".to_owned(),
        at: None,
      },
    )
    .await
    .unwrap();
  // The join docs/design.md states: the newest handed trace of the same memory and session key, no later.
  let joined: Vec<(String, String)> = sqlx::query_as(
    "SELECT u.kind, h.kind FROM memory_traces u JOIN LATERAL (SELECT kind FROM memory_traces h \
     WHERE h.memory_id = u.memory_id AND h.kind = 'handed' AND h.session_key = u.session_key \
     AND h.real_at <= u.real_at ORDER BY h.real_at DESC, h.id DESC LIMIT 1) h ON TRUE \
     WHERE u.memory_id = $1 AND u.kind IN ('used', 'confirmed')",
  )
  .bind(row.id)
  .fetch_all(&served.store.pool)
  .await
  .unwrap();
  assert_eq!(joined, [("used".to_owned(), "handed".to_owned())]);
}
