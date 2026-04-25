//! [`ProjectRoot`] — the discovered location of a Progest project.
//!
//! Every subcommand that touches an existing project starts by calling
//! [`ProjectRoot::discover`]: it walks upward from a starting directory
//! looking for a `.progest/` subdirectory and returns a value that hands out
//! well-known paths inside it. The tree walk mirrors how `git` locates its
//! repo root, so the UX is familiar.

use std::path::{Path, PathBuf};

use super::document::ProjectError;
use super::layout::{
    DOT_DIR, HISTORY_DB_FILENAME, INDEX_DB_FILENAME, LOCAL_DIR, PROJECT_TOML_FILENAME,
    RULES_TOML_FILENAME, SCHEMA_TOML_FILENAME, USER_IGNORE_FILENAME, VIEWS_TOML_FILENAME,
};

/// Absolute path to a discovered project root.
///
/// The value is guaranteed to point at a directory that contains a
/// `.progest/` subdirectory at construction time. Callers should not hold
/// it across operations that might delete the project — reconcile is
/// expected to be run against a live tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    root: PathBuf,
}

impl ProjectRoot {
    /// Wrap `root` without running discovery. The caller is responsible for
    /// ensuring the path points at a directory that already contains a
    /// `.progest/` subdirectory.
    #[must_use]
    pub fn from_path(root: PathBuf) -> Self {
        Self { root }
    }

    /// Walk upward from `start` searching for a `.progest/` subdirectory.
    ///
    /// Returns [`ProjectError::NotFound`] when the walk reaches the
    /// filesystem root without finding one.
    pub fn discover(start: &Path) -> Result<Self, ProjectError> {
        let canonical_start = std::fs::canonicalize(start)?;
        let mut cursor: Option<&Path> = Some(&canonical_start);
        while let Some(dir) = cursor {
            if dir.join(DOT_DIR).is_dir() {
                return Ok(Self {
                    root: dir.to_path_buf(),
                });
            }
            cursor = dir.parent();
        }
        Err(ProjectError::NotFound {
            start: canonical_start,
        })
    }

    /// Absolute path of the project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Absolute path of the `.progest/` subdirectory.
    #[must_use]
    pub fn dot_dir(&self) -> PathBuf {
        self.root.join(DOT_DIR)
    }

    /// Absolute path of `.progest/project.toml`.
    #[must_use]
    pub fn project_toml(&self) -> PathBuf {
        self.dot_dir().join(PROJECT_TOML_FILENAME)
    }

    /// Absolute path of `.progest/ignore`.
    #[must_use]
    pub fn user_ignore(&self) -> PathBuf {
        self.dot_dir().join(USER_IGNORE_FILENAME)
    }

    /// Absolute path of `.progest/index.db`.
    #[must_use]
    pub fn index_db(&self) -> PathBuf {
        self.dot_dir().join(INDEX_DB_FILENAME)
    }

    /// Absolute path of `.progest/local/history.db`.
    #[must_use]
    pub fn history_db(&self) -> PathBuf {
        self.dot_dir().join(LOCAL_DIR).join(HISTORY_DB_FILENAME)
    }

    /// Absolute path of `.progest/rules.toml`.
    ///
    /// The file is optional — callers that don't find it on disk must
    /// fall back to an empty ruleset, not error.
    #[must_use]
    pub fn rules_toml(&self) -> PathBuf {
        self.dot_dir().join(RULES_TOML_FILENAME)
    }

    /// Absolute path of `.progest/schema.toml`.
    ///
    /// Optional: absent means "use the builtin alias catalog and no
    /// project-defined `[extension_compounds]`".
    #[must_use]
    pub fn schema_toml(&self) -> PathBuf {
        self.dot_dir().join(SCHEMA_TOML_FILENAME)
    }

    /// Absolute path of `.progest/views.toml`.
    ///
    /// Optional: absent means "no shared saved views — CLI/UI uses
    /// ad-hoc queries only".
    #[must_use]
    pub fn views_toml(&self) -> PathBuf {
        self.dot_dir().join(VIEWS_TOML_FILENAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_project(tmp: &Path, marker_name: &str) -> PathBuf {
        let root = tmp.join(marker_name);
        std::fs::create_dir_all(root.join(DOT_DIR)).unwrap();
        root
    }

    #[test]
    fn discover_finds_project_at_the_starting_path() {
        let tmp = TempDir::new().unwrap();
        let root = make_project(tmp.path(), "demo");
        let found = ProjectRoot::discover(&root).unwrap();
        // Canonicalized paths may differ on macOS (/var vs /private/var), so
        // compare via canonicalize on both sides.
        let expected = std::fs::canonicalize(&root).unwrap();
        assert_eq!(found.root(), expected);
    }

    #[test]
    fn discover_walks_up_from_a_nested_directory() {
        let tmp = TempDir::new().unwrap();
        let root = make_project(tmp.path(), "demo");
        let deep = root.join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();
        let found = ProjectRoot::discover(&deep).unwrap();
        let expected = std::fs::canonicalize(&root).unwrap();
        assert_eq!(found.root(), expected);
    }

    #[test]
    fn discover_errors_when_no_project_exists() {
        let tmp = TempDir::new().unwrap();
        // Create an unrelated directory with no .progest/ anywhere in the
        // chain (TempDir is created under /var/folders/.../Tmp which never
        // has .progest/).
        let empty = tmp.path().join("nowhere");
        std::fs::create_dir_all(&empty).unwrap();
        let err = ProjectRoot::discover(&empty).unwrap_err();
        assert!(matches!(err, ProjectError::NotFound { .. }));
    }
}
