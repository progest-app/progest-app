//! Error type shared by the index layer.
//!
//! Kept in its own module so that `store.rs` and `migration.rs` can both
//! surface the same [`IndexError`] without introducing a cycle when tag
//! operations land in a follow-up commit.

use thiserror::Error;

use crate::fs::ProjectPathError;
use crate::identity::{FileIdError, FingerprintError};

use super::migration::MigrationError;

/// Errors returned by [`super::Index`] operations and the `SqliteIndex` setup path.
#[derive(Debug, Error)]
pub enum IndexError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error("stored file_id is not a valid UUID: {0}")]
    InvalidFileId(#[from] FileIdError),
    #[error("stored fingerprint is invalid: {0}")]
    InvalidFingerprint(#[from] FingerprintError),
    #[error("stored path is not a valid project path: {0}")]
    InvalidPath(#[from] ProjectPathError),
    #[error("unknown kind in index: {0}")]
    InvalidKind(String),
    #[error("unknown status in index: {0}")]
    InvalidStatus(String),
}
