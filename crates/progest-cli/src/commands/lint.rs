//! `progest lint` — rules + accepts + sequence-drift report.
//!
//! Walks the project tree (or the filtered subset in `paths`) and
//! hands the result to `core::lint::lint_paths`. Emits the grouped
//! report as either a human-facing text list or a stable JSON wire
//! that downstream tools (Tauri panel, saved-search integrations,
//! CI gates) can consume.
//!
//! Exit code follows DSL §8.2:
//!
//! - `0` — no `strict` and no `evaluation_error` violations
//! - `1` — at least one `strict` or `evaluation_error`
//!
//! `--format json` always prints the JSON report to stdout,
//! regardless of exit code, so `progest lint --format json | jq ...`
//! works uniformly on clean and dirty trees.

use std::path::Path;
use std::path::PathBuf;

use anyhow::{Context, Result};
use progest_core::fs::StdFileSystem;
use progest_core::index::Index;
use progest_core::lint::{LintOptions, LintReport, lint_paths, write_to_index};
use progest_core::meta::StdMetaStore;
use progest_core::rules::{BUILTIN_COMPOUND_EXTS, Severity, Violation};
use serde::Serialize;

use crate::context::{
    CleanupOverrides, discover_root, load_alias_catalog_from_root, load_cleanup_config,
    load_ruleset, open_index,
};
use crate::output::{OutputFormat, emit_json};
use crate::walk::collect_entries;

pub struct LintArgs {
    pub paths: Vec<PathBuf>,
    pub format: OutputFormat,
    /// Retain rule traces for every evaluated file, not just violating
    /// ones. Produces a much larger JSON payload — only useful when
    /// authoring rules and wondering why a file matched the rule it did.
    pub explain: bool,
}

pub fn run(cwd: &Path, args: &LintArgs) -> Result<i32> {
    let root = discover_root(cwd)?;

    let ruleset = load_ruleset(&root)?;
    let catalog = load_alias_catalog_from_root(&root)?;
    let cleanup = load_cleanup_config(&root, &CleanupOverrides::default())?;

    let entries = collect_entries(&root, &args.paths)?;
    let paths: Vec<_> = entries.into_iter().map(|e| e.path).collect();

    let fs = StdFileSystem::new(root.root().to_path_buf());
    let store = StdMetaStore::new(fs);

    let opts = LintOptions {
        ruleset: &ruleset,
        alias_catalog: &catalog,
        compound_exts: BUILTIN_COMPOUND_EXTS,
        cleanup_cfg: &cleanup,
        explain: args.explain,
    };
    let report =
        lint_paths(store.filesystem(), &store, &paths, &opts).context("running lint pass")?;

    // Persist results into the search index so `is:violation` /
    // `is:misplaced` queries see the latest state. Failures here
    // are non-fatal — the lint output is still emitted, but stderr
    // gets a warning so users can investigate. (Index drift is
    // recoverable: any subsequent lint run rewrites the rows.)
    let index = open_index(&root).context("opening index for violations writer")?;
    let visited: Vec<_> = paths
        .iter()
        .filter_map(|p| index.get_file_by_path(p).ok().flatten())
        .map(|row| row.file_id)
        .collect();
    if let Err(e) = write_to_index(&index, &visited, &report) {
        eprintln!("warning: failed to update violations index: {e}");
    }

    match args.format {
        OutputFormat::Text => emit_text(&report),
        OutputFormat::Json => emit_json(&report, "lint")?,
    }

    Ok(i32::from(report.fails_ci()))
}

// --- Emit ------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TextBucket<'a> {
    title: &'static str,
    rows: &'a [Violation],
}

fn emit_text(report: &LintReport) {
    let buckets = [
        TextBucket {
            title: "naming",
            rows: &report.naming,
        },
        TextBucket {
            title: "placement",
            rows: &report.placement,
        },
        TextBucket {
            title: "sequence",
            rows: &report.sequence,
        },
    ];

    let mut any = false;
    for b in &buckets {
        if b.rows.is_empty() {
            continue;
        }
        any = true;
        println!("[{}]", b.title);
        for v in b.rows {
            println!(
                "  {} ({}) [{}]",
                v.path.as_str(),
                v.rule_id.as_str(),
                severity_label(v.severity),
            );
            println!("    {}", v.reason);
            if !v.suggested_names.is_empty() {
                println!("    suggest: {}", v.suggested_names.join(", "));
            }
        }
        println!();
    }

    if !any {
        println!("(clean)");
    }

    let s = &report.summary;
    println!(
        "Summary: {} scanned — {} naming / {} placement / {} sequence — {} strict, {} eval-error, {} warn, {} hint",
        s.scanned,
        s.naming_count,
        s.placement_count,
        s.sequence_count,
        s.strict_count,
        s.evaluation_error_count,
        s.warn_count,
        s.hint_count,
    );
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Strict => "strict",
        Severity::Warn => "warn",
        Severity::Hint => "hint",
        Severity::EvaluationError => "eval-error",
    }
}
