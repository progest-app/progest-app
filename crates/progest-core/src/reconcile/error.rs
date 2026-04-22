//! Error type returned by the reconcile module.
//!
//! Each variant wraps the underlying error from the layer it came from so the
//! source can be recovered with [`std::error::Error::source`]. We avoid
//! flattening causes into strings so that `progest doctor` can inspect
//! specific failures (e.g. "index write failed" vs "meta write failed") and
//! surface actionable diagnostics.

use thiserror::Error;

use crate::fs::{FsError, IgnoreError, ScanError};
use crate::identity::FingerprintError;
use crate::index::IndexError;
use crate::meta::MetaStoreError;

/// Aggregate error surface for reconcile operations.
#[derive(Debug, Error)]
pub enum ReconcileError {
    /// Error raised by the filesystem layer (read, metadata, atomic write, …).
    #[error(transparent)]
    Fs(#[from] FsError),

    /// Error raised while loading the project's ignore rules.
    #[error(transparent)]
    Ignore(#[from] IgnoreError),

    /// Error raised while walking the project tree.
    #[error(transparent)]
    Scan(#[from] ScanError),

    /// Error raised while reading or writing a sidecar `.meta`.
    #[error(transparent)]
    Meta(#[from] MetaStoreError),

    /// Error raised while reading or writing the `SQLite` index.
    #[error(transparent)]
    Index(#[from] IndexError),

    /// Error raised while hashing a file to derive its fingerprint.
    #[error(transparent)]
    Fingerprint(#[from] FingerprintError),
}
