//! The brain service (docs/design.md and docs/boundary-api.md): an agent's long-term memory as a service. It
//! keeps many brains, each an account's and one agent's, as subjects, layers and versioned memories, hands a
//! turn its recall block, renders a persona and moves a brain in and out as a brain file, and holds nothing
//! outside a brain. Its harness plugin, the MCP server `bmo-brain mcp`, calls the service through
//! `bmo-brain-client`.

pub mod api;
pub mod base;
pub mod chain;
pub mod clock;
pub mod config;
pub mod mcp;
pub mod openapi;
pub mod store;
pub mod tool_env;
