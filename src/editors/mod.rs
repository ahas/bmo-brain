//! The brain's editors: what the recollectors read and leave behind, and the documents they write.

pub mod archive;
pub mod book;
pub mod commits;
pub mod desk;
pub mod documents;
pub mod editor;
pub mod lock;
pub mod people;
pub mod prompts;
pub mod recollection;
pub mod runs;
pub mod settle;
pub mod sheets;

/// A folder's documents as they stood, by path relative to it.
pub type DocumentBackup = std::collections::BTreeMap<String, Vec<u8>>;
