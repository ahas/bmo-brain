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
pub mod holder;
pub mod mcp;
pub mod openapi;
pub mod store;
pub mod tool_env;

/// The version the service's health and its MCP server answer: the set's version the build was made at,
/// which the testbed hands the build as `BMO_VERSION`, else the manifest's placeholder (bmo-chat's
/// `docs/conventions.md`, 「Versions」).
pub const VERSION: &str = match option_env!("BMO_VERSION") {
  Some(version) => version,
  None => env!("CARGO_PKG_VERSION"),
};
