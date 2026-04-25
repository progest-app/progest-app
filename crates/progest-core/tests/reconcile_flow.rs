//! End-to-end checks for `progest_core::reconcile` exercised against real
//! on-disk state (`StdFileSystem`, `SqliteIndex`, `StdMetaStore`).
//!
//! Unit tests inside the crate cover individual merge helpers; this suite
//! drives the reconciler the way CLI `scan` and the watch worker will —
//! producing and consuming `.meta` sidecars and `SQLite` rows together — so
//! that regressions in the three-way interaction surface here first.

use std::fs;

mod support;

use progest_core::fs::{FileSystem, ProjectPath, StdFileSystem};
use progest_core::index::{Index, SqliteIndex};
use progest_core::meta::{MetaStore, SIDECAR_SUFFIX, StdMetaStore, sidecar_path};
use progest_core::reconcile::{ChangeSet, FsEvent, ReconcileOutcome, Reconciler};
use tempfile::TempDir;

use support::p as path;

/// Bundle of collaborators rooted at a freshly created tempdir.
///
/// Owning the `TempDir` keeps it alive for the test's duration; dropping the
/// bundle tears the directory down. Collaborator references are produced via
/// the `reconciler` helper, which borrows them together so the `Reconciler`
/// itself doesn't have to outlive them.
struct Harness {
    _tmp: TempDir,
    root: std::path::PathBuf,
    fs: StdFileSystem,
    meta: StdMetaStore<StdFileSystem>,
    index: SqliteIndex,
}

impl Harness {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let fs = StdFileSystem::new(root.clone());
        let meta = StdMetaStore::new(fs.clone());
        let index = SqliteIndex::open_in_memory().unwrap();
        Self {
            _tmp: tmp,
            root,
            fs,
            meta,
            index,
        }
    }

    fn reconciler(&self) -> Reconciler<'_> {
        Reconciler::new(&self.fs, &self.meta, &self.index)
    }

    fn write(&self, rel: &str, body: &[u8]) {
        let target = self.root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&target, body).unwrap();
    }

    fn remove(&self, rel: &str) {
        fs::remove_file(self.root.join(rel)).unwrap();
    }

    fn abs(&self, rel: &str) -> std::path::PathBuf {
        self.root.join(rel)
    }
}

fn sidecar(rel: &str) -> ProjectPath {
    sidecar_path(&ProjectPath::new(rel).unwrap()).unwrap()
}

#[test]
fn first_scan_creates_sidecar_and_indexes_every_file() {
    let h = Harness::new();
    h.write("hero.psd", b"hero-bytes");
    h.write("shots/s010/c001.mov", b"movie-bytes");

    let report = h.reconciler().full_scan().unwrap();

    assert_eq!(report.added(), 2);
    assert_eq!(report.unchanged(), 0);
    assert_eq!(report.updated(), 0);
    assert_eq!(report.removed(), 0);
    assert!(report.orphan_metas.is_empty());

    for rel in ["hero.psd", "shots/s010/c001.mov"] {
        assert!(h.meta.exists(&sidecar(rel)), "expected {rel}.meta");
        let row = h.index.get_file_by_path(&path(rel)).unwrap();
        assert!(row.is_some(), "expected index row for {rel}");
    }
}

#[test]
fn second_scan_is_idempotent_without_fs_changes() {
    let h = Harness::new();
    h.write("hero.psd", b"hero-bytes");
    h.reconciler().full_scan().unwrap();

    let report = h.reconciler().full_scan().unwrap();

    assert_eq!(report.added(), 0);
    assert_eq!(report.updated(), 0);
    assert_eq!(report.removed(), 0);
    assert_eq!(report.unchanged(), 1);
}

#[test]
fn modifying_a_file_bumps_fingerprint_and_index_row() {
    let h = Harness::new();
    h.write("hero.psd", b"v1");
    h.reconciler().full_scan().unwrap();
    let original = h
        .index
        .get_file_by_path(&path("hero.psd"))
        .unwrap()
        .unwrap();
    let original_sidecar = h.meta.load(&sidecar("hero.psd")).unwrap();

    // Overwrite with different content and a different size: the cheap
    // compare misses on size alone, which is enough to drive the fingerprint
    // recompute path without taking a dependency on mtime bumping.
    h.write("hero.psd", b"v2-longer");

    let report = h.reconciler().full_scan().unwrap();

    assert_eq!(report.updated(), 1);
    let updated = h
        .index
        .get_file_by_path(&path("hero.psd"))
        .unwrap()
        .unwrap();
    assert_eq!(updated.file_id, original.file_id, "file_id must be stable");
    assert_ne!(updated.fingerprint, original.fingerprint);
    let updated_sidecar = h.meta.load(&sidecar("hero.psd")).unwrap();
    assert_eq!(updated_sidecar.file_id, original_sidecar.file_id);
    assert_eq!(updated_sidecar.content_fingerprint, updated.fingerprint);
}

#[test]
fn deleting_a_file_removes_the_index_row_but_leaves_sidecar() {
    let h = Harness::new();
    h.write("hero.psd", b"bytes");
    h.reconciler().full_scan().unwrap();
    let id = h
        .index
        .get_file_by_path(&path("hero.psd"))
        .unwrap()
        .unwrap()
        .file_id;

    h.remove("hero.psd");

    let report = h.reconciler().full_scan().unwrap();

    assert_eq!(report.removed(), 1);
    assert!(h.index.get_file(&id).unwrap().is_none());
    assert!(
        h.meta.exists(&sidecar("hero.psd")),
        ".meta must survive FS deletion so doctor can resolve it"
    );
}

#[test]
fn orphan_sidecars_are_reported_without_side_effects() {
    let h = Harness::new();
    // A sidecar with no companion — perhaps a teammate deleted the file
    // without cleaning up the `.meta`.
    h.write(&format!("ghost.psd{SIDECAR_SUFFIX}"), b"{}\n");

    let report = h.reconciler().full_scan().unwrap();

    assert!(report.outcomes.is_empty());
    assert_eq!(report.orphan_metas.len(), 1);
    assert_eq!(report.orphan_metas[0].as_str(), "ghost.psd.meta");
    // Side effect check: orphan sidecar must still exist on disk.
    assert!(h.fs.exists(&path("ghost.psd.meta")));
}

#[test]
fn preexisting_sidecar_is_respected_on_first_scan() {
    // Simulate a freshly cloned git checkout: files and `.meta` sidecars
    // arrive together, but the local index is empty. The reconciler must
    // adopt the sidecar's `file_id` rather than minting a new one.
    let h = Harness::new();
    h.write("hero.psd", b"bytes");
    let preset = {
        use progest_core::identity::{FileId, compute_fingerprint};
        use progest_core::meta::MetaDocument;
        let fp = compute_fingerprint(std::io::Cursor::new(b"bytes")).unwrap();
        MetaDocument::new(FileId::new_v7(), fp)
    };
    h.meta.save(&sidecar("hero.psd"), &preset).unwrap();

    h.reconciler().full_scan().unwrap();

    let row = h
        .index
        .get_file_by_path(&path("hero.psd"))
        .unwrap()
        .unwrap();
    assert_eq!(
        row.file_id, preset.file_id,
        "reconciler must adopt the sidecar's identity"
    );
    let reloaded = h.meta.load(&sidecar("hero.psd")).unwrap();
    assert_eq!(reloaded.file_id, preset.file_id);
}

#[test]
fn apply_changes_added_event_matches_full_scan_outcome() {
    let h = Harness::new();
    h.write("hero.psd", b"bytes");

    let report = h
        .reconciler()
        .apply_changes(&ChangeSet::from_events([FsEvent::Added(path("hero.psd"))]))
        .unwrap();

    assert_eq!(report.outcomes.len(), 1);
    assert!(matches!(report.outcomes[0], ReconcileOutcome::Added { .. }));
    assert!(h.meta.exists(&sidecar("hero.psd")));
    assert!(
        h.index
            .get_file_by_path(&path("hero.psd"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn apply_changes_removed_event_drops_index_row() {
    let h = Harness::new();
    h.write("hero.psd", b"bytes");
    h.reconciler().full_scan().unwrap();
    let id = h
        .index
        .get_file_by_path(&path("hero.psd"))
        .unwrap()
        .unwrap()
        .file_id;

    h.remove("hero.psd");
    let report = h
        .reconciler()
        .apply_changes(&ChangeSet::from_events([FsEvent::Removed(path(
            "hero.psd",
        ))]))
        .unwrap();

    assert_eq!(report.outcomes.len(), 1);
    assert!(matches!(
        report.outcomes[0],
        ReconcileOutcome::Removed { .. }
    ));
    assert!(h.index.get_file(&id).unwrap().is_none());
}

#[test]
fn apply_changes_renamed_event_preserves_file_id() {
    let h = Harness::new();
    h.write("hero.psd", b"bytes");
    h.reconciler().full_scan().unwrap();
    let original = h
        .index
        .get_file_by_path(&path("hero.psd"))
        .unwrap()
        .unwrap();

    // Move the file on disk so apply_changes' metadata lookup succeeds.
    fs::rename(h.abs("hero.psd"), h.abs("hero_v2.psd")).unwrap();

    let report = h
        .reconciler()
        .apply_changes(&ChangeSet::from_events([FsEvent::Renamed {
            from: path("hero.psd"),
            to: path("hero_v2.psd"),
        }]))
        .unwrap();

    assert_eq!(report.outcomes.len(), 1);
    assert!(matches!(
        report.outcomes[0],
        ReconcileOutcome::Updated { .. }
    ));
    let moved = h
        .index
        .get_file_by_path(&path("hero_v2.psd"))
        .unwrap()
        .unwrap();
    assert_eq!(moved.file_id, original.file_id);
    assert!(
        h.index
            .get_file_by_path(&path("hero.psd"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn apply_changes_ignores_sidecar_paths() {
    // Sidecar churn should never reach apply_changes — the reconciler
    // normalizes around the companion file — but if the watch layer forwards
    // one anyway (e.g. a git checkout modifies `.meta` alongside the file),
    // the event must be a no-op rather than treating the sidecar as a file.
    let h = Harness::new();
    h.write("hero.psd", b"bytes");
    h.reconciler().full_scan().unwrap();

    let report = h
        .reconciler()
        .apply_changes(&ChangeSet::from_events([FsEvent::Modified(sidecar(
            "hero.psd",
        ))]))
        .unwrap();

    assert!(report.outcomes.is_empty());
}
