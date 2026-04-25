//! `SQLite`-backed index over the project's tracked files.
//!
//! The index is a **rebuildable cache** of the reconciled view: a crash, a
//! schema bump, or an out-of-date database can always be thrown away and
//! rebuilt from the authoritative sources (`.meta` sidecars plus a
//! filesystem scan). Nothing here stores information that isn't derivable
//! from disk — see `docs/IMPLEMENTATION_PLAN.md` §3 for the division of
//! responsibilities between index and meta.
//!
//! The module is split into:
//!
//! - [`migration`] — schema versioning and the embedded migration runner.
//! - [`row`] — typed row domain values exchanged at the [`Index`] boundary.
//! - [`store`] — the [`Index`] trait and [`SqliteIndex`] implementation.
//!
//! Tag operations land in a follow-up commit on top of this foundation.

pub mod error;
pub mod migration;
pub mod row;
pub mod store;

pub use error::IndexError;
pub use migration::{MIGRATIONS, Migration, MigrationError, apply, current_version};
pub use row::FileRow;
pub use store::{
    CustomFieldEntry, CustomFieldValue, Index, RichRow, SearchProjection, SqliteIndex,
    ViolationCounts, ViolationRecord,
};
