//! The game as a host plays it: every event it says and every alarm it arms, on a clock that moves only
//! when the test moves it, with the book and the words it is played with read from
//! `fixtures/word-chain/game.toml`.

use std::sync::{Arc, LazyLock};

use rusqlite::Connection;
use serde::Deserialize;
use word_chain::policy::WORD_CHAIN;
use word_chain::{
  ChainRules, Difficulty, Elimination, Ending, GameEvent, GameSettings, Opening, Phase, Rejection,
  RobotSeat, SqliteWordBook, WordChainGame,
};

// The words each test plays, by what they are for.
#[derive(Deserialize)]
struct Words {
  book: Vec<String>,
  loanword: String,
  opening: String,
  opening_hands: Vec<String>,
  follows: String,
  follows_hands: Vec<String>,
  after_follows: Vec<String>,
  not_in_book: String,
  hanbang: String,
  initial_sound: String,
  initial_sound_hands: Vec<String>,
  by_initial_sound: String,
  not_moves: Vec<String>,
}

static W: LazyLock<Words> =
  LazyLock::new(|| toml::from_str(include_str!("fixtures/word-chain/game.toml")).unwrap());

const ROBOT: &str = "agent-a";

fn book() -> Arc<SqliteWordBook> {
  let db = Connection::open_in_memory().expect("an in-memory database");
  let words: Vec<(&str, bool)> = W
    .book
    .iter()
    .map(|word| (word.as_str(), false))
    .chain([(W.loanword.as_str(), true)])
    .collect();

  SqliteWordBook::write(&db, &words, &[]).expect("the fixture book");

  Arc::new(SqliteWordBook::over(db))
}

fn settings() -> GameSettings {
  GameSettings {
    rules: ChainRules {
      loanwords: false,
      manner: false,
    },
    turn_ms: Some(30_000),
    idle_ms: 600_000,
    recruit_ms: 60_000,
    robot_ms: 1_000,
    round_answers: 1,
    robot: None,
    opening: None,
  }
}

fn robot(difficulty: Difficulty) -> Option<RobotSeat> {
  Some(RobotSeat {
    id: ROBOT.to_owned(),
    name: "Agent A".to_owned(),
    difficulty,
  })
}

// A game and the clock it keeps time on.
struct Run {
  play: WordChainGame,
  now: u64,
  alarms: Vec<(u64, u64)>,
  events: Vec<GameEvent>,
}

impl Run {
  fn new(settings: GameSettings, players: &[&str]) -> Self {
    let mut run = Self {
      play: WordChainGame::new(book(), settings),
      now: 0,
      alarms: Vec::new(),
      events: Vec::new(),
    };

    run.play.open();

    for id in players {
      run.play.join(id, &id.to_uppercase());
    }

    run.play.begin();
    run.pump();
    run
  }

  fn pump(&mut self) {
    let now = self.now;

    self
      .alarms
      .extend(self.play.take_armed().into_iter().map(|alarm| (now + alarm.ms, alarm.id)));
    self.events.extend(self.play.take_said().into_iter().map(|said| said.event));
  }

  fn submit(&mut self, id: &str, word: &str) -> bool {
    let heard = self.play.submit(id, word);

    self.pump();
    heard
  }

  fn advance(&mut self, ms: u64) {
    let until = self.now + ms;

    while let Some(at) = self
      .alarms
      .iter()
      .enumerate()
      .filter(|(_, (due, _))| *due <= until)
      .min_by_key(|(_, (due, _))| *due)
      .map(|(at, _)| at)
    {
      let (due, id) = self.alarms.remove(at);

      self.now = due;
      self.play.fire(id);
      self.pump();
    }

    self.now = until;
  }

  fn current(&self) -> Option<String> {
    self.play.current().map(|player| player.id.clone())
  }

  fn last(&self) -> &GameEvent {
    self.events.last().expect("an event")
  }

  fn rejections(&self) -> Vec<Rejection> {
    self
      .events
      .iter()
      .filter_map(|event| match event {
        GameEvent::Rejected { reason, .. } => Some(*reason),
        _ => None,
      })
      .collect()
  }

  fn accepted(&self) -> Vec<(String, String)> {
    self
      .events
      .iter()
      .filter_map(|event| match event {
        GameEvent::Accepted { player, word, .. } => Some((player.id.clone(), word.clone())),
        _ => None,
      })
      .collect()
  }

  fn eliminated(&self) -> Option<(String, Elimination, Option<String>)> {
    self.events.iter().find_map(|event| match event {
      GameEvent::Eliminated {
        player, reason, by, ..
      } => Some((player.id.clone(), *reason, by.as_ref().map(|by| by.id.clone()))),
      _ => None,
    })
  }

  fn over(&self) -> (Ending, Vec<(String, i64)>) {
    match self.last() {
      GameEvent::Over { ranking, reason } => {
        (*reason, ranking.iter().map(|player| (player.id.clone(), player.score)).collect())
      }
      other => panic!("the game is not over: {other:?}"),
    }
  }
}

#[test]
fn a_game_opens_on_a_word_from_the_book_and_hands_the_first_move_to_the_first_joiner() {
  let run = Run::new(settings(), &["a", "b"]);

  assert!(run.events.iter().any(|event| matches!(
    event,
    GameEvent::Round { word, openings, first: true } if *word == W.opening && *openings == W.opening_hands
  )));
  assert_eq!(run.current().as_deref(), Some("a"));
  assert!(matches!(
    run.last(),
    GameEvent::Turn {
      seconds: Some(30),
      ..
    }
  ));
}

#[test]
fn a_word_that_follows_is_scored_and_the_move_passes_on() {
  let mut run = Run::new(settings(), &["a", "b"]);

  assert!(run.submit("a", &W.follows));
  assert!(run.events.iter().any(|event| matches!(
    event,
    GameEvent::Accepted { word, points, .. } if *word == W.follows && *points == 2 * WORD_CHAIN.points.per_syllable
  )));
  assert_eq!(run.current().as_deref(), Some("b"));
}

#[test]
fn a_word_not_in_the_book_used_or_off_the_chain_is_refused_and_the_move_kept() {
  let mut run = Run::new(settings(), &["a", "b"]);

  run.submit("a", &W.not_in_book);
  run.submit("a", &W.follows);
  run.submit("b", &W.after_follows[0]);
  run.submit("a", &W.follows);
  assert!(run.submit("a", &W.after_follows[1]));

  assert_eq!(run.rejections(), [Rejection::NotInBook, Rejection::Used, Rejection::OffChain]);
  assert!(
    matches!(run.last(), GameEvent::Rejected { word, openings, .. } if *word == W.after_follows[1] && *openings == W.opening_hands)
  );
  assert_eq!(run.current().as_deref(), Some("a"));
}

#[test]
fn a_word_may_open_with_the_initial_sound_form_of_the_last_syllable() {
  let mut run = Run::new(settings(), &["a", "b"]);

  run.submit("a", &W.initial_sound);
  assert_eq!(run.play.openings(), W.initial_sound_hands);
  run.submit("b", &W.by_initial_sound);
  assert_eq!(
    run.accepted(),
    [("a".to_owned(), W.initial_sound.clone()), ("b".to_owned(), W.by_initial_sound.clone())]
  );
}

#[test]
fn nobody_out_of_turn_is_heard_nor_anything_that_is_not_a_word() {
  let mut run = Run::new(settings(), &["a", "b"]);

  assert!(!run.submit("b", &W.follows));

  for text in &W.not_moves {
    assert!(!run.submit("a", text), "{text}");
  }
}

#[test]
fn a_hanbang_puts_the_next_player_out_at_once_and_ends_a_two_player_game_with_scores() {
  let mut run = Run::new(settings(), &["a", "b"]);

  run.submit("a", &W.hanbang);

  assert_eq!(run.eliminated(), Some(("b".to_owned(), Elimination::Hanbang, Some("a".to_owned()))));

  let points = &WORD_CHAIN.points;
  let syllables = W.hanbang.chars().count() as i64;

  assert_eq!(
    run.over(),
    (
      Ending::Decided,
      vec![
        ("a".to_owned(), syllables * points.per_syllable + points.hanbang + points.survivor),
        ("b".to_owned(), 0)
      ]
    )
  );
}

#[test]
fn the_manner_rule_refuses_a_hanbang_word_and_a_loanword_is_refused_where_loanwords_are_off() {
  let mut run = Run::new(
    GameSettings {
      rules: ChainRules {
        loanwords: false,
        manner: true,
      },
      ..settings()
    },
    &["a", "b"],
  );

  run.submit("a", &W.hanbang);
  run.submit("a", &W.loanword);

  assert_eq!(run.rejections(), [Rejection::Manner, Rejection::Loanword]);
}

#[test]
fn a_player_whose_time_runs_down_is_out() {
  let mut run = Run::new(settings(), &["a", "b"]);

  run.advance(30_000);

  assert_eq!(run.eliminated(), Some(("a".to_owned(), Elimination::Timeout, None)));
  assert_eq!(run.over().1.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
}

#[test]
fn with_no_limit_the_move_is_waited_for_and_a_game_nobody_plays_closes_with_nobody_out() {
  let mut run = Run::new(
    GameSettings {
      turn_ms: None,
      ..settings()
    },
    &["a", "b"],
  );

  assert!(matches!(run.last(), GameEvent::Turn { seconds: None, .. }));
  run.advance(599_000);
  assert!(run.submit("a", &W.follows));
  // The word taken starts the wait again for the next player.
  run.advance(599_000);
  assert_eq!(run.play.phase(), Phase::Playing);
  run.advance(1_000);

  assert_eq!(run.eliminated(), None);
  assert_eq!(run.over(), (Ending::Idle, vec![("a".to_owned(), 20), ("b".to_owned(), 0)]));
}

#[test]
fn a_player_giving_up_leaves_the_game_and_stopping_ends_it() {
  let mut run = Run::new(settings(), &["a", "b", "c"]);

  assert!(run.play.give_up("b"));
  assert_eq!(run.current().as_deref(), Some("a"));
  run.play.stop();
  run.pump();
  assert_eq!(run.over().0, Ending::Stopped);
}

#[test]
fn a_game_opens_on_the_word_a_player_gave_and_the_player_after_them_moves_first() {
  let run = Run::new(
    GameSettings {
      opening: Some(Opening {
        word: W.follows.clone(),
        by: Some("a".to_owned()),
      }),
      ..settings()
    },
    &["a", "b", "c"],
  );

  assert!(run.events.iter().any(|event| matches!(
    event,
    GameEvent::Round { word, openings, first: true } if *word == W.follows && *openings == W.follows_hands
  )));
  assert_eq!(
    run.play.players().iter().map(|player| player.id.as_str()).collect::<Vec<_>>(),
    ["b", "c", "a"]
  );
  assert_eq!(run.current().as_deref(), Some("b"));
}

#[test]
fn the_robot_sits_last_and_answers_from_the_book_after_its_pause() {
  let mut run = Run::new(
    GameSettings {
      robot: robot(Difficulty::Normal),
      ..settings()
    },
    &["a"],
  );

  assert_eq!(
    run.play.players().iter().map(|player| player.id.as_str()).collect::<Vec<_>>(),
    ["a", ROBOT]
  );
  run.submit("a", &W.follows);
  assert_eq!(run.current().as_deref(), Some(ROBOT));
  run.advance(1_000);

  let (by, word) = run.accepted().last().cloned().expect("the robot's word");

  assert_eq!(by, ROBOT);
  assert!(W.after_follows.contains(&word));
}

#[test]
fn the_robot_goes_out_when_the_manner_rule_leaves_it_nothing_to_play() {
  let mut run = Run::new(
    GameSettings {
      rules: ChainRules {
        loanwords: false,
        manner: true,
      },
      robot: robot(Difficulty::Hard),
      ..settings()
    },
    &["a", "b"],
  );

  run.submit("a", &W.follows);
  run.submit("b", &W.after_follows[1]);
  // The word just played is answered only by one word, which nothing follows.
  run.advance(1_000);

  assert_eq!(run.eliminated(), Some((ROBOT.to_owned(), Elimination::Stuck, None)));
  assert_eq!(
    run.play.players().iter().map(|player| player.out).collect::<Vec<_>>(),
    [None, None, Some(Elimination::Stuck)]
  );
}

#[test]
fn a_game_nobody_joined_ends_as_soon_as_it_would_start() {
  let run = Run::new(
    GameSettings {
      robot: robot(Difficulty::Easy),
      ..settings()
    },
    &[],
  );

  assert_eq!(run.over().0, Ending::Nobody);
}

#[test]
fn one_player_alone_plays_on_until_their_time_runs_down() {
  let mut run = Run::new(settings(), &["a"]);

  run.submit("a", &W.follows);
  assert_eq!(run.current().as_deref(), Some("a"));
  run.advance(30_000);
  assert_eq!(
    run.over(),
    (Ending::Decided, vec![("a".to_owned(), 2 * WORD_CHAIN.points.per_syllable)])
  );
}
