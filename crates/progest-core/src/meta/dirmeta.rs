//! On-disk schema and I/O for `.dirmeta.toml`.
//!
//! Every directory inside a Progest project can carry its own
//! `.dirmeta.toml`. For now the loader is intentionally narrow:
//!
//! - It provides typed access to the sections `.meta` and `.dirmeta.toml`
//!   share (`tags`, `notes`, `custom`, `meta_internal`).
//! - It preserves every other top-level key verbatim through a
//!   [`toml::Table`] `extra` field, so the not-yet-landed
//!   `[accepts]` section (M2 `core::accepts`) round-trips untouched even
//!   when the current build does not know its schema.
//!
//! Consumers that need typed access to `[accepts]` will layer on top in
//! `core::accepts`: they read `document.extra.get("accepts")`, parse with
//! their own struct, and write back through the same path.
//!
//! Like `.meta`, the `schema_version` is validated on load so a forward-
//! incompatible rev from a newer Progest never silently drops fields.

use serde::{Deserialize, Serialize};
use toml::Table;

use crate::fs::{FileSystem, FsError, ProjectPath};

use super::document::{MetaError, NotesSection, SCHEMA_VERSION, TagsSection};
use super::store::MetaStoreError;

/// Filename used for per-directory metadata.
pub const DIRMETA_FILENAME: &str = ".dirmeta.toml";

/// Parsed representation of a `.dirmeta.toml` file.
///
/// The document intentionally carries a *subset* of [`super::MetaDocument`]:
/// directories are not files, so `file_id`, `content_fingerprint`, and
/// `source_file_id` don't apply. `tags`, `notes`, `custom`, and
/// `meta_internal` mirror the file-level schema so the same UI code paths
/// can read both.
///
/// Unknown top-level keys (including `[accepts]` until `core::accepts`
/// lands) are preserved in [`DirmetaDocument::extra`] to guarantee
/// lossless round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirmetaDocument {
    /// Schema version written to disk. Validated against [`SCHEMA_VERSION`] on load.
    pub schema_version: u32,

    /// `[tags]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<TagsSection>,

    /// `[notes]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<NotesSection>,

    /// `[custom]` section — free-form user-defined fields.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub custom: Table,

    /// `[meta_internal]` section — local-only state, excluded from git merges.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub meta_internal: Table,

    /// Unknown top-level keys preserved verbatim for forward compatibility.
    ///
    /// Most notably `[accepts]` lives here until `core::accepts` introduces
    /// a typed schema for it.
    #[serde(flatten)]
    pub extra: Table,
}

impl DirmetaDocument {
    /// Empty document at the current schema version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            tags: None,
            notes: None,
            custom: Table::new(),
            meta_internal: Table::new(),
            extra: Table::new(),
        }
    }

    /// Parse from TOML text. Rejects documents whose `schema_version`
    /// differs from the current build's [`SCHEMA_VERSION`].
    pub fn from_toml_str(input: &str) -> Result<Self, MetaError> {
        let doc: Self = toml::from_str(input)?;
        if doc.schema_version != SCHEMA_VERSION {
            return Err(MetaError::UnsupportedVersion {
                found: doc.schema_version,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(doc)
    }

    /// Serialize to TOML.
    pub fn to_toml_string(&self) -> Result<String, MetaError> {
        Ok(toml::to_string(self)?)
    }
}

impl Default for DirmetaDocument {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive the project-relative path to a directory's `.dirmeta.toml`.
///
/// For the project root the dirmeta sits at `/.dirmeta.toml`; for a
/// subdirectory at `<dir>/.dirmeta.toml`.
pub fn dirmeta_path(dir: &ProjectPath) -> Result<ProjectPath, MetaStoreError> {
    if dir.is_root() {
        Ok(ProjectPath::new(DIRMETA_FILENAME)?)
    } else {
        Ok(ProjectPath::new(format!(
            "{}/{DIRMETA_FILENAME}",
            dir.as_str()
        ))?)
    }
}

/// Load the dirmeta document for `dir`, if one exists. Returns `Ok(None)`
/// when the directory has no `.dirmeta.toml`.
pub fn load_dirmeta<F: FileSystem>(
    fs: &F,
    dir: &ProjectPath,
) -> Result<Option<DirmetaDocument>, MetaStoreError> {
    let path = dirmeta_path(dir)?;
    if !fs.exists(&path) {
        return Ok(None);
    }
    let bytes = match fs.read(&path) {
        Ok(b) => b,
        Err(FsError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let text =
        String::from_utf8(bytes).map_err(|_| MetaStoreError::InvalidUtf8(path.to_string()))?;
    Ok(Some(DirmetaDocument::from_toml_str(&text)?))
}

/// Write `doc` atomically as the dirmeta for `dir`.
pub fn save_dirmeta<F: FileSystem>(
    fs: &F,
    dir: &ProjectPath,
    doc: &DirmetaDocument,
) -> Result<(), MetaStoreError> {
    let path = dirmeta_path(dir)?;
    let text = doc.to_toml_string()?;
    fs.write_atomic(&path, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFileSystem;

    #[test]
    fn dirmeta_path_at_root_drops_parent() {
        let path = dirmeta_path(&ProjectPath::root()).unwrap();
        assert_eq!(path.as_str(), DIRMETA_FILENAME);
    }

    #[test]
    fn dirmeta_path_for_subdir_appends_filename() {
        let path = dirmeta_path(&ProjectPath::new("assets").unwrap()).unwrap();
        assert_eq!(path.as_str(), "assets/.dirmeta.toml");
    }

    #[test]
    fn default_document_has_current_schema_version() {
        let doc = DirmetaDocument::new();
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
        assert!(doc.tags.is_none());
        assert!(doc.custom.is_empty());
        assert!(doc.extra.is_empty());
    }

    #[test]
    fn accepts_section_round_trips_via_extra_table() {
        // `core::accepts` will land the typed schema later; until then the
        // flatten'd `extra` table is the contract that keeps teammates on
        // newer Progest versions from losing data on save.
        let raw = r#"
schema_version = 1

[accepts]
inherit = false
exts = [".psd", ".tif", ":image"]
mode = "warn"
"#;
        let doc = DirmetaDocument::from_toml_str(raw).unwrap();
        let rendered = doc.to_toml_string().unwrap();
        assert!(rendered.contains("[accepts]"));
        assert!(rendered.contains(".psd"));
        assert!(rendered.contains(":image"));
        assert!(rendered.contains("inherit"));
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let raw = "schema_version = 42\n";
        let err = DirmetaDocument::from_toml_str(raw).unwrap_err();
        assert!(matches!(
            err,
            MetaError::UnsupportedVersion {
                found: 42,
                expected: SCHEMA_VERSION
            }
        ));
    }

    #[test]
    fn load_returns_none_when_dirmeta_is_absent() {
        let fs = MemFileSystem::new();
        let result = load_dirmeta(&fs, &ProjectPath::new("assets").unwrap()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_then_load_round_trips_for_a_subdirectory() {
        let fs = MemFileSystem::new();
        let dir = ProjectPath::new("assets").unwrap();
        let mut doc = DirmetaDocument::new();
        doc.custom.insert("owner".into(), "art-team".into());

        save_dirmeta(&fs, &dir, &doc).unwrap();
        let loaded = load_dirmeta(&fs, &dir).unwrap().unwrap();
        assert_eq!(loaded, doc);
    }
}
