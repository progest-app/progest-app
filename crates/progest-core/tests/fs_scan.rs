//! End-to-end sanity checks for the `progest_core::fs` module, exercised
//! through the public API on a realistic tempdir fixture. Unit tests inside
//! the crate already cover individual concerns; this file asserts that the
//! re-exports compose correctly for downstream consumers (cli, tauri).

use std::collections::BTreeSet;
use std::fs;

use progest_core::fs::{EntryKind, IgnoreRules, ScanEntry, Scanner, StdFileSystem};
use tempfile::TempDir;

fn write(root: &std::path::Path, rel: &str, body: &str) {
    let target = root.join(rel);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(target, body).unwrap();
}

#[test]
fn realistic_project_layout_is_scanned_with_expected_entries() {
    let tmp = TempDir::new().unwrap();
    // Project contents we care about.
    write(tmp.path(), "assets/characters/hero.psd", "psd");
    write(tmp.path(), "assets/characters/.DS_Store", "junk");
    write(tmp.path(), "assets/backgrounds/bg01.png", "png");
    write(tmp.path(), "shots/s010/c001.mov", "mov");
    write(tmp.path(), "notes.md", "md");
    // Things that must be filtered out.
    write(tmp.path(), ".progest/project.toml", "");
    write(tmp.path(), ".progest/index.db", "");
    write(tmp.path(), "node_modules/pkg/index.js", "");
    write(tmp.path(), ".git/HEAD", "");
    write(tmp.path(), "scratch.blend1", "");
    // User-specified ignore rule.
    write(tmp.path(), ".progest/ignore", "scratch/\n");
    write(tmp.path(), "scratch/throwaway.txt", "");

    let fs_impl = StdFileSystem::new(tmp.path().to_path_buf());
    let rules = IgnoreRules::load(&fs_impl).unwrap();
    let scanner = Scanner::new(tmp.path().to_path_buf(), rules);

    let entries: Vec<ScanEntry> = scanner.into_iter().map(Result::unwrap).collect();
    let paths: BTreeSet<String> = entries
        .iter()
        .map(|e| e.path.as_str().to_string())
        .collect();

    // Kept entries.
    for expected in [
        "assets",
        "assets/characters",
        "assets/characters/hero.psd",
        "assets/backgrounds",
        "assets/backgrounds/bg01.png",
        "shots",
        "shots/s010",
        "shots/s010/c001.mov",
        "notes.md",
    ] {
        assert!(paths.contains(expected), "missing entry: {expected}");
    }

    // Entries that must not appear.
    for forbidden in [
        "assets/characters/.DS_Store",
        ".progest",
        ".progest/project.toml",
        ".progest/index.db",
        "node_modules",
        ".git",
        "scratch.blend1",
        "scratch",
        "scratch/throwaway.txt",
    ] {
        assert!(
            !paths.contains(forbidden),
            "entry should have been ignored: {forbidden}"
        );
    }

    // Kinds are reported correctly for at least one file and directory.
    let hero = entries
        .iter()
        .find(|e| e.path.as_str() == "assets/characters/hero.psd")
        .unwrap();
    assert_eq!(hero.kind, EntryKind::File);
    assert_eq!(hero.size, 3);

    let dir = entries
        .iter()
        .find(|e| e.path.as_str() == "assets/characters")
        .unwrap();
    assert_eq!(dir.kind, EntryKind::Dir);
}
