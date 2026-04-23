//! `progest rename` — preview / apply mechanical name renames.
//!
//! Two input modes:
//!
//! - **Path-based** (`progest rename PATH...`): walks the project,
//!   filters to the requested paths, runs the `core::naming` cleanup
//!   pipeline on each basename, and turns the candidates into a
//!   [`RenamePreview`].
//! - **Stdin-based** (`progest rename --from-stdin`): reads a
//!   pre-built `RenameOp[]` JSON array on stdin (the wire shape
//!   produced by an earlier `--preview --format=json` run) and
//!   feeds it straight into apply. Lets users review / hand-edit the
//!   plan between preview and commit.
//!
//! Modes:
//!
//! - **`--preview`** (default): print the plan, no disk mutation.
//! - **`--apply`**: run [`core::rename::Rename::apply`] on the plan,
//!   then print the outcome.
//!
//! `--fill-mode prompt` wires in [`crate::prompter::StdinHolePrompter`]
//! so a CJK hole can be filled interactively at preview time.
//!
//! Exit codes:
//!
//! - `0` — preview ran or apply succeeded.
//! - `1` — apply was requested and the plan had unresolved conflicts,
//!   or the apply itself errored.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use progest_core::fs::{EntryKind, IgnoreRules, ProjectPath, ScanEntry, Scanner, StdFileSystem};
use progest_core::history::SqliteStore as HistoryStore;
use progest_core::index::SqliteIndex;
use progest_core::naming::{
    CaseStyle, CleanupConfig, FillMode, NameCandidate, clean_basename, extract_cleanup_config,
};
use progest_core::project::{ProjectDocument, ProjectRoot};
use progest_core::rename::{
    ApplyOutcome, Rename, RenameOp, RenamePreview, RenameRequest, build_preview,
    build_preview_with_prompter, requests_from_sequence,
};
use progest_core::rules::BUILTIN_COMPOUND_EXTS;
use progest_core::sequence::detect_sequences;
use serde::Serialize;

use crate::commands::clean::{CaseFlag, FillFlag, FormatFlag};
use crate::prompter::StdinHolePrompter;

/// CLI arguments accepted by `progest rename`.
pub struct RenameArgs {
    pub paths: Vec<PathBuf>,
    pub format: FormatFlag,
    pub mode: RenameMode,
    pub from_stdin: bool,
    pub case: Option<CaseFlag>,
    pub strip_cjk: bool,
    pub strip_suffix: bool,
    pub fill_mode: FillFlag,
    pub placeholder: Option<String>,
    /// When set, run sequence detection on PATH... and rename each
    /// detected sequence by replacing the stem prefix with this
    /// value (numeric index, padding, separator, and extension are
    /// preserved). Singletons (paths that aren't part of a detected
    /// sequence of ≥2 members) are skipped with a warning.
    pub sequence_stem: Option<String>,
}

/// Whether the command should preview only or commit to disk.
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum RenameMode {
    Preview,
    Apply,
}

pub fn run(cwd: &Path, args: &RenameArgs) -> Result<i32> {
    let root = ProjectRoot::discover(cwd).with_context(|| {
        format!(
            "could not find a Progest project at or above `{}`",
            cwd.display()
        )
    })?;

    let preview = if args.from_stdin {
        if !args.paths.is_empty() {
            bail!("--from-stdin and PATH... are mutually exclusive");
        }
        if args.sequence_stem.is_some() {
            bail!("--from-stdin and --sequence-stem are mutually exclusive");
        }
        load_preview_from_stdin().context("reading rename plan from stdin")?
    } else if let Some(new_stem) = args.sequence_stem.as_deref() {
        build_preview_from_sequences(&root, args, new_stem)?
    } else {
        build_preview_from_paths(&root, args)?
    };

    match args.mode {
        RenameMode::Preview => {
            emit_preview(&preview, &args.format)?;
            Ok(0)
        }
        RenameMode::Apply => apply_preview(&root, &preview, &args.format),
    }
}

// --- Path mode -------------------------------------------------------------

fn build_preview_from_paths(root: &ProjectRoot, args: &RenameArgs) -> Result<RenamePreview> {
    let cfg = load_cleanup_config(root, args)?;
    let entries = collect_entries(root, &args.paths)?;

    let requests: Vec<RenameRequest> = entries
        .iter()
        .map(|e| {
            let original = e.path.file_name().unwrap_or("");
            let cand: NameCandidate = clean_basename(original, &cfg, BUILTIN_COMPOUND_EXTS);
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
    Ok(preview)
}

fn load_cleanup_config(root: &ProjectRoot, args: &RenameArgs) -> Result<CleanupConfig> {
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
        cfg.convert_case = case_flag_to_style(case);
    }
    if args.strip_cjk {
        cfg.remove_cjk = true;
    }
    if args.strip_suffix {
        cfg.remove_copy_suffix = true;
    }
    Ok(cfg)
}

fn case_flag_to_style(flag: &CaseFlag) -> CaseStyle {
    match flag {
        CaseFlag::Off => CaseStyle::Off,
        CaseFlag::Snake => CaseStyle::Snake,
        CaseFlag::Kebab => CaseStyle::Kebab,
        CaseFlag::Camel => CaseStyle::Camel,
        CaseFlag::Pascal => CaseStyle::Pascal,
        CaseFlag::Lower => CaseStyle::Lower,
        CaseFlag::Upper => CaseStyle::Upper,
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

// --- Sequence mode ---------------------------------------------------------

fn build_preview_from_sequences(
    root: &ProjectRoot,
    args: &RenameArgs,
    new_stem: &str,
) -> Result<RenamePreview> {
    let entries = collect_entries(root, &args.paths)?;
    let paths: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
    let detection = detect_sequences(&paths);

    if detection.sequences.is_empty() {
        bail!(
            "no sequences detected in {} input path(s); --sequence-stem requires at least one \
             group of ≥2 numbered files sharing parent + stem + separator + padding + extension",
            paths.len()
        );
    }
    if !detection.singletons.is_empty() {
        eprintln!(
            "skipping {} singleton path(s) (not part of any detected sequence)",
            detection.singletons.len()
        );
    }

    let requests: Vec<RenameRequest> = detection
        .sequences
        .iter()
        .flat_map(|seq| requests_from_sequence(seq, new_stem))
        .collect();

    let fs = StdFileSystem::new(root.root().to_path_buf());
    // Sequence renames don't traverse the cleanup pipeline, so the
    // candidates are pure literals — `FillMode::Skip` would never
    // surface a hole. Use it as the safe default.
    let preview = build_preview(&requests, &progest_core::naming::FillMode::Skip, &fs)?;
    Ok(preview)
}

// --- Stdin mode ------------------------------------------------------------

fn load_preview_from_stdin() -> Result<RenamePreview> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("failed to read stdin")?;
    // Accept either a flat `RenameOp[]` array (the most common pipe
    // shape) or a `{"ops": [...]}` object so callers can round-trip
    // a previously serialized preview verbatim.
    if let Ok(preview) = serde_json::from_str::<RenamePreview>(&buf) {
        return Ok(preview);
    }
    let ops: Vec<RenameOp> =
        serde_json::from_str(&buf).context("stdin must be a RenameOp[] or RenamePreview JSON")?;
    Ok(RenamePreview { ops })
}

// --- Preview emit ----------------------------------------------------------

#[derive(Debug, Serialize)]
struct PreviewReport<'a> {
    ops: &'a [RenameOp],
    summary: PreviewSummary,
}

#[derive(Debug, Serialize)]
struct PreviewSummary {
    total: usize,
    clean: usize,
    conflicting: usize,
}

fn emit_preview(preview: &RenamePreview, format: &FormatFlag) -> Result<()> {
    let summary = PreviewSummary {
        total: preview.ops.len(),
        clean: preview.clean_ops().count(),
        conflicting: preview.conflicting_ops().count(),
    };
    match format {
        FormatFlag::Text => emit_preview_text(preview, &summary),
        FormatFlag::Json => emit_preview_json(preview, &summary)?,
    }
    Ok(())
}

fn emit_preview_text(preview: &RenamePreview, summary: &PreviewSummary) {
    if preview.ops.is_empty() {
        println!("(no rename candidates)");
    }
    for op in &preview.ops {
        println!("{}", op.from);
        println!("  → {}", op.to);
        for c in &op.conflicts {
            println!("     ! {:?}: {}", c.kind, c.message);
        }
    }
    println!(
        "\nSummary: {} ops ({} clean, {} conflicting)",
        summary.total, summary.clean, summary.conflicting
    );
}

fn emit_preview_json(preview: &RenamePreview, summary: &PreviewSummary) -> Result<()> {
    let report = PreviewReport {
        ops: &preview.ops,
        summary: PreviewSummary {
            total: summary.total,
            clean: summary.clean,
            conflicting: summary.conflicting,
        },
    };
    let s = serde_json::to_string_pretty(&report).context("serializing preview report")?;
    println!("{s}");
    Ok(())
}

// --- Apply ------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ApplyReport<'a> {
    batch_id: &'a str,
    group_id: Option<&'a str>,
    applied: usize,
    index_warnings: usize,
    history_warnings: usize,
    paths: Vec<AppliedPath<'a>>,
}

#[derive(Debug, Serialize)]
struct AppliedPath<'a> {
    from: &'a ProjectPath,
    to: &'a ProjectPath,
}

fn apply_preview(root: &ProjectRoot, preview: &RenamePreview, format: &FormatFlag) -> Result<i32> {
    if !preview.is_clean() {
        emit_preview(preview, format)?;
        eprintln!(
            "\nrefusing to apply: {} op(s) carry conflicts",
            preview.conflicting_ops().count()
        );
        return Ok(1);
    }

    let fs = StdFileSystem::new(root.root().to_path_buf());
    let index = SqliteIndex::open(&root.index_db())
        .with_context(|| format!("opening index `{}`", root.index_db().display()))?;
    let history = HistoryStore::open(&root.history_db())
        .with_context(|| format!("opening history `{}`", root.history_db().display()))?;
    let driver = Rename::new(&fs, &index, &history);
    let outcome = driver.apply(preview).context("applying rename batch")?;

    emit_apply(&outcome, format)?;
    Ok(0)
}

fn emit_apply(outcome: &ApplyOutcome, format: &FormatFlag) -> Result<()> {
    match format {
        FormatFlag::Text => emit_apply_text(outcome),
        FormatFlag::Json => emit_apply_json(outcome)?,
    }
    Ok(())
}

fn emit_apply_text(outcome: &ApplyOutcome) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "applied {} op(s)", outcome.applied.len());
    if let Some(g) = &outcome.group_id {
        let _ = writeln!(out, "  group: {g}");
    }
    let _ = writeln!(out, "  batch: {}", outcome.batch_id);
    for op in &outcome.applied {
        let _ = writeln!(out, "  {} → {}", op.from, op.to);
    }
    if !outcome.index_warnings.is_empty() {
        let _ = writeln!(out, "\nindex warnings ({}):", outcome.index_warnings.len());
        for w in &outcome.index_warnings {
            let _ = writeln!(out, "  {} → {}: {}", w.from, w.to, w.message);
        }
    }
    if !outcome.history_warnings.is_empty() {
        let _ = writeln!(
            out,
            "\nhistory warnings ({}):",
            outcome.history_warnings.len()
        );
        for w in &outcome.history_warnings {
            let _ = writeln!(out, "  {} → {}: {}", w.from, w.to, w.message);
        }
    }
}

fn emit_apply_json(outcome: &ApplyOutcome) -> Result<()> {
    let report = ApplyReport {
        batch_id: &outcome.batch_id,
        group_id: outcome.group_id.as_deref(),
        applied: outcome.applied.len(),
        index_warnings: outcome.index_warnings.len(),
        history_warnings: outcome.history_warnings.len(),
        paths: outcome
            .applied
            .iter()
            .map(|op| AppliedPath {
                from: &op.from,
                to: &op.to,
            })
            .collect(),
    };
    let s = serde_json::to_string_pretty(&report).context("serializing apply report")?;
    println!("{s}");
    Ok(())
}
