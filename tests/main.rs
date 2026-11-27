//! The crate's one test harness (bmo-chat's docs/conventions.md, 「Rust builds」): each file of tests a
//! module of it, sharing the process.

mod support;

mod base;
mod baseline;
mod brains;
mod consolidation;
mod events;
mod held;
mod history;
mod holder_command;
mod learning;
mod mcp;
mod misses;
mod openapi;
mod orgs;
mod recall;
mod removals;
mod repository;
mod screen;
mod sheets;
mod sleep;
mod uses;
mod vectors;
