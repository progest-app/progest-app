//! Saved-views CRUD (`docs/SEARCH_DSL.md` §11).
//!
//! `views.toml` lives at the project root and is git-shared. Each
//! entry has a stable `id`, a human `name`, and a DSL `query` string.
//! v1 also accepts the `description` and `group_by` fields; `sort`
//! is reserved for v1.x and is silently kept as raw text.
//!
//! The loader / saver round-trips through `toml`. View order is
//! preserved on disk (`Vec<View>`); the saver emits entries in their
//! current order.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fs::{FileSystem, FsError, ProjectPath};

use super::parse::{ParseError, parse};

/// `views.toml` schema version. Forward-compat handling matches
/// `core::rules` / `core::accepts` (unknown keys → warning, not
/// fatal — but the loader currently surfaces only structural errors).
pub const VIEWS_SCHEMA_VERSION: u32 = 1;

/// One saved view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub id: String,
    pub name: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// Reserved for v1.x. Loaded as raw text, not interpreted yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

impl View {
    /// Validate the view itself: id shape, query parses.
    pub fn validate(&self) -> Result<(), ViewError> {
        if self.id.is_empty() {
            return Err(ViewError::InvalidId {
                id: self.id.clone(),
            });
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(ViewError::InvalidId {
                id: self.id.clone(),
            });
        }
        if self.id.len() > 64 {
            return Err(ViewError::InvalidId {
                id: self.id.clone(),
            });
        }
        // Query must parse successfully (validate against schema is
        // the caller's job — different schemas → different keys).
        parse(&self.query).map_err(|e| ViewError::InvalidQuery {
            id: self.id.clone(),
            error: e,
        })?;
        Ok(())
    }
}

/// Top-level `views.toml` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewsDocument {
    pub schema_version: u32,
    #[serde(default, rename = "views")]
    pub views: Vec<View>,
}

impl Default for ViewsDocument {
    fn default() -> Self {
        Self {
            schema_version: VIEWS_SCHEMA_VERSION,
            views: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ViewError {
    #[error("views.toml not present")]
    NotFound,
    #[error("read/write views.toml: {0}")]
    Fs(#[from] FsError),
    #[error("parse views.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("serialize views.toml: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("views.toml is not valid UTF-8")]
    Encoding,
    #[error("unsupported views.toml schema_version {found}; this build understands {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("duplicate view id {id:?}")]
    DuplicateId { id: String },
    #[error("invalid view id {id:?} (must match [a-z0-9_-]{{1,64}})")]
    InvalidId { id: String },
    #[error("invalid query in view {id:?}: {error}")]
    InvalidQuery { id: String, error: ParseError },
    #[error("view {id:?} not found")]
    UnknownId { id: String },
}

/// Load `views.toml` from `path`. Missing file is treated as the
/// `NotFound` variant so callers can choose to default to empty.
pub fn load(fs: &dyn FileSystem, path: &ProjectPath) -> Result<ViewsDocument, ViewError> {
    let bytes = match fs.read(path) {
        Ok(b) => b,
        Err(FsError::NotFound(_)) => return Err(ViewError::NotFound),
        Err(e) => return Err(ViewError::Fs(e)),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| ViewError::Encoding)?;
    let doc: ViewsDocument = toml::from_str(text)?;
    if doc.schema_version != VIEWS_SCHEMA_VERSION {
        return Err(ViewError::UnsupportedSchema {
            found: doc.schema_version,
            expected: VIEWS_SCHEMA_VERSION,
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    for v in &doc.views {
        if !seen.insert(&v.id) {
            return Err(ViewError::DuplicateId { id: v.id.clone() });
        }
        v.validate()?;
    }
    Ok(doc)
}

/// Save `doc` to `path`, replacing any existing file. Validates
/// every view + uniqueness before writing so a save can never put
/// the file into a bad state.
pub fn save(fs: &dyn FileSystem, path: &ProjectPath, doc: &ViewsDocument) -> Result<(), ViewError> {
    let mut seen = std::collections::BTreeSet::new();
    for v in &doc.views {
        if !seen.insert(&v.id) {
            return Err(ViewError::DuplicateId { id: v.id.clone() });
        }
        v.validate()?;
    }
    let text = toml::to_string_pretty(doc)?;
    fs.write_atomic(path, text.as_bytes())?;
    Ok(())
}

/// Insert or replace a view by id. Returns the document after the
/// mutation; caller persists with [`save`].
pub fn upsert(doc: &mut ViewsDocument, view: View) -> Result<(), ViewError> {
    view.validate()?;
    if let Some(slot) = doc.views.iter_mut().find(|v| v.id == view.id) {
        *slot = view;
    } else {
        doc.views.push(view);
    }
    Ok(())
}

/// Remove the view with the given id. Returns an error if the id
/// doesn't exist (so the CLI can report a clear failure).
pub fn delete(doc: &mut ViewsDocument, id: &str) -> Result<(), ViewError> {
    let before = doc.views.len();
    doc.views.retain(|v| v.id != id);
    if doc.views.len() == before {
        return Err(ViewError::UnknownId { id: id.into() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::mem::MemFileSystem;

    fn view(id: &str, q: &str) -> View {
        View {
            id: id.into(),
            name: format!("View {id}"),
            query: q.into(),
            description: None,
            group_by: None,
            sort: None,
        }
    }

    #[test]
    fn round_trip_save_and_load() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("views.toml").unwrap();
        let mut doc = ViewsDocument::default();
        upsert(&mut doc, view("violations", "is:violation")).unwrap();
        upsert(
            &mut doc,
            view("psd-shots", r#"type:psd path:"./assets/shots/**""#),
        )
        .unwrap();
        save(&fs, &path, &doc).unwrap();

        let loaded = load(&fs, &path).unwrap();
        assert_eq!(loaded.schema_version, VIEWS_SCHEMA_VERSION);
        assert_eq!(loaded.views.len(), 2);
        assert_eq!(loaded.views[0].id, "violations");
        assert_eq!(loaded.views[1].id, "psd-shots");
    }

    #[test]
    fn missing_file_returns_not_found() {
        let fs = MemFileSystem::new();
        let err = load(&fs, &ProjectPath::new("missing.toml").unwrap()).unwrap_err();
        assert!(matches!(err, ViewError::NotFound));
    }

    #[test]
    fn duplicate_id_rejected_on_save() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("views.toml").unwrap();
        let doc = ViewsDocument {
            schema_version: VIEWS_SCHEMA_VERSION,
            views: vec![view("dup", "tag:a"), view("dup", "tag:b")],
        };
        let err = save(&fs, &path, &doc).unwrap_err();
        assert!(matches!(err, ViewError::DuplicateId { .. }));
    }

    #[test]
    fn invalid_id_rejected() {
        let v = view("Has Space", "tag:a");
        let err = v.validate().unwrap_err();
        assert!(matches!(err, ViewError::InvalidId { .. }));
    }

    #[test]
    fn invalid_query_rejected() {
        let v = view("bad", "--double");
        let err = v.validate().unwrap_err();
        assert!(matches!(err, ViewError::InvalidQuery { .. }));
    }

    #[test]
    fn upsert_replaces_by_id() {
        let mut doc = ViewsDocument::default();
        upsert(&mut doc, view("v1", "tag:a")).unwrap();
        upsert(&mut doc, view("v1", "tag:b")).unwrap();
        assert_eq!(doc.views.len(), 1);
        assert_eq!(doc.views[0].query, "tag:b");
    }

    #[test]
    fn delete_unknown_errors() {
        let mut doc = ViewsDocument::default();
        let err = delete(&mut doc, "missing").unwrap_err();
        assert!(matches!(err, ViewError::UnknownId { .. }));
    }

    #[test]
    fn schema_version_mismatch_rejected() {
        let fs = MemFileSystem::new();
        let path = ProjectPath::new("views.toml").unwrap();
        let body = "schema_version = 9\nviews = []\n";
        fs.write_atomic(&path, body.as_bytes()).unwrap();
        let err = load(&fs, &path).unwrap_err();
        assert!(matches!(err, ViewError::UnsupportedSchema { .. }));
    }
}
