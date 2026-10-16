//! The memory rows an agent holds (docs/memories.md): the rows it holds in a room or for none, the notes it
//! keeps for a room's digest and the watches a row sets, written and read on the brain's database. What a
//! row is read against that the brain does not hold comes through `people::Known`, and how a watch's check
//! is started through `watches::LaunchWatch`.

pub mod events;
pub mod memories;
pub mod notes;
pub mod people;
pub mod watches;

pub use brain_support::database::Fault;
