//! Typed row representations exchanged across the [`super::Index`] boundary.
//!
//! These are domain values, not thin SQL row wrappers: reconcile and doctor
//! work with [`FileRow`] directly, and the translation to `rusqlite::Row` is
//! confined to [`super::store`].

use crate::fs::ProjectPath;
use crate::identity::{FileId, Fingerprint};
use crate::meta::{Kind, Status};

/// Row for the `files` table, keyed by [`FileId`] with a unique `path`.
///
/// Nullable SQL columns (`size`, `mtime`, `created_at`, `last_seen_at`)
/// map to [`Option`] so that callers who don't have the information yet
/// can upsert without inventing placeholder values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub file_id: FileId,
    pub path: ProjectPath,
    pub fingerprint: Fingerprint,
    pub source_file_id: Option<FileId>,
    pub kind: Kind,
    pub status: Status,
    pub size: Option<u64>,
    /// File modification time as a Unix timestamp in seconds.
    pub mtime: Option<i64>,
    /// Creation time in an ISO-8601 / RFC-3339 string, matching the
    /// `.meta` `created_at` serialization.
    pub created_at: Option<String>,
    /// Last time reconcile observed this row on disk.
    pub last_seen_at: Option<String>,
}
