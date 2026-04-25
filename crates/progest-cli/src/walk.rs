//! Shared filtered project walker for reporting subcommands.
//!
//! `lint`, `clean`, and `rename` each accept the same `[PATH]...`
//! arguments and need the same scan + filter pipeline:
//!
//! 1. Build a [`Scanner`] anchored at the project root, honouring
//!    [`IgnoreRules`].
//! 2. Keep only `EntryKind::File` entries.
//! 3. If `paths` is non-empty, keep only entries whose `ProjectPath`
//!    starts with one of the requested filters (after normalizing
//!    each filter against the project root).
//!
//! Lifting the function here means the three subcommands no longer
//! drift on whitespace, error wording, or filter semantics.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use progest_core::fs::{EntryKind, IgnoreRules, ScanEntry, Scanner, StdFileSystem};
use progest_core::project::ProjectRoot;

/// Walk the project anchored at `root`, returning every file entry
/// that survives the ignore rules and the optional `paths` filter.
pub fn collect_entries(root: &ProjectRoot, paths: &[PathBuf]) -> Result<Vec<ScanEntry>> {
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let rules = IgnoreRules::load(&fs).with_context(|| {
        format!(
            "failed to load ignore rules from `{}`",
            root.root().display()
        )
    })?;
    let scanner = Scanner::new(root.root().to_path_buf(), rules);

    let mut out = Vec::new();
    for entry in scanner {
        let entry = entry.context("scan walk failed")?;
        if !matches!(entry.kind, EntryKind::File) {
            continue;
        }
        if !paths.is_empty() && !entry_matches_filter(&entry, paths, root.root()) {
            continue;
        }
        out.push(entry);
    }
    Ok(out)
}

/// Return `true` iff `entry`'s `ProjectPath` is rooted at one of the
/// `paths` filters (each filter is interpreted as either an absolute
/// path or a project-root-relative path; non-rooted filters are
/// silently dropped).
fn entry_matches_filter(entry: &ScanEntry, paths: &[PathBuf], root: &Path) -> bool {
    paths.iter().any(|p| {
        let abs = if p.is_absolute() {
            p.clone()
        } else {
            root.join(p)
        };
        let Ok(rel) = abs.strip_prefix(root) else {
            return false;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        entry.path.as_str().starts_with(rel_str.as_str())
    })
}
