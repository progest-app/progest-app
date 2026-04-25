//! `progest scan` — reconcile the project tree against the index.

use std::path::Path;

use anyhow::{Context, Result};
use progest_core::fs::StdFileSystem;
use progest_core::index::SqliteIndex;
use progest_core::meta::StdMetaStore;
use progest_core::reconcile::{Reconciler, ScanReport};

use crate::context::discover_root;

/// Run `progest scan` starting the discovery walk from `cwd`.
pub fn run(cwd: &Path) -> Result<()> {
    let report = scan(cwd)?;
    print_report(&report);
    Ok(())
}

/// Shared scan routine used by both `scan` and `doctor` so the two never
/// disagree about what "reconciled" means.
pub(crate) fn scan(cwd: &Path) -> Result<ScanReport> {
    let root = discover_root(cwd)?;
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let meta = StdMetaStore::new(fs.clone());
    let index = SqliteIndex::open(&root.index_db())
        .with_context(|| format!("failed to open index at `{}`", root.index_db().display()))?;
    let reconciler = Reconciler::new(&fs, &meta, &index);
    reconciler.full_scan().context("reconcile full scan failed")
}

fn print_report(report: &ScanReport) {
    println!(
        "Scanned: {} added, {} updated, {} unchanged, {} removed",
        report.added(),
        report.updated(),
        report.unchanged(),
        report.removed(),
    );
    if !report.orphan_metas.is_empty() {
        println!(
            "Warning: {} orphan `.meta` file(s) detected (run `progest doctor` for details).",
            report.orphan_metas.len(),
        );
    }
}
