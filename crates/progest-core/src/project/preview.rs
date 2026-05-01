//! Pre-flight inspection for [`super::layout::initialize`].
//!
//! The GUI runs this before showing a confirmation dialog so the user can
//! see (a) where the project will land, (b) whether the directory is already
//! a Progest project, and (c) — when initializing in place — roughly how
//! many files the first scan will pick up after the built-in ignore rules
//! are applied.
//!
//! The walk uses the same [`Scanner`] + [`IgnoreRules`] the reconciler does,
//! so the count matches what `FlatView` will show after `init` + initial scan.

use std::path::{Path, PathBuf};

use crate::fs::{IgnoreRules, Scanner};

use super::document::ProjectError;
use super::layout::DOT_DIR;

/// Snapshot of what `progest init` would do at `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPreview {
    /// Canonicalized target path when it exists, otherwise the path as
    /// supplied by the caller. Always absolute when `target_exists`.
    pub target_path: PathBuf,

    /// Whether the target directory currently exists on disk. `false` for
    /// the "new project" flow when the directory will be created by init.
    pub target_exists: bool,

    /// `true` when the target already contains a `.progest/` subdirectory.
    /// Callers should offer "open" rather than "initialize" in this case.
    pub is_existing_project: bool,

    /// Number of non-ignored files (regular + symlink) under `target_path`,
    /// applying the built-in default ignore rules. `None` when the target
    /// does not exist yet (new-project flow).
    pub predicted_file_count: Option<u64>,

    /// Relative paths that `initialize` will create or touch under the
    /// project root, in display order. Stable across calls; rendered as a
    /// bullet list in the confirmation dialog.
    pub artifacts: Vec<&'static str>,

    /// Whether a top-level `.gitignore` exists at `target_path`. When `true`,
    /// init will append the Progest entries; when `false`, init will create
    /// the file. Surfaced so the dialog can label the row appropriately.
    pub gitignore_exists: bool,
}

/// Inspect `target` without mutating anything.
///
/// `target` may or may not exist — for the "create new project" flow it is
/// the not-yet-created directory the dialog is about to make.
pub fn preview_init(target: &Path) -> Result<InitPreview, ProjectError> {
    let (target_path, target_exists) = if target.exists() {
        (dunce::canonicalize(target)?, true)
    } else {
        (target.to_path_buf(), false)
    };

    let is_existing_project = target_exists && target_path.join(DOT_DIR).is_dir();
    let gitignore_exists = target_exists && target_path.join(".gitignore").is_file();

    let predicted_file_count = if target_exists && !is_existing_project {
        Some(count_scannable_files(&target_path)?)
    } else {
        None
    };

    Ok(InitPreview {
        target_path,
        target_exists,
        is_existing_project,
        predicted_file_count,
        artifacts: INIT_ARTIFACTS.to_vec(),
        gitignore_exists,
    })
}

/// Relative paths that [`super::layout::initialize`] creates or appends to,
/// in the order the confirmation dialog should render them.
pub const INIT_ARTIFACTS: &[&str] = &[
    ".progest/project.toml",
    ".progest/ignore",
    ".progest/index.db",
    ".progest/local/",
    ".progest/thumbs/",
    ".gitignore",
];

fn count_scannable_files(root: &Path) -> Result<u64, ProjectError> {
    // Use the same matcher the scanner builds at runtime, but stand it up
    // from the static defaults instead of `IgnoreRules::load`. The target
    // isn't a Progest project yet so there's no `.progest/ignore` to read,
    // and we still want `.progest/` itself to be ignored in the existing-
    // project edge case (e.g. when previewing a directory that already has
    // it for some reason).
    let rules = IgnoreRules::from_patterns(root, crate::fs::DEFAULT_PATTERNS.iter().copied())
        .map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;

    let mut count: u64 = 0;
    for entry in Scanner::new(root.to_path_buf(), rules) {
        let entry = entry.map_err(|e| ProjectError::Io(std::io::Error::other(e)))?;
        if matches!(
            entry.kind,
            crate::fs::EntryKind::File | crate::fs::EntryKind::Symlink
        ) {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::project::layout::initialize;

    fn write(root: &Path, rel: &str, body: &str) {
        let target = root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(target, body).unwrap();
    }

    #[test]
    fn preview_for_nonexistent_target_reports_no_count() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("brand-new-project");
        let preview = preview_init(&target).unwrap();
        assert!(!preview.target_exists);
        assert!(!preview.is_existing_project);
        assert!(!preview.gitignore_exists);
        assert_eq!(preview.predicted_file_count, None);
        assert!(preview.artifacts.contains(&".progest/project.toml"));
    }

    #[test]
    fn preview_for_empty_existing_directory_reports_zero_files() {
        let tmp = TempDir::new().unwrap();
        let preview = preview_init(tmp.path()).unwrap();
        assert!(preview.target_exists);
        assert!(!preview.is_existing_project);
        assert_eq!(preview.predicted_file_count, Some(0));
    }

    #[test]
    fn preview_counts_non_ignored_files_only() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "assets/foo.psd", "x");
        write(tmp.path(), "notes.txt", "y");
        write(tmp.path(), "node_modules/dep/index.js", "z");
        write(tmp.path(), ".DS_Store", "z");
        write(tmp.path(), "scene.blend1", "z");

        let preview = preview_init(tmp.path()).unwrap();
        assert_eq!(preview.predicted_file_count, Some(2));
    }

    #[test]
    fn preview_detects_existing_progest_project() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("demo");
        initialize(&project, "Demo").unwrap();

        let preview = preview_init(&project).unwrap();
        assert!(preview.is_existing_project);
        // The count is intentionally `None` for an existing project — the
        // dialog should redirect to the open-project flow rather than show
        // a misleading number that won't match a real scan.
        assert_eq!(preview.predicted_file_count, None);
    }

    #[test]
    fn preview_flags_existing_gitignore() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), ".gitignore", "*.log\n");
        let preview = preview_init(tmp.path()).unwrap();
        assert!(preview.gitignore_exists);
    }

    #[test]
    fn preview_count_matches_subsequent_scan() {
        // Regression guard: the predicted count must come from the same
        // matcher the scanner uses, otherwise the dialog will lie about
        // what `progest init` + first reconcile is going to surface.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "assets/a.psd", "x");
        write(tmp.path(), "assets/b.psd", "y");
        write(tmp.path(), "scratch/note.txt", "z");
        write(tmp.path(), ".DS_Store", "z");
        write(tmp.path(), "node_modules/dep/index.js", "z");

        let predicted = preview_init(tmp.path()).unwrap().predicted_file_count;

        // Re-run the same walk via Scanner directly with the production
        // load path; the two numbers must agree.
        let fs = crate::fs::StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let actual: u64 = Scanner::new(tmp.path().to_path_buf(), rules)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| {
                matches!(
                    e.kind,
                    crate::fs::EntryKind::File | crate::fs::EntryKind::Symlink
                )
            })
            .count() as u64;

        assert_eq!(predicted, Some(actual));
    }
}
