//! The brain service: each chat agent's memory rows, its brain documents (knowledge, the person book and
//! room rules) and the recollector runs that write them. Other units reach it over HTTP only.

pub mod api;
pub mod commands;
pub mod config;
pub mod editors;
pub mod failure;
pub mod migration;
pub mod policy;
pub mod ports;
pub mod room_rules;
pub mod schedules;
pub mod settings;
pub mod watch_runner;
pub mod watches;

pub use failure::Failure;
