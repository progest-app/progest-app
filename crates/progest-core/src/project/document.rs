//! Serde schema for `.progest/project.toml`.
//!
//! The document is intentionally tiny: project identity, display name, and
//! the Progest version that wrote the file. Everything else that could live
//! here — team conventions, per-project settings — is either in `rules.toml`
//! or the sidecars, and keeping this file lean avoids upgrade churn.

use std::fmt;
use std::io;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml::Table;
use uuid::Uuid;

/// Stable per-project identifier. `UUIDv7` so that projects sort by
/// creation time without an extra timestamp field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectId(Uuid);

impl ProjectId {
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ProjectId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

impl From<ProjectId> for String {
    fn from(id: ProjectId) -> String {
        id.to_string()
    }
}

impl TryFrom<String> for ProjectId {
    type Error = uuid::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Parsed representation of `.progest/project.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectDocument {
    /// Stable project identifier.
    pub id: ProjectId,

    /// Human-readable project name (defaults to the root directory basename).
    pub name: String,

    /// Progest version that wrote this document — useful for future schema
    /// migrations and for the About dialog.
    pub progest_version: String,

    /// Unknown top-level keys preserved verbatim so that a newer Progest
    /// release can add fields without an older installation stripping them.
    #[serde(flatten, default)]
    pub extra: Table,
}

impl ProjectDocument {
    /// Build a document for a brand-new project.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self {
            id: ProjectId::new_v7(),
            name,
            progest_version: crate::VERSION.to_string(),
            extra: Table::new(),
        }
    }

    /// Serialize to TOML.
    pub fn to_toml_string(&self) -> Result<String, ProjectError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Parse from TOML text.
    pub fn from_toml_str(text: &str) -> Result<Self, ProjectError> {
        Ok(toml::from_str(text)?)
    }
}

/// Errors surfaced by project-level operations.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// No `.progest/` directory was found by walking up from the starting
    /// path. Commands that operate on an existing project surface this so
    /// users know they need to run `progest init` first.
    #[error("no Progest project found at or above `{start}`")]
    NotFound { start: std::path::PathBuf },

    /// `progest init` was asked to create a project but one already exists
    /// at the target. The caller decides whether to error or prompt.
    #[error("Progest project already initialized at `{root}`")]
    AlreadyInitialized { root: std::path::PathBuf },

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("failed to serialize project.toml: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("failed to parse project.toml: {0}")]
    TomlDe(#[from] toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_populates_id_and_version() {
        let doc = ProjectDocument::new("Demo".into());
        assert_eq!(doc.name, "Demo");
        assert!(!doc.progest_version.is_empty());
        // UUIDv7 values have the version nibble set to 7 in the 7th byte.
        assert_eq!(doc.id.as_uuid().get_version_num(), 7);
    }

    #[test]
    fn toml_round_trip_preserves_unknown_top_level_keys() {
        let raw = r#"
id = "0190f3d7-5dbc-7abc-8000-0123456789ab"
name = "Demo"
progest_version = "0.1.0"
custom_field = "hello"
"#;
        let doc = ProjectDocument::from_toml_str(raw).unwrap();
        let rendered = doc.to_toml_string().unwrap();
        assert!(rendered.contains(r#"custom_field = "hello""#));
    }
}
