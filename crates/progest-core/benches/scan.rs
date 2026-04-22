//! M1 completion-gate benchmark.
//!
//! Measures the wall-clock cost of a Progest reconcile on a 10k-file
//! project *after the first scan has already run*. The expected behaviour
//! on M1 is that incremental scans — size + mtime cheap compare, no
//! fingerprint recomputation — finish comfortably under 5 seconds.
//!
//! Run with:
//!
//! ```sh
//! cargo bench -p progest-core --bench scan
//! ```
//!
//! The initial bulk `.meta` generation is out of scope for the 5s budget
//! (documented in `docs/IMPLEMENTATION_PLAN.md` M1 section) — it happens
//! once as setup and is excluded from the measured iterations.

use std::fs;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use progest_core::fs::StdFileSystem;
use progest_core::index::SqliteIndex;
use progest_core::meta::StdMetaStore;
use progest_core::project;
use progest_core::reconcile::Reconciler;
use tempfile::TempDir;

const FILE_COUNT: usize = 10_000;
const DIRS: usize = 100;
const FILES_PER_DIR: usize = FILE_COUNT / DIRS;

/// Materialize a fresh project populated with `FILE_COUNT` tiny files
/// spread across `DIRS` subdirectories, then run the initial reconcile so
/// sidecars and index rows are in place. Returns the `TempDir` (kept alive
/// for the benchmark's lifetime) and its root path.
fn prepare_project() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    project::initialize(tmp.path(), "bench").expect("init");

    for d in 0..DIRS {
        let dir = tmp.path().join(format!("dir_{d:03}"));
        fs::create_dir_all(&dir).expect("mkdir");
        for f in 0..FILES_PER_DIR {
            let path = dir.join(format!("file_{f:03}.txt"));
            // Content is small but unique per file so fingerprint collisions
            // don't skew the benchmark.
            fs::write(&path, format!("d={d:03}/f={f:03}")).expect("write");
        }
    }

    let root = std::fs::canonicalize(tmp.path()).unwrap();
    let fs_impl = StdFileSystem::new(root.clone());
    let meta = StdMetaStore::new(fs_impl.clone());
    let index_path = root.join(".progest").join("index.db");
    let index = SqliteIndex::open(&index_path).expect("open index");
    let reconciler = Reconciler::new(&fs_impl, &meta, &index);
    let report = reconciler.full_scan().expect("initial scan");
    // FILE_COUNT real files plus the `.gitignore` that `project::initialize`
    // wrote (one more file) — everything under .progest/ is ignored by the
    // scanner, so it does not contribute.
    assert!(
        report.added() >= FILE_COUNT,
        "expected initial scan to add at least {FILE_COUNT} files, got {}",
        report.added()
    );
    tmp
}

fn bench_incremental_scan(c: &mut Criterion) {
    let tmp = prepare_project();
    let root = std::fs::canonicalize(tmp.path()).unwrap();

    let mut group = c.benchmark_group("incremental_scan_10k");
    // 10k-file scans take noticeable wall time even on the cheap-compare
    // path; default (100 iterations, 3s warm-up) would run for minutes.
    // 10 samples is plenty for the gate check we care about here.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    group.bench_function("second_pass", |b| {
        b.iter(|| {
            let fs_impl = StdFileSystem::new(root.clone());
            let meta = StdMetaStore::new(fs_impl.clone());
            let index = SqliteIndex::open(&root.join(".progest").join("index.db")).unwrap();
            let reconciler = Reconciler::new(&fs_impl, &meta, &index);
            let report = reconciler.full_scan().unwrap();
            // Guard: if anything surprised us (e.g. all files re-added
            // every run), the bench result would be meaningless.
            assert_eq!(report.added(), 0);
            assert_eq!(report.removed(), 0);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_incremental_scan);
criterion_main!(benches);
