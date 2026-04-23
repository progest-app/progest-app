//! `progest clean` — mechanical name cleanup with preview / apply.
//!
//! Walks the project tree, runs the `core::naming` pipeline over
//! every surviving file's basename, and reports the candidates the
//! pipeline would produce. `--apply` runs the changed-and-resolved
//! candidates through `core::rename::Rename::apply` for an atomic
//! commit, sharing the staging + history wiring with
//! `progest rename`.
//!
//! Flags layer on top of `.progest/project.toml [cleanup]`:
//!
//! - `--case <style>`: override `convert_case`
//! - `--strip-cjk` / `--strip-suffix`: force the flag on (no way to
//!   force off — users who disable at the config level and want a
//!   one-shot preview with the flag off should edit `project.toml`)
//! - `--fill-mode <skip|placeholder>`: how to resolve holes for the
//!   final string. `placeholder` uses `--placeholder <STR>` or `_`.
//! - `--format <text|json>`: text for humans, JSON for scripts.
//!
//! Exit code is always `0` for a successful preview — having things
//! to clean is not a failure. Genuine errors (no project, walker
//! blew up) exit non-zero via `anyhow`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use progest_core::fs::{EntryKind, IgnoreRules, ScanEntry, Scanner, StdFileSystem};
use progest_core::history::SqliteStore as HistoryStore;
use progest_core::index::SqliteIndex;
use progest_core::naming::{
    CaseStyle, CleanupConfig, FillMode, Hole, NameCandidate, Segment, clean_basename,
    extract_cleanup_config, resolve,
};
use progest_core::project::{ProjectDocument, ProjectRoot};
use progest_core::rename::{
    ApplyOutcome, Rename, RenameRequest, build_preview, build_preview_with_prompter,
};
use progest_core::rules::BUILTIN_COMPOUND_EXTS;
use serde::Serialize;

use crate::prompter::StdinHolePrompter;

// --- CLI flag types --------------------------------------------------------

#[derive(ValueEnum, Clone, Debug)]
pub enum FormatFlag {
    Text,
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CaseFlag {
    Off,
    Snake,
    Kebab,
    Camel,
    Pascal,
    Lower,
    Upper,
}

impl CaseFlag {
    fn to_style(&self) -> CaseStyle {
        match self {
            Self::Off => CaseStyle::Off,
            Self::Snake => CaseStyle::Snake,
            Self::Kebab => CaseStyle::Kebab,
            Self::Camel => CaseStyle::Camel,
            Self::Pascal => CaseStyle::Pascal,
            Self::Lower => CaseStyle::Lower,
            Self::Upper => CaseStyle::Upper,
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum FillFlag {
    Skip,
    Placeholder,
    /// Interactive: prompt the user (via stdin/stderr) for each hole.
    /// Wired in by `progest rename`; `progest clean` still treats this
    /// as `skip` because clean is preview-only.
    Prompt,
}

// --- Entry point -----------------------------------------------------------

pub struct CleanArgs {
    pub paths: Vec<PathBuf>,
    pub format: FormatFlag,
    pub case: Option<CaseFlag>,
    pub strip_cjk: bool,
    pub strip_suffix: bool,
    pub fill_mode: FillFlag,
    pub placeholder: Option<String>,
    /// When `true`, commit the would-rename candidates through
    /// `core::rename::Rename::apply` after emitting the preview report.
    pub apply: bool,
}

pub fn run(cwd: &Path, args: &CleanArgs) -> Result<i32> {
    let root = ProjectRoot::discover(cwd).with_context(|| {
        format!(
            "could not find a Progest project at or above `{}`",
            cwd.display()
        )
    })?;

    let cfg = load_cfg(&root, args)?;
    let fill_mode = build_fill_mode(args);

    let entries = collect_entries(&root, &args.paths)?;
    let report = build_report(&root, &cfg, &fill_mode, &entries);

    match args.format {
        FormatFlag::Text => emit_text(&report),
        FormatFlag::Json => emit_json(&report)?,
    }

    if args.apply {
        return commit(&root, &cfg, args, &entries);
    }
    Ok(0)
}

fn load_cfg(root: &ProjectRoot, args: &CleanArgs) -> Result<CleanupConfig> {
    let raw = std::fs::read_to_string(root.project_toml())
        .with_context(|| format!("failed to read `{}`", root.project_toml().display()))?;
    let doc = ProjectDocument::from_toml_str(&raw)
        .with_context(|| format!("failed to parse `{}`", root.project_toml().display()))?;
    let (mut cfg, warns) = extract_cleanup_config(&doc.extra).with_context(|| {
        format!(
            "failed to read [cleanup] from `{}`",
            root.project_toml().display()
        )
    })?;
    for w in warns {
        eprintln!("warning: {w:?}");
    }
    if let Some(case) = &args.case {
        cfg.convert_case = case.to_style();
    }
    if args.strip_cjk {
        cfg.remove_cjk = true;
    }
    if args.strip_suffix {
        cfg.remove_copy_suffix = true;
    }
    Ok(cfg)
}

fn build_fill_mode(args: &CleanArgs) -> FillMode {
    match args.fill_mode {
        FillFlag::Placeholder => {
            FillMode::Placeholder(args.placeholder.clone().unwrap_or_else(|| "_".to_string()))
        }
        // `clean` is preview-only and runs unattended — treat
        // `--fill-mode prompt` the same as `skip` so unresolved
        // candidates surface as "skipped" instead of blocking on
        // stdin. Use `progest rename` for interactive resolution.
        FillFlag::Skip | FillFlag::Prompt => FillMode::Skip,
    }
}

fn collect_entries(root: &ProjectRoot, paths: &[PathBuf]) -> Result<Vec<ScanEntry>> {
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

// --- Report ----------------------------------------------------------------

#[derive(Debug, Serialize)]
struct Report<'a> {
    candidates: Vec<Entry<'a>>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct Entry<'a> {
    path: &'a str,
    original: &'a str,
    sentinel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    holes: Vec<HoleOut>,
    changed: bool,
}

#[derive(Debug, Serialize)]
struct HoleOut {
    seq: usize,
    origin: String,
    kind: &'static str,
}

#[derive(Debug, Serialize, Default)]
struct Summary {
    scanned: usize,
    would_rename: usize,
    skipped_due_to_holes: usize,
    unchanged: usize,
}

fn build_report<'a>(
    _root: &ProjectRoot,
    cfg: &CleanupConfig,
    fill: &FillMode,
    entries: &'a [ScanEntry],
) -> Report<'a> {
    let mut candidates: Vec<Entry<'a>> = Vec::new();
    let mut summary = Summary {
        scanned: entries.len(),
        ..Summary::default()
    };

    for e in entries {
        let original = e.path.file_name().unwrap_or("");
        let cand = clean_basename(original, cfg, BUILTIN_COMPOUND_EXTS);
        let sentinel = cand.to_sentinel_string();
        let holes = hole_vec(&cand);
        let resolved = resolve(&cand, fill).ok().map(|r| r.basename);

        let changed = resolved
            .as_deref()
            .map_or_else(|| sentinel != original, |r| r != original);

        let skipped_reason = if resolved.is_none() && !holes.is_empty() {
            Some(format!(
                "unresolved hole(s) under fill-mode {}",
                fill_label(fill)
            ))
        } else {
            None
        };

        if skipped_reason.is_some() {
            summary.skipped_due_to_holes += 1;
        } else if changed {
            summary.would_rename += 1;
        } else {
            summary.unchanged += 1;
        }

        candidates.push(Entry {
            path: e.path.as_str(),
            original,
            sentinel,
            resolved,
            skipped_reason,
            holes,
            changed,
        });
    }

    Report {
        candidates,
        summary,
    }
}

fn hole_vec(cand: &NameCandidate) -> Vec<HoleOut> {
    let mut out = Vec::new();
    let mut seq = 0;
    for seg in &cand.segments {
        if let Segment::Hole(h) = seg {
            seq += 1;
            out.push(HoleOut {
                seq,
                origin: hole_origin(h).to_owned(),
                kind: "cjk",
            });
        }
    }
    out
}

fn hole_origin(h: &Hole) -> &str {
    &h.origin
}

fn fill_label(mode: &FillMode) -> &'static str {
    match mode {
        FillMode::Skip => "skip",
        FillMode::Placeholder(_) => "placeholder",
        FillMode::Prompt => "prompt",
    }
}

// --- Emit ------------------------------------------------------------------

fn emit_text(report: &Report<'_>) {
    let mut any = false;
    for c in &report.candidates {
        if !c.changed && c.skipped_reason.is_none() {
            continue;
        }
        any = true;
        println!("{}", c.path);
        if let Some(r) = &c.resolved {
            println!("  → {r}");
        } else {
            println!("  → {}  [skipped]", c.sentinel);
            if let Some(reason) = &c.skipped_reason {
                println!("     {reason}");
            }
        }
        if !c.holes.is_empty() {
            let mut s = String::new();
            for h in &c.holes {
                if !s.is_empty() {
                    s.push_str(", ");
                }
                let _ = write!(s, "⟨{}-{}⟩={}", h.kind, h.seq, h.origin);
            }
            println!("     holes: {s}");
        }
    }
    if !any {
        println!("(nothing to clean)");
    }
    println!(
        "\nSummary: {} scanned, {} would rename, {} skipped (holes), {} unchanged",
        report.summary.scanned,
        report.summary.would_rename,
        report.summary.skipped_due_to_holes,
        report.summary.unchanged,
    );
}

fn emit_json(report: &Report<'_>) -> Result<()> {
    let s = serde_json::to_string_pretty(report).context("serializing clean report")?;
    println!("{s}");
    Ok(())
}

// --- Apply -----------------------------------------------------------------

/// Commit the changed-and-resolvable candidates from the report. Runs
/// only the rename half (FS + .meta + index + history); the preview
/// report is emitted separately above so the user can audit before
/// re-running with `--apply`.
fn commit(
    root: &ProjectRoot,
    cfg: &CleanupConfig,
    args: &CleanArgs,
    entries: &[ScanEntry],
) -> Result<i32> {
    // Build one RenameRequest per scanned entry; let the preview
    // builder handle conflict detection (Identity, TargetExists,
    // DuplicateTarget, Unresolved). Filtering "would change" up
    // front would short-circuit Identity detection but also miss
    // surprising collisions, so we let the preview catch all four.
    let requests: Vec<RenameRequest> = entries
        .iter()
        .map(|e| {
            let original = e.path.file_name().unwrap_or("");
            let cand = clean_basename(original, cfg, BUILTIN_COMPOUND_EXTS);
            RenameRequest::new(e.path.clone(), cand)
        })
        .collect();

    let fs = StdFileSystem::new(root.root().to_path_buf());
    let preview = match (&args.fill_mode, args.placeholder.clone()) {
        (FillFlag::Skip, _) => build_preview(&requests, &FillMode::Skip, &fs)?,
        (FillFlag::Placeholder, p) => build_preview(
            &requests,
            &FillMode::Placeholder(p.unwrap_or_else(|| "_".into())),
            &fs,
        )?,
        (FillFlag::Prompt, _) => {
            let prompter = StdinHolePrompter::from_stdio();
            build_preview_with_prompter(&requests, &prompter, &fs)?
        }
    };

    // Drop ops with `from == to` before checking cleanness:
    // - Identity (no rename needed) — the dominant case
    // - Unresolved (`to` falls back to `from`) — already surfaced in
    //   the preview report; the user saw them and chose to apply
    //   anyway, so skip them silently rather than blocking apply
    let actionable: Vec<_> = preview
        .ops
        .into_iter()
        .filter(|op| op.from != op.to)
        .collect();
    let preview = progest_core::rename::RenamePreview { ops: actionable };

    if preview.ops.is_empty() {
        println!("\n(nothing to apply)");
        return Ok(0);
    }
    if !preview.is_clean() {
        eprintln!(
            "\nrefusing to apply: {} op(s) carry conflicts",
            preview.conflicting_ops().count()
        );
        return Ok(1);
    }

    let index = SqliteIndex::open(&root.index_db())
        .with_context(|| format!("opening index `{}`", root.index_db().display()))?;
    let history = HistoryStore::open(&root.history_db())
        .with_context(|| format!("opening history `{}`", root.history_db().display()))?;
    let driver = Rename::new(&fs, &index, &history);
    let outcome = driver.apply(&preview).context("applying rename batch")?;

    emit_apply_summary(&outcome);
    Ok(0)
}

fn emit_apply_summary(outcome: &ApplyOutcome) {
    println!("\napplied {} op(s)", outcome.applied.len());
    if let Some(g) = &outcome.group_id {
        println!("  group: {g}");
    }
    println!("  batch: {}", outcome.batch_id);
    if !outcome.index_warnings.is_empty() {
        println!("  index warnings: {}", outcome.index_warnings.len());
    }
    if !outcome.history_warnings.is_empty() {
        println!("  history warnings: {}", outcome.history_warnings.len());
    }
}
