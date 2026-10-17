//! The ports to the other units, one per owner (`docs/boundary-api.md`, 「Calls the brain makes」). Each
//! is a trait the brain's code is written against and an HTTP client that answers it; a test answers it
//! with a fake of its own.

pub mod agency;
pub mod gate;
pub mod http;
pub mod job;
pub mod receiver;
