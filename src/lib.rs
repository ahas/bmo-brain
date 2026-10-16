//! The brain service: each chat agent's memory rows, its brain documents (knowledge, the person book and
//! room rules) and the recollector runs that write them. Other units reach it over HTTP only.

pub mod policy;
