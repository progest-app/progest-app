//! Shared scaffolding for `progest-core` integration tests.
//!
//! Cargo treats every file under `tests/` as its own crate, so this
//! module is pulled in via `mod support;` from each integration test
//! that wants to share helpers.
//!
//! Items are flagged `#[allow(dead_code)]` because not every test
//! pulls in every helper — the lints fire per-test crate, and a
//! helper that's used by only some test files would otherwise warn.

#![allow(dead_code)]

use progest_core::fs::ProjectPath;
use progest_core::identity::{FileId, Fingerprint};
use progest_core::meta::MetaDocument;

/// Construct a `ProjectPath` from a literal in tests, panicking on
/// the (test-author) error of supplying a non-project-relative path.
pub fn p(rel: &str) -> ProjectPath {
    ProjectPath::new(rel).expect("test path literal must be a valid ProjectPath")
}

/// A canonical, parseable fingerprint literal for tests that don't
/// care about the actual hash value — only that the field is set.
pub fn sample_fingerprint() -> Fingerprint {
    "blake3:00112233445566778899aabbccddeeff"
        .parse()
        .expect("literal fingerprint parses")
}

/// A fresh `MetaDocument` with a new v7 `FileId` and the canonical
/// `sample_fingerprint`. For tests that go on to set tags, notes,
/// or custom fields on top.
pub fn sample_doc() -> MetaDocument {
    MetaDocument::new(FileId::new_v7(), sample_fingerprint())
}
