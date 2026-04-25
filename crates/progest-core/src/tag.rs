//! Tag CRUD on top of [`crate::index::Index`].
//!
//! Tags are stored on the index in the `tags(file_id, tag)` table.
//! Reconcile / rename keep the (`file_id`, tag) pairs alive across
//! file moves; this module gives the CLI and Tauri IPC a single
//! seam they can call without each owning their own SQL.
//!
//! Tag names follow `^[a-zA-Z0-9_-]+$` — the same shape the search
//! DSL accepts (`docs/SEARCH_DSL.md` §4.2). Validation lives here so
//! that callers receive a typed error rather than a generic SQL
//! constraint failure.

use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;

use crate::identity::FileId;
use crate::index::{Index, IndexError};

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").expect("static regex compiles"));

/// Maximum length for a single tag name (graphemes / bytes — they
/// agree because the regex restricts to ASCII).
pub const MAX_TAG_LEN: usize = 64;

#[derive(Debug, Error)]
pub enum TagError {
    #[error("tag {tag:?} contains characters outside [a-zA-Z0-9_-]")]
    InvalidShape { tag: String },
    #[error("tag {tag:?} exceeds the maximum length of {MAX_TAG_LEN}")]
    TooLong { tag: String },
    #[error("tag must not be empty")]
    Empty,
    #[error("index error: {0}")]
    Index(#[from] IndexError),
}

/// Validate a tag name. Used by `add` and `remove` so callers don't
/// silently insert weird tags via raw SQL.
pub fn validate_tag(tag: &str) -> Result<(), TagError> {
    if tag.is_empty() {
        return Err(TagError::Empty);
    }
    if tag.len() > MAX_TAG_LEN {
        return Err(TagError::TooLong { tag: tag.into() });
    }
    if !TAG_RE.is_match(tag) {
        return Err(TagError::InvalidShape { tag: tag.into() });
    }
    Ok(())
}

/// Add `tag` to `file_id`. Idempotent (re-adding a tag is a no-op).
pub fn add(index: &dyn Index, file_id: &FileId, tag: &str) -> Result<(), TagError> {
    validate_tag(tag)?;
    index.tag_add(file_id, tag)?;
    Ok(())
}

/// Remove `tag` from `file_id`. Missing pair is a no-op.
pub fn remove(index: &dyn Index, file_id: &FileId, tag: &str) -> Result<(), TagError> {
    validate_tag(tag)?;
    index.tag_remove(file_id, tag)?;
    Ok(())
}

/// List the tags attached to `file_id`, sorted lexicographically.
pub fn list(index: &dyn Index, file_id: &FileId) -> Result<Vec<String>, TagError> {
    Ok(index.list_tags_for_file(file_id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::ProjectPath;
    use crate::identity::Fingerprint;
    use crate::index::{FileRow, SqliteIndex};
    use crate::meta::{Kind, Status};

    fn fresh_index_with_file() -> (SqliteIndex, FileId) {
        let index = SqliteIndex::open_in_memory().unwrap();
        let file_id = FileId::new_v7();
        let row = FileRow {
            file_id,
            path: ProjectPath::new("a.psd").unwrap(),
            fingerprint: Fingerprint::from_bytes([0u8; 16]),
            source_file_id: None,
            kind: Kind::Asset,
            status: Status::Active,
            size: None,
            mtime: None,
            created_at: None,
            last_seen_at: None,
        };
        index.upsert_file(&row).unwrap();
        (index, file_id)
    }

    #[test]
    fn add_then_list_returns_tag() {
        let (idx, fid) = fresh_index_with_file();
        add(&idx, &fid, "wip").unwrap();
        assert_eq!(list(&idx, &fid).unwrap(), vec!["wip".to_string()]);
    }

    #[test]
    fn add_is_idempotent() {
        let (idx, fid) = fresh_index_with_file();
        add(&idx, &fid, "wip").unwrap();
        add(&idx, &fid, "wip").unwrap();
        assert_eq!(list(&idx, &fid).unwrap(), vec!["wip".to_string()]);
    }

    #[test]
    fn remove_drops_tag() {
        let (idx, fid) = fresh_index_with_file();
        add(&idx, &fid, "wip").unwrap();
        remove(&idx, &fid, "wip").unwrap();
        assert!(list(&idx, &fid).unwrap().is_empty());
    }

    #[test]
    fn remove_missing_is_a_noop() {
        let (idx, fid) = fresh_index_with_file();
        remove(&idx, &fid, "wip").unwrap();
        assert!(list(&idx, &fid).unwrap().is_empty());
    }

    #[test]
    fn list_sorted_lexicographically() {
        let (idx, fid) = fresh_index_with_file();
        add(&idx, &fid, "review").unwrap();
        add(&idx, &fid, "approved").unwrap();
        add(&idx, &fid, "wip").unwrap();
        assert_eq!(
            list(&idx, &fid).unwrap(),
            vec![
                "approved".to_string(),
                "review".to_string(),
                "wip".to_string()
            ]
        );
    }

    #[test]
    fn empty_tag_rejected() {
        let (idx, fid) = fresh_index_with_file();
        let err = add(&idx, &fid, "").unwrap_err();
        assert!(matches!(err, TagError::Empty));
    }

    #[test]
    fn invalid_shape_rejected() {
        let (idx, fid) = fresh_index_with_file();
        let err = add(&idx, &fid, "with space").unwrap_err();
        assert!(matches!(err, TagError::InvalidShape { .. }));
    }

    #[test]
    fn too_long_rejected() {
        let (idx, fid) = fresh_index_with_file();
        let long = "x".repeat(MAX_TAG_LEN + 1);
        let err = add(&idx, &fid, &long).unwrap_err();
        assert!(matches!(err, TagError::TooLong { .. }));
    }
}
