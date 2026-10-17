//! The editors' fragment desk (docs/fragments.md): the desk file a batch writes its rows on, the library trait
//! whose members its caller hands in and the agents' folders they stand in, a row's metadata and index line, and
//! the rows the brain's table already holds. The fragment tool writes on it for an editor's run, and the
//! service reads and settles it around the batch.

pub mod desk;
pub mod libraries;
pub mod meta;
pub mod ops;
pub mod policy;
pub mod rows;
