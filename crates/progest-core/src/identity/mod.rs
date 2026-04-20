//! File identity primitives.
//!
//! Every file Progest tracks gets a stable [`FileId`] (`UUIDv7`) and a content
//! fingerprint (blake3, 128-bit truncated — added in a follow-up). Together
//! they let the rest of core answer two questions independently:
//!
//! - "Is this the same file?" — by `FileId`
//! - "Is this the same *content*?" — by fingerprint
//!
//! The distinction is load-bearing: copying a file must yield a new `FileId`
//! (with `source_file_id` pointing back at the original), while rename/move
//! preserves the existing `FileId`.

pub mod file_id;
pub mod fingerprint;

pub use file_id::{FileId, FileIdError};
pub use fingerprint::{Fingerprint, FingerprintError, compute_fingerprint};
