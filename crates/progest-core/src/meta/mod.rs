//! Sidecar metadata (`.meta`) read/write.
//!
//! `.meta` files are the authoritative source of file identity, tags, notes,
//! and custom fields. The `SQLite` index is a rebuildable cache derived from
//! them; when the two disagree, `.meta` wins.
//!
//! The module is split into:
//!
//! - [`document`] — TOML schema types ([`MetaDocument`] and its sections).
//! - [`store`] — the [`MetaStore`] trait plus an implementation that layers
//!   on top of [`crate::fs::FileSystem`] for atomic on-disk writes.
//!
//! The `.dirmeta.toml` reader and the failed-write pending queue are
//! intentionally out of scope for this first slice and land as follow-up PRs.

pub mod dirmeta;
pub mod document;
pub mod pending;
pub mod store;

pub use dirmeta::{DIRMETA_FILENAME, DirmetaDocument, dirmeta_path, load_dirmeta, save_dirmeta};
pub use document::{
    CoreSection, Kind, MetaDocument, MetaError, NamingSection, NotesSection, SCHEMA_VERSION,
    Status, TagsSection,
};
pub use pending::{
    FlushReport, PENDING_DIR, PendingEntry, PendingError, PendingQueue, envelope_filename,
};
pub use store::{MetaStore, MetaStoreError, SIDECAR_SUFFIX, StdMetaStore, sidecar_path};
