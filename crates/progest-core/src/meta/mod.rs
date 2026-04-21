//! Sidecar metadata (`.meta`) read/write.
//!
//! `.meta` files are the authoritative source of file identity, tags, notes,
//! and custom fields. The `SQLite` index is a rebuildable cache derived from
//! them; when the two disagree, `.meta` wins.
//!
//! The `.dirmeta.toml` reader and the failed-write pending queue are
//! intentionally out of scope for this first slice and land as follow-up PRs.

pub mod document;

pub use document::{
    CoreSection, Kind, MetaDocument, MetaError, NamingSection, NotesSection, SCHEMA_VERSION,
    Status, TagsSection,
};
