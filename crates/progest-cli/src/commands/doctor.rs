//! `progest doctor` — integrity report for the project.
//!
//! The M1 doctor is intentionally limited to what `Reconciler::full_scan`
//! already reports: orphan `.meta` sidecars whose companion file is gone.
//! Richer diagnostics (UUID conflicts, index/sidecar fingerprint drift)
//! land alongside the corresponding reconcile extensions in later
//! milestones.

use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;

use super::scan;

/// Run `progest doctor` from `cwd`. Returns an [`ExitCode`] so the caller
/// can surface a non-zero code when integrity issues are found — the rest
/// of the CLI uses [`anyhow::Result`], which collapses to exit code 1 only
/// on error, and "found 3 orphans" is not an error per se.
pub fn run(cwd: &Path) -> Result<ExitCode> {
    let report = scan::scan(cwd)?;

    println!(
        "Scan summary: {} added, {} updated, {} unchanged, {} removed",
        report.added(),
        report.updated(),
        report.unchanged(),
        report.removed(),
    );

    if report.orphan_metas.is_empty() {
        println!("No integrity issues detected.");
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "\nFound {} orphan `.meta` file(s):",
        report.orphan_metas.len()
    );
    for path in &report.orphan_metas {
        println!("  • {path}");
    }
    println!(
        "\nOrphan sidecars usually mean the companion file was deleted without its `.meta`. Review each entry and decide whether to delete the sidecar or restore the file."
    );
    Ok(ExitCode::from(2))
}
