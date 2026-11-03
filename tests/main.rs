//! The crate's one test harness (bmo-chat's docs/conventions.md, 「Rust builds」): each file of tests a
//! module of it, sharing the process.

mod support;

mod brains;
mod events;
mod learning;
mod mcp;
mod openapi;
mod repository;
mod screen;
mod sheets;
mod uses;
