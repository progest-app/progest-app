//! Append-only operation log with undo/redo stack (`core::history`).
//!
//! The store is the record of completed operations, not the driver —
//! callers at the rename / tag / meta-edit / import layers are
//! responsible for applying the change to disk; [`Store::append`]
//! lands *after* that work succeeds. History itself never touches
//! `.meta`, the filesystem, or `core::index`.
//!
//! Backed by `SQLite` at `.progest/local/history.db`. The two-table
//! schema (see `migrations/0001_initial.sql`) is:
//!
//! - `entries(id, ts, op_kind, payload_json, inverse_json, consumed, group_id)`
//!   — one row per recorded operation.
//! - `meta(key, value)` — `pointer` → id of the most recently
//!   applied, not-yet-undone entry.
//!
//! Retention is a hard 50 (REQUIREMENTS §3.4). Callers that need
//! older history should rely on VCS instead.
//!
//! See [`store::Store`] for the consumer-facing trait and
//! [`types::Operation`] for the op vocabulary. Inverse derivation
//! is pure and lives in [`inverse::invert`].

pub mod error;
pub mod inverse;
pub mod migration;
pub mod store;
pub mod types;

pub use error::HistoryError;
pub use inverse::invert;
pub use migration::{MIGRATIONS, Migration, MigrationError, apply, current_version};
pub use store::{RETENTION_LIMIT, SqliteStore, Store};
pub use types::{AppendRequest, Entry, EntryId, OpKind, Operation};
