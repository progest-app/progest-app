//! Synchronous project scanner.
//!
//! [`Scanner`] walks the project tree beneath a root directory and yields one
//! [`ScanEntry`] per surviving file or directory. The traversal is filtered
//! by [`IgnoreRules`], so anything matched by the built-in defaults or the
//! user's `.progest/ignore` is skipped entirely — subtrees under an ignored
//! directory are not descended into.
//!
//! Symlinks are surfaced as `EntryKind::Symlink` but never followed. v1 of
//! Progest treats them as pointers, not files (see docs/REQUIREMENTS.md
//! §7.2), so higher layers can warn and move on.
//!
//! The iterator is synchronous because the M1 performance budget
//! (10k files < 5s) is comfortably within a single core's reach. Callers that
//! want parallelism can wrap the iterator in rayon or dispatch work across
//! channels themselves.

use std::path::PathBuf;
use std::time::SystemTime;

use ignore::{Match, Walk, WalkBuilder};
use thiserror::Error;

use super::{IgnoreRules, ProjectPath, ProjectPathError};

/// Whether a [`ScanEntry`] represents a file, directory, or symlink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

/// One entry produced by [`Scanner::iter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    pub path: ProjectPath,
    pub kind: EntryKind,
    pub size: u64,
    pub mtime: SystemTime,
}

/// Errors raised while walking the tree.
#[derive(Debug, Error)]
pub enum ScanError {
    #[error("walk error: {0}")]
    Walk(#[from] ignore::Error),
    #[error(transparent)]
    Path(#[from] ProjectPathError),
}

/// Immutable handle to a project root and its ignore rules. Cheap to clone
/// relative to the work done by [`Scanner::iter`]; hold one per project.
pub struct Scanner {
    root: PathBuf,
    rules: IgnoreRules,
}

impl Scanner {
    /// Build a scanner for the project rooted at `root` using `rules`.
    #[must_use]
    pub fn new(root: PathBuf, rules: IgnoreRules) -> Self {
        Self { root, rules }
    }
}

impl IntoIterator for Scanner {
    type Item = Result<ScanEntry, ScanError>;
    type IntoIter = ScanIter;

    fn into_iter(self) -> ScanIter {
        let matcher = self.rules.matcher().clone();
        let root = self.root.clone();
        let filter_root = self.root.clone();
        let walker = WalkBuilder::new(&self.root)
            .standard_filters(false)
            .hidden(false)
            .follow_links(false)
            .filter_entry(move |entry| {
                // Always retain the root itself — the iterator skips it later.
                if entry.path() == filter_root {
                    return true;
                }
                let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
                !matches!(
                    matcher.matched_path_or_any_parents(entry.path(), is_dir),
                    Match::Ignore(_)
                )
            })
            .build();
        ScanIter { walker, root }
    }
}

/// Iterator form returned by [`Scanner::into_iter`].
pub struct ScanIter {
    walker: Walk,
    root: PathBuf,
}

impl Iterator for ScanIter {
    type Item = Result<ScanEntry, ScanError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = match self.walker.next()? {
                Ok(entry) => entry,
                Err(err) => return Some(Err(ScanError::Walk(err))),
            };

            // The root itself is always yielded by `ignore::Walk`; we hide it
            // because scan consumers only care about contents.
            if entry.path() == self.root {
                continue;
            }

            let Some(file_type) = entry.file_type() else {
                continue;
            };

            let kind = if file_type.is_symlink() {
                EntryKind::Symlink
            } else if file_type.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };

            let path = match ProjectPath::from_absolute(&self.root, entry.path()) {
                Ok(p) => p,
                Err(e) => return Some(Err(ScanError::Path(e))),
            };

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => return Some(Err(ScanError::Walk(e))),
            };
            let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

            return Some(Ok(ScanEntry {
                path,
                kind,
                size: metadata.len(),
                mtime,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::fs::StdFileSystem;

    fn write(root: &std::path::Path, rel: &str, body: &str) {
        let target = root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(target, body).unwrap();
    }

    fn collect(root: PathBuf, rules: IgnoreRules) -> Vec<ScanEntry> {
        Scanner::new(root, rules)
            .into_iter()
            .map(Result::unwrap)
            .collect()
    }

    fn paths(entries: &[ScanEntry]) -> BTreeSet<String> {
        entries
            .iter()
            .map(|e| e.path.as_str().to_string())
            .collect()
    }

    #[test]
    fn scanner_yields_files_and_directories_below_root() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "assets/foo.psd", "x");
        write(tmp.path(), "notes.txt", "y");

        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let entries = collect(tmp.path().to_path_buf(), rules);

        let names = paths(&entries);
        assert!(names.contains("assets"));
        assert!(names.contains("assets/foo.psd"));
        assert!(names.contains("notes.txt"));
    }

    #[test]
    fn scanner_respects_default_ignore_rules() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "keep.psd", "x");
        write(tmp.path(), "node_modules/dep/index.js", "x");
        write(tmp.path(), ".DS_Store", "x");
        write(tmp.path(), "scene.blend1", "x");
        write(tmp.path(), ".git/HEAD", "x");

        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let entries = collect(tmp.path().to_path_buf(), rules);

        let names = paths(&entries);
        assert!(names.contains("keep.psd"));
        assert!(!names.iter().any(|p| p.starts_with("node_modules")));
        assert!(!names.contains(".DS_Store"));
        assert!(!names.contains("scene.blend1"));
        assert!(!names.iter().any(|p| p.starts_with(".git")));
    }

    #[test]
    fn scanner_respects_user_ignore_file() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "keep.psd", "x");
        write(tmp.path(), "build/out.cache", "x");
        write(
            tmp.path(),
            ".progest/ignore",
            "*.cache\n# skip build artifacts\nbuild/\n",
        );

        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let entries = collect(tmp.path().to_path_buf(), rules);

        let names = paths(&entries);
        assert!(names.contains("keep.psd"));
        assert!(!names.iter().any(|p| p.starts_with("build")));
    }

    #[test]
    fn scanner_skips_the_progest_directory_entirely() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "a.psd", "x");
        write(tmp.path(), ".progest/project.toml", "x");
        write(tmp.path(), ".progest/index.db", "x");

        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let entries = collect(tmp.path().to_path_buf(), rules);

        let names = paths(&entries);
        assert!(names.contains("a.psd"));
        assert!(!names.iter().any(|p| p.starts_with(".progest")));
    }

    #[test]
    fn scanner_reports_file_sizes() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "note.txt", "1234567890");

        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let entries = collect(tmp.path().to_path_buf(), rules);

        let note = entries
            .iter()
            .find(|e| e.path.as_str() == "note.txt")
            .expect("note.txt should be present");
        assert_eq!(note.kind, EntryKind::File);
        assert_eq!(note.size, 10);
    }

    #[cfg(unix)]
    #[test]
    fn scanner_surfaces_symlinks_without_following() {
        use std::os::unix::fs::symlink;

        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "target.txt", "t");
        symlink(tmp.path().join("target.txt"), tmp.path().join("link.txt")).unwrap();

        let fs = StdFileSystem::new(tmp.path().to_path_buf());
        let rules = IgnoreRules::load(&fs).unwrap();
        let entries = collect(tmp.path().to_path_buf(), rules);

        let link = entries
            .iter()
            .find(|e| e.path.as_str() == "link.txt")
            .expect("symlink should appear in scan output");
        assert_eq!(link.kind, EntryKind::Symlink);
    }
}
