//! The Korean word chain (kkeutmal-itgi) as a game (docs/word-chain.md, 「Where it stands in the code」): the
//! Hangul arithmetic and the particle a name takes by it, the word book a game judges by and its robot
//! plays from, and the game itself — players, turns, judging, rounds, scores and the robot. It names no
//! room, road or agent: whoever hosts a game says how each event reads, and keeps its time.

pub mod book;
pub mod game;
pub mod hangul;
pub mod particles;
pub mod policy;

pub use book::{ChainRules, SqliteWordBook, WordBook, WordEntry, WordMeaning, openings_after};
pub use game::{
  Alarm, Difficulty, Elimination, Ending, GameEvent, GameSettings, Opening, Phase, Player, Rejection,
  RobotSeat, Said, WordChainGame,
};
pub use hangul::is_hangul_word;
