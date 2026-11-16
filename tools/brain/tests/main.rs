//! The crate's one test harness (bmo-chat's docs/conventions.md, 「Rust builds」): each file of tests a
//! module of it, sharing the process.

// The service's own scratch databases, so this binary's are named, dropped and swept as the service's are; it
// takes the copies alone of what the module offers.
#[allow(dead_code)]
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

mod tool;
