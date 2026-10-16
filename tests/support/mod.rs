//! What the service's tests share: the fixed ids a test names the owner, a room and a message by, the
//! agents' names, a migrated database of the test's own, the other units in memory, and a stub server a
//! port's client is tested against. A test needing a second one of a kind names its own beside these.

#![allow(dead_code)]

pub mod agency;
pub mod calls;
pub mod gate;
pub mod receiver;
pub mod scratch;
pub mod stub;

/// The owner's user id, which is also the chat id of the owner's private chat with the chat agent.
pub const OWNER: i64 = 42;
/// A group room's chat id.
pub const ROOM: i64 = -100;
/// A message's id within its room.
pub const MESSAGE: i64 = 9;
/// An agent's registration name.
pub const AGENT: &str = "agent-a";
/// A second agent's registration name.
pub const OTHER_AGENT: &str = "agent-b";
