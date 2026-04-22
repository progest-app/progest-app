//! Integration tests for the `.meta` pending-write queue.
//!
//! Exercises `StdMetaStore` against a real tempdir so the full loop —
//! fail to save → enqueue → next successful save drains — is covered
//! end-to-end. Failures are induced by deliberately making the sidecar's
//! parent something that is not a directory, which is the same surface
//! an OS-level `ENOTDIR` would expose in production.

use std::fs;

use progest_core::fs::{ProjectPath, StdFileSystem};
use progest_core::identity::{FileId, Fingerprint};
use progest_core::meta::{
    MetaDocument, MetaStore, PENDING_DIR, PendingQueue, StdMetaStore, sidecar_path,
};
use tempfile::TempDir;

fn sample_fingerprint() -> Fingerprint {
    "blake3:00112233445566778899aabbccddeeff"
        .parse()
        .expect("literal fingerprint parses")
}

fn sample_doc() -> MetaDocument {
    MetaDocument::new(FileId::new_v7(), sample_fingerprint())
}

#[test]
fn save_failure_enqueues_a_pending_entry() {
    let tmp = TempDir::new().unwrap();
    // Block `assets/` by creating it as a regular file. Any attempt to
    // write inside it fails with ENOTDIR.
    fs::write(tmp.path().join("assets"), b"not a directory").unwrap();

    let fs_impl = StdFileSystem::new(tmp.path().to_path_buf());
    let store = StdMetaStore::new(fs_impl.clone());

    let sidecar = sidecar_path(&ProjectPath::new("assets/hero.psd").unwrap()).unwrap();
    let result = store.save(&sidecar, &sample_doc());
    assert!(result.is_err(), "expected save to fail, got {result:?}");

    let queue = PendingQueue::new(&fs_impl);
    let entries = queue.list().unwrap();
    assert_eq!(entries.len(), 1, "pending entries: {entries:#?}");
    assert_eq!(entries[0].target, sidecar.as_str());
    assert!(entries[0].last_error.is_some());
    assert_eq!(entries[0].attempts, 0);

    // Verify the on-disk queue dir exists so a future doctor can find it.
    let dir_abs = tmp.path().join(PENDING_DIR);
    assert!(
        dir_abs.is_dir(),
        "expected {} to be a directory",
        dir_abs.display()
    );
}

#[test]
fn next_successful_save_drains_the_pending_queue() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("assets"), b"not a directory").unwrap();

    let fs_impl = StdFileSystem::new(tmp.path().to_path_buf());
    let store = StdMetaStore::new(fs_impl.clone());

    let blocked = sidecar_path(&ProjectPath::new("assets/hero.psd").unwrap()).unwrap();
    let _ = store.save(&blocked, &sample_doc());
    assert_eq!(PendingQueue::new(&fs_impl).list().unwrap().len(), 1);

    // Clear the obstruction — swap the file out for a directory so retries
    // can actually succeed.
    fs::remove_file(tmp.path().join("assets")).unwrap();
    fs::create_dir_all(tmp.path().join("assets")).unwrap();

    // Any subsequent operation on the store triggers the implicit flush,
    // which should drain the queued write and write it to disk.
    let other = sidecar_path(&ProjectPath::new("notes.txt").unwrap()).unwrap();
    store.save(&other, &sample_doc()).unwrap();

    assert!(PendingQueue::new(&fs_impl).list().unwrap().is_empty());
    assert!(tmp.path().join("assets/hero.psd.meta").is_file());
    assert!(tmp.path().join("notes.txt.meta").is_file());
}

#[test]
fn repeated_failure_bumps_attempts_on_the_existing_entry() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("assets"), b"not a directory").unwrap();

    let fs_impl = StdFileSystem::new(tmp.path().to_path_buf());
    let store = StdMetaStore::new(fs_impl.clone());

    let sidecar = sidecar_path(&ProjectPath::new("assets/hero.psd").unwrap()).unwrap();
    let _ = store.save(&sidecar, &sample_doc());

    // Issue another save to a still-blocked sibling — the flush will retry
    // the queued entry, fail again, and bump `attempts`.
    let sibling = sidecar_path(&ProjectPath::new("assets/villain.psd").unwrap()).unwrap();
    let _ = store.save(&sibling, &sample_doc());

    let entries = PendingQueue::new(&fs_impl).list().unwrap();
    // Two targets in flight: the original, with attempts bumped; and the
    // sibling, freshly enqueued.
    assert_eq!(entries.len(), 2);
    let original = entries
        .iter()
        .find(|e| e.target == sidecar.as_str())
        .expect("original entry present");
    assert!(
        original.attempts >= 1,
        "expected attempts >= 1, got {}",
        original.attempts
    );
}

#[test]
fn explicit_flush_pending_is_idempotent_when_queue_is_empty() {
    let tmp = TempDir::new().unwrap();
    let fs_impl = StdFileSystem::new(tmp.path().to_path_buf());
    let store = StdMetaStore::new(fs_impl);

    // No queue exists yet; flush_pending must not error.
    store.flush_pending().unwrap();
    store.flush_pending().unwrap();
}
