//! The crate's one test harness (bmo-chat's docs/conventions.md, 「Rust builds」): each file of tests a
//! module of it, sharing the process.

mod support;

mod brains;
mod consolidation;
mod events;
mod history;
mod learning;
mod mcp;
mod misses;
mod openapi;
mod orgs;
mod recall;
mod repository;
mod screen;
mod sheets;
mod uses;
mod vectors;
