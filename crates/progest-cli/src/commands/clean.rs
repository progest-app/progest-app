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

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use progest_core::fs::{ProjectPath, ScanEntry, StdFileSystem};
use progest_core::naming::{
    CaseStyle, CleanupConfig, FillMode, Hole, NameCandidate, Segment, clean_basename, resolve,
};
use progest_core::project::ProjectRoot;
use progest_core::rename::{
    ApplyOutcome, Rename, RenameRequest, build_preview, build_preview_with_prompter,
};
use progest_core::rules::BUILTIN_COMPOUND_EXTS;
use progest_core::sequence::detect_sequences;
use serde::Serialize;
use uuid::Uuid;

use crate::context::{
    CleanupOverrides, discover_root, load_cleanup_config, open_history, open_index,
};
use crate::output::{OutputFormat, emit_json};
use crate::prompter::StdinHolePrompter;
use crate::walk::collect_entries;

// --- CLI flag types --------------------------------------------------------

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
    pub(crate) fn to_style(&self) -> CaseStyle {
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
    pub format: OutputFormat,
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
    let root = discover_root(cwd)?;

    let cfg = load_cfg(&root, args)?;
    let fill_mode = build_fill_mode(args);

    let entries = collect_entries(&root, &args.paths)?;
    let seq_groups = sequence_groups(&entries);
    let report = build_report(&root, &cfg, &fill_mode, &entries, &seq_groups);

    match args.format {
        OutputFormat::Text => emit_text(&report),
        OutputFormat::Json => emit_json(&report, "clean")?,
    }

    if args.apply {
        return commit(&root, &cfg, args, &entries, &seq_groups);
    }
    Ok(0)
}

/// Detect numbered sequences in the walked entries and emit a
/// per-member `seq-<uuid>` tag so the preview (JSON + text) can show
/// grouping and so `--apply` can bundle an entire sequence under one
/// undo `group_id`. `Uuid::now_v7` is freshly generated per
/// invocation — the in-memory mapping is what ties the preview to the
/// apply call in a single run, so persisting determinism across runs
/// would only cost us (history ids must not collide across batches).
fn sequence_groups(entries: &[ScanEntry]) -> HashMap<ProjectPath, String> {
    let paths: Vec<ProjectPath> = entries.iter().map(|e| e.path.clone()).collect();
    let detection = detect_sequences(&paths);
    let mut out = HashMap::new();
    for seq in &detection.sequences {
        let id = format!("seq-{}", Uuid::now_v7().simple());
        for m in &seq.members {
            out.insert(m.path.clone(), id.clone());
        }
    }
    out
}

fn load_cfg(root: &ProjectRoot, args: &CleanArgs) -> Result<CleanupConfig> {
    let overrides = CleanupOverrides {
        case: args.case.as_ref().map(CaseFlag::to_style),
        force_remove_cjk: args.strip_cjk,
        force_remove_copy_suffix: args.strip_suffix,
    };
    load_cleanup_config(root, &overrides)
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
    /// `seq-<uuid>` shared with every other entry in the same detected
    /// numbered sequence. `None` for singletons. The same id flows
    /// into the apply path's history `group_id` so undo reverses the
    /// whole sequence in one step.
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_group: Option<String>,
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
    seq_groups: &HashMap<ProjectPath, String>,
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
            sequence_group: seq_groups.get(&e.path).cloned(),
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
        let seq_tag = c
            .sequence_group
            .as_deref()
            .map(|g| format!("  [{g}]"))
            .unwrap_or_default();
        println!("{}{seq_tag}", c.path);
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
    let seq_members = report
        .candidates
        .iter()
        .filter(|c| c.sequence_group.is_some())
        .count();
    println!(
        "\nSummary: {} scanned, {} would rename, {} skipped (holes), {} unchanged, {} in sequences",
        report.summary.scanned,
        report.summary.would_rename,
        report.summary.skipped_due_to_holes,
        report.summary.unchanged,
        seq_members,
    );
}

// --- Apply -----------------------------------------------------------------

/// Commit the changed-and-resolvable candidates from the report. Runs
/// only the rename half (FS + .meta + index + history); the preview
/// report is emitted separately above so the user can audit before
/// re-running with `--apply`.
///
/// Decomposed into [`build_clean_requests`] (pure: candidate
/// construction + sequence group tagging), [`resolve_preview`]
/// (fill-mode dispatch over the cleanup builder), and
/// [`drop_identity_ops`] (filter out `from == to` rows the user
/// already saw in preview). The orchestration here just glues those
/// together and runs the apply driver.
fn commit(
    root: &ProjectRoot,
    cfg: &CleanupConfig,
    args: &CleanArgs,
    entries: &[ScanEntry],
    seq_groups: &HashMap<ProjectPath, String>,
) -> Result<i32> {
    let requests = build_clean_requests(entries, cfg, seq_groups);
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let preview = resolve_preview(&requests, &args.fill_mode, args.placeholder.as_deref(), &fs)?;
    let preview = drop_identity_ops(preview);

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

    let index = open_index(root)?;
    let history = open_history(root)?;
    let driver = Rename::new(&fs, &index, &history);
    let outcome = driver.apply(&preview).context("applying rename batch")?;

    emit_apply_summary(&outcome);
    Ok(0)
}

/// Build one [`RenameRequest`] per scanned entry, threading shared
/// `seq-<uuid>` group ids through so an entire detected sequence
/// commits under a single history batch (and reverses with one undo).
///
/// The cleanup pipeline runs over every entry — including unchanged
/// ones — because letting the preview builder see them all means it
/// can detect `Identity` / `TargetExists` / `DuplicateTarget` /
/// `Unresolved` conflicts with the full picture; pre-filtering
/// "would change" here would short-circuit that.
fn build_clean_requests(
    entries: &[ScanEntry],
    cfg: &CleanupConfig,
    seq_groups: &HashMap<ProjectPath, String>,
) -> Vec<RenameRequest> {
    entries
        .iter()
        .map(|e| {
            let original = e.path.file_name().unwrap_or("");
            let cand = clean_basename(original, cfg, BUILTIN_COMPOUND_EXTS);
            let req = RenameRequest::new(e.path.clone(), cand);
            match seq_groups.get(&e.path) {
                Some(g) => req.with_group_id(g.clone()),
                None => req,
            }
        })
        .collect()
}

/// Resolve `requests` into a [`RenamePreview`] under the requested
/// fill-mode. `Prompt` wires in [`StdinHolePrompter`]; the other
/// modes go through the non-interactive builder.
fn resolve_preview(
    requests: &[RenameRequest],
    fill: &FillFlag,
    placeholder: Option<&str>,
    fs: &StdFileSystem,
) -> Result<progest_core::rename::RenamePreview> {
    let placeholder = placeholder.unwrap_or("_").to_string();
    Ok(match fill {
        FillFlag::Skip => build_preview(requests, &FillMode::Skip, fs)?,
        FillFlag::Placeholder => build_preview(requests, &FillMode::Placeholder(placeholder), fs)?,
        FillFlag::Prompt => {
            let prompter = StdinHolePrompter::from_stdio();
            build_preview_with_prompter(requests, &prompter, fs)?
        }
    })
}

/// Drop ops with `from == to` before checking cleanness:
///
/// - Identity (no rename needed) — the dominant case
/// - Unresolved (`to` falls back to `from`) — already surfaced in
///   the preview report; the user saw them and chose to apply
///   anyway, so skip them silently rather than blocking apply
fn drop_identity_ops(
    preview: progest_core::rename::RenamePreview,
) -> progest_core::rename::RenamePreview {
    let actionable: Vec<_> = preview
        .ops
        .into_iter()
        .filter(|op| op.from != op.to)
        .collect();
    progest_core::rename::RenamePreview { ops: actionable }
}

fn emit_apply_summary(outcome: &ApplyOutcome) {
    println!("\napplied {} op(s)", outcome.applied.len());
    if let Some(g) = &outcome.group_id {
        println!("  group: {g}");
    }
    println!("  batch: {}", outcome.batch_id);
    let index_warns = outcome.index_warnings().count();
    if index_warns > 0 {
        println!("  index warnings: {index_warns}");
    }
    let history_warns = outcome.history_warnings().count();
    if history_warns > 0 {
        println!("  history warnings: {history_warns}");
    }
}
