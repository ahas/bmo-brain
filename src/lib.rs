//! The brain service (docs/design.md and docs/boundary-api.md): an agent's long-term memory as a service. It
//! keeps many brains, each an account's and one agent's, as subjects, layers and versioned memories, hands a
//! turn its recall block, renders a persona and moves a brain in and out as a brain file. The rows held under a
//! caller's opaque scope (memory rows, notes, watches, reminders, fragments, libraries, locks and the caller's
//! own records) keep answering until their readers move. Its harness plugin, the MCP server `bmo-brain mcp`,
//! calls the service through `bmo-brain-client`.

pub mod api;
pub mod clock;
pub mod command;
pub mod config;
pub mod import;
pub mod mcp;
pub mod moving;
pub mod openapi;
pub mod store;
pub mod tool_env;
