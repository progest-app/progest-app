//! Ignore rules shared between the scanner and reconcile.
//!
//! Progest ships with a built-in set of patterns (VCS metadata, OS clutter,
//! DCC autosave files, and the project's own `.progest/` directory) that are
//! always hidden from the scanner. On top of that, a user-editable file at
//! `.progest/ignore` lets each project extend the list with gitignore-style
//! rules.
//!
//! This module exposes [`IgnoreRules`], a single matcher built from the
//! merged rule set. Callers only need to ask `is_ignored` — they do not need
//! to know where the rules came from.

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use thiserror::Error;

use super::{FileSystem, FsError, ProjectPath};

/// Patterns that are always ignored, independent of any user configuration.
///
/// The list is drawn from docs/REQUIREMENTS.md §7.1 and extended with
/// `.progest/`, which stores Progest's own state and must never appear in
/// the scanner output.
pub const DEFAULT_PATTERNS: &[&str] = &[
    ".git/",
    ".svn/",
    ".hg/",
    "node_modules/",
    "__pycache__/",
    "venv/",
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    "*.tmp",
    "*.bak",
    "*.swp",
    "*~",
    "*.blend1",
    "*.psd~",
    ".autosave/",
    ".progest/",
];

/// Location of the user-editable ignore file, relative to the project root.
pub const USER_IGNORE_PATH: &str = ".progest/ignore";

/// Errors that may arise while constructing [`IgnoreRules`].
#[derive(Debug, Error)]
pub enum IgnoreError {
    #[error("invalid ignore pattern `{line}`: {source}")]
    Parse {
        line: String,
        #[source]
        source: ignore::Error,
    },
    #[error("failed to compile ignore matcher: {0}")]
    Build(#[source] ignore::Error),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("ignore file is not valid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Compiled ignore matcher combining built-in defaults with the project's
/// optional `.progest/ignore` file.
pub struct IgnoreRules {
    matcher: Gitignore,
}

impl IgnoreRules {
    /// Load the ignore rules for the project backed by `fs`.
    ///
    /// Defaults are always applied; the user file is optional and silently
    /// skipped when absent. Comment lines (`#…`) and blank lines inside the
    /// user file are ignored.
    ///
    /// # Panics
    /// Panics only if the compiled-in [`USER_IGNORE_PATH`] constant fails
    /// [`ProjectPath`] validation, which is a programmer error.
    pub fn load(fs: &dyn FileSystem) -> Result<Self, IgnoreError> {
        let mut builder = GitignoreBuilder::new(fs.root());

        for pattern in DEFAULT_PATTERNS {
            builder
                .add_line(None, pattern)
                .map_err(|source| IgnoreError::Parse {
                    line: (*pattern).to_string(),
                    source,
                })?;
        }

        let user_path = ProjectPath::new(USER_IGNORE_PATH).expect("static path is valid");
        if fs.exists(&user_path) {
            let bytes = fs.read(&user_path)?;
            let text = String::from_utf8(bytes)?;
            for raw in text.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                builder
                    .add_line(None, line)
                    .map_err(|source| IgnoreError::Parse {
                        line: line.to_string(),
                        source,
                    })?;
            }
        }

        let matcher = builder.build().map_err(IgnoreError::Build)?;
        Ok(Self { matcher })
    }

    /// Build an `IgnoreRules` directly from a set of patterns, useful in
    /// tests that don't want to stand up a filesystem.
    pub fn from_patterns<I, S>(root: &std::path::Path, patterns: I) -> Result<Self, IgnoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = GitignoreBuilder::new(root);
        for pattern in patterns {
            let pattern = pattern.as_ref();
            builder
                .add_line(None, pattern)
                .map_err(|source| IgnoreError::Parse {
                    line: pattern.to_string(),
                    source,
                })?;
        }
        let matcher = builder.build().map_err(IgnoreError::Build)?;
        Ok(Self { matcher })
    }

    /// Return `true` if `path` should be ignored by the scanner.
    ///
    /// The project root itself is never considered ignored. `is_dir` should
    /// reflect the path's actual kind so that directory-only patterns
    /// (e.g. `node_modules/`) match correctly.
    #[must_use]
    pub fn is_ignored(&self, path: &ProjectPath, is_dir: bool) -> bool {
        if path.is_root() {
            return false;
        }
        matches!(
            self.matcher
                .matched_path_or_any_parents(path.as_str(), is_dir),
            Match::Ignore(_)
        )
    }

    /// Underlying [`Gitignore`] matcher, exposed for callers that need to
    /// integrate with the `ignore` crate directly (notably the scanner's
    /// `filter_entry` closure).
    pub(crate) fn matcher(&self) -> &Gitignore {
        &self.matcher
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::fs::{ProjectPath, StdFileSystem};

    fn setup() -> (TempDir, StdFileSystem) {
        let dir = TempDir::new().unwrap();
        let fs = StdFileSystem::new(dir.path().to_path_buf());
        (dir, fs)
    }

    fn ignored(rules: &IgnoreRules, path: &str, is_dir: bool) -> bool {
        rules.is_ignored(&ProjectPath::new(path).unwrap(), is_dir)
    }

    #[test]
    fn defaults_ignore_vcs_and_os_clutter() {
        let (_dir, fs) = setup();
        let rules = IgnoreRules::load(&fs).unwrap();

        assert!(ignored(&rules, ".git", true));
        assert!(ignored(&rules, ".DS_Store", false));
        assert!(ignored(&rules, "node_modules", true));
        assert!(ignored(&rules, "assets/.DS_Store", false));
    }

    #[test]
    fn defaults_ignore_dcc_autosave_patterns() {
        let (_dir, fs) = setup();
        let rules = IgnoreRules::load(&fs).unwrap();

        assert!(ignored(&rules, "scene.blend1", false));
        assert!(ignored(&rules, "assets/image.psd~", false));
        assert!(ignored(&rules, "project/.autosave", true));
    }

    #[test]
    fn defaults_ignore_the_progest_directory() {
        let (_dir, fs) = setup();
        let rules = IgnoreRules::load(&fs).unwrap();

        assert!(ignored(&rules, ".progest", true));
        assert!(ignored(&rules, ".progest/index.db", false));
    }

    #[test]
    fn user_ignore_file_extends_defaults() {
        let (_dir, fs) = setup();
        let user_path = ProjectPath::new(".progest/ignore").unwrap();
        fs.write_atomic(&user_path, b"*.cache\n# comment\n\nprivate/\n")
            .unwrap();
        let rules = IgnoreRules::load(&fs).unwrap();

        assert!(ignored(&rules, "build/output.cache", false));
        assert!(ignored(&rules, "private", true));
    }

    #[test]
    fn user_negation_unignores_default_match() {
        let (_dir, fs) = setup();
        let user_path = ProjectPath::new(".progest/ignore").unwrap();
        fs.write_atomic(&user_path, b"!keep.tmp\n").unwrap();
        let rules = IgnoreRules::load(&fs).unwrap();

        assert!(!ignored(&rules, "keep.tmp", false));
        assert!(ignored(&rules, "other.tmp", false));
    }

    #[test]
    fn regular_files_are_not_ignored() {
        let (_dir, fs) = setup();
        let rules = IgnoreRules::load(&fs).unwrap();

        assert!(!ignored(&rules, "assets/foo.psd", false));
        assert!(!ignored(&rules, "assets", true));
    }

    #[test]
    fn project_root_is_never_ignored() {
        let (_dir, fs) = setup();
        let rules = IgnoreRules::load(&fs).unwrap();
        assert!(!rules.is_ignored(&ProjectPath::root(), true));
    }

    #[test]
    fn from_patterns_builds_standalone_matcher() {
        let root = PathBuf::from("/tmp/example");
        let rules = IgnoreRules::from_patterns(&root, ["*.log", "tmp/"]).unwrap();
        assert!(ignored(&rules, "debug.log", false));
        assert!(ignored(&rules, "tmp", true));
        assert!(!ignored(&rules, "app.rs", false));
    }
}
