//! The brain service (docs/boundary-api.md): memory rows, notes, watches, reminders, fragments and the
//! libraries they come from, knowledge reads, locks and scopes, stored and served under the caller's opaque
//! scopes. It never executes and calls no other service; its clock turns moments and conditions into events.
//! Its harness plugin, the MCP server `bmo-brain mcp`, calls the service through `bmo-brain-client`.

pub mod api;
pub mod clock;
pub mod command;
pub mod config;
pub mod mcp;
pub mod openapi;
pub mod store;
