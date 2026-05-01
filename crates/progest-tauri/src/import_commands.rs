//! IPC commands for importing external files into the project.
//!
//! `import_ranking` walks project dirmeta to find the best destination
//! directories for a given set of source files.  `import_preview`
//! builds a conflict-checked preview and `import_apply` commits the
//! import atomically.

use std::path::{Path, PathBuf};

use progest_core::accepts::{
    AliasCatalog, EffectiveAccepts, compute_effective_accepts, extract_accepts, normalize_ext,
};
use progest_core::fs::ProjectPath;
use progest_core::history::SqliteStore as HistoryStore;
use progest_core::import::{
    self, Import, ImportMode, ImportRequest, build_preview, rank_destinations,
};
use progest_core::index::Index;
use progest_core::meta::{StdMetaStore, load_dirmeta};
use progest_core::thumbnail::{self, ThumbnailCache};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::commands::{load_alias_catalog_for_ctx, no_project_error};
use crate::progress::ProgressEvent;
use crate::state::{AppState, ProjectContext};

// ------------------------------------------------------------------ wire types

#[derive(Debug, Clone, Serialize)]
pub struct SuggestedDestinationWire {
    pub path: String,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportRankingResponse {
    pub suggestions: Vec<SuggestedDestinationWire>,
    pub all_dirs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportRequestWire {
    pub source: String,
    pub dest: String,
    #[serde(default)]
    pub mode: String,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportOpWire {
    pub source: String,
    pub dest: String,
    pub mode: String,
    pub group_id: Option<String>,
    pub conflicts: Vec<ImportConflictWire>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportConflictWire {
    DestExists {
        existing_path: String,
    },
    SourceMissing {
        reason: String,
    },
    SourceIsProject {
        project_path: String,
    },
    PlacementMismatch {
        expected_exts: Vec<String>,
        suggestion: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreviewWire {
    pub ops: Vec<ImportOpWire>,
    pub clean: bool,
    pub conflict_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedFileWire {
    pub source: String,
    pub dest: String,
    pub file_id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportWarningWire {
    pub kind: String,
    pub dest: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportOutcomeWire {
    pub batch_id: String,
    pub group_id: Option<String>,
    pub imported: Vec<ImportedFileWire>,
    pub warnings: Vec<ImportWarningWire>,
}

// ------------------------------------------------------------------- commands

/// Given a list of source file paths, rank project directories by how
/// well they accept each file's extension.  The frontend shows the
/// top suggestions in the import modal.
///
/// Async so the directory walk doesn't block the UI thread.
#[tauri::command]
pub async fn import_ranking(
    sources: Vec<String>,
    app: AppHandle,
) -> Result<ImportRankingResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let exts: Vec<String> = sources
            .iter()
            .filter_map(|s| {
                Path::new(s)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase)
            })
            .collect();

        let all_dirs = collect_all_dirs(ctx);

        if exts.is_empty() {
            return Ok(ImportRankingResponse {
                suggestions: Vec::new(),
                all_dirs,
            });
        }

        let catalog = load_alias_catalog_for_ctx(ctx);
        let dirs = collect_dirmeta_effective_accepts(ctx, &catalog);

        let ext = normalize_ext(&exts[0]);
        let ranked = rank_destinations(&dirs, &ext);

        Ok(ImportRankingResponse {
            suggestions: ranked
                .into_iter()
                .map(|s| SuggestedDestinationWire {
                    path: s.path.as_str().to_owned(),
                    score: s.score,
                })
                .collect(),
            all_dirs,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Build a conflict-checked preview without mutating the filesystem.
#[tauri::command]
pub async fn import_preview(
    requests: Vec<ImportRequestWire>,
    app: AppHandle,
) -> Result<ImportPreviewWire, String> {
    let reqs = wire_to_requests(&requests)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;
        let preview = build_preview(&reqs, &ctx.fs, ctx.root.root());
        Ok(preview_to_wire(&preview))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Apply a clean import atomically.  Fails if any op carries
/// conflicts — the frontend should resolve them before calling.
///
/// Async + `spawn_blocking` so file copies and thumbnail generation
/// don't freeze the UI.
#[tauri::command]
pub async fn import_apply(
    requests: Vec<ImportRequestWire>,
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    app: AppHandle,
) -> Result<ImportOutcomeWire, String> {
    let reqs = wire_to_requests(&requests)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let preview = build_preview(&reqs, &ctx.fs, ctx.root.root());

        if !preview.is_clean() {
            return Err(format!(
                "cannot apply: {} conflict(s) remain",
                preview.conflicting_ops().count()
            ));
        }

        let meta_store = StdMetaStore::new(ctx.fs.clone());
        let history =
            HistoryStore::open(&ctx.root.history_db()).map_err(|e| format!("open history: {e}"))?;

        let driver = Import::new(&ctx.fs, &meta_store, &ctx.index, &history, ctx.root.root());
        let outcome = driver
            .apply_with_progress(&preview, &|current, total, msg| {
                let _ = on_progress.send(ProgressEvent {
                    current,
                    total,
                    message: msg.to_string(),
                });
            })
            .map_err(|e| format!("apply: {e}"))?;

        let cache = ThumbnailCache::new(
            ctx.root.root().join(".progest/thumbs"),
            thumbnail::DEFAULT_CACHE_MAX_BYTES,
        );
        let thumb_requests: Vec<thumbnail::ThumbnailRequest> = outcome
            .imported
            .iter()
            .filter_map(|f| {
                let row = ctx.index.get_file(&f.file_id).ok()??;
                let abs_path = ctx.root.root().join(row.path.as_str());
                Some(thumbnail::ThumbnailRequest {
                    path: row.path,
                    abs_path,
                    file_id: row.file_id,
                    fingerprint: row.fingerprint,
                    size: thumbnail::DEFAULT_MAX_DIM,
                })
            })
            .collect();

        if !thumb_requests.is_empty() {
            let _ = on_progress.send(ProgressEvent {
                current: 0,
                total: 0,
                message: "Generating thumbnails\u{2026}".to_string(),
            });
            let _ = thumbnail::generate_batch(&thumb_requests, &cache, false);
        }

        Ok(outcome_to_wire(&outcome))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ------------------------------------------------------------------- helpers

fn wire_to_requests(wires: &[ImportRequestWire]) -> Result<Vec<ImportRequest>, String> {
    wires
        .iter()
        .map(|w| {
            let source = PathBuf::from(&w.source);
            let dest =
                ProjectPath::new(&w.dest).map_err(|e| format!("invalid dest `{}`: {e}", w.dest))?;
            let mode = match w.mode.as_str() {
                "move" => ImportMode::Move,
                _ => ImportMode::Copy,
            };
            Ok(ImportRequest {
                source,
                dest,
                mode,
                group_id: w.group_id.clone(),
            })
        })
        .collect()
}

fn preview_to_wire(preview: &import::ImportPreview) -> ImportPreviewWire {
    ImportPreviewWire {
        conflict_count: preview.conflicting_ops().count(),
        clean: preview.is_clean(),
        ops: preview.ops.iter().map(op_to_wire).collect(),
    }
}

fn op_to_wire(op: &import::ImportOp) -> ImportOpWire {
    ImportOpWire {
        source: op.source.clone(),
        dest: op.dest.as_str().to_owned(),
        mode: match op.mode {
            ImportMode::Copy => "copy",
            ImportMode::Move => "move",
        }
        .to_owned(),
        group_id: op.group_id.clone(),
        conflicts: op.conflicts.iter().map(conflict_to_wire).collect(),
    }
}

fn conflict_to_wire(c: &import::ImportConflict) -> ImportConflictWire {
    match c {
        import::ImportConflict::DestExists { existing_path } => ImportConflictWire::DestExists {
            existing_path: existing_path.as_str().to_owned(),
        },
        import::ImportConflict::SourceMissing { reason } => ImportConflictWire::SourceMissing {
            reason: reason.clone(),
        },
        import::ImportConflict::SourceIsProject { project_path } => {
            ImportConflictWire::SourceIsProject {
                project_path: project_path.as_str().to_owned(),
            }
        }
        import::ImportConflict::PlacementMismatch {
            expected_exts,
            suggestion,
        } => ImportConflictWire::PlacementMismatch {
            expected_exts: expected_exts.clone(),
            suggestion: suggestion.as_ref().map(|s| s.as_str().to_owned()),
        },
    }
}

fn outcome_to_wire(outcome: &import::ImportOutcome) -> ImportOutcomeWire {
    ImportOutcomeWire {
        batch_id: outcome.batch_id.clone(),
        group_id: outcome.group_id.clone(),
        imported: outcome
            .imported
            .iter()
            .map(|f| ImportedFileWire {
                source: f.source.clone(),
                dest: f.dest.as_str().to_owned(),
                file_id: f.file_id.to_string(),
                mode: match f.mode {
                    ImportMode::Copy => "copy",
                    ImportMode::Move => "move",
                }
                .to_owned(),
            })
            .collect(),
        warnings: outcome
            .warnings
            .iter()
            .map(|w| match w {
                import::ImportWarning::MetaCreate { dest, message } => ImportWarningWire {
                    kind: "meta_create".into(),
                    dest: dest.as_str().to_owned(),
                    message: message.clone(),
                },
                import::ImportWarning::IndexInsert { dest, message } => ImportWarningWire {
                    kind: "index_insert".into(),
                    dest: dest.as_str().to_owned(),
                    message: message.clone(),
                },
                import::ImportWarning::HistoryAppend { dest, message } => ImportWarningWire {
                    kind: "history_append".into(),
                    dest: dest.as_str().to_owned(),
                    message: message.clone(),
                },
                import::ImportWarning::FingerprintFailed { dest, message } => ImportWarningWire {
                    kind: "fingerprint_failed".into(),
                    dest: dest.as_str().to_owned(),
                    message: message.clone(),
                },
            })
            .collect(),
    }
}

/// Walk the entire project tree and collect `(dir_path,
/// effective_accepts)` for every directory that carries an `[accepts]`
/// block (own or inherited).  Used by `import_ranking` to feed
/// `rank_destinations`.
fn collect_dirmeta_effective_accepts(
    ctx: &ProjectContext,
    catalog: &AliasCatalog,
) -> Vec<(ProjectPath, EffectiveAccepts)> {
    let mut result: Vec<(ProjectPath, EffectiveAccepts)> = Vec::new();

    let mut dirs_to_visit: Vec<(ProjectPath, PathBuf)> =
        vec![(ProjectPath::root(), ctx.root.root().to_path_buf())];

    while let Some((rel, abs)) = dirs_to_visit.pop() {
        if let Some(eff) = compute_effective_for_dir(ctx, &rel, catalog) {
            result.push((rel.clone(), eff));
        }

        let Ok(entries) = std::fs::read_dir(&abs) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let child_rel = if rel.is_root() {
                match ProjectPath::new(&name) {
                    Ok(p) => p,
                    Err(_) => continue,
                }
            } else {
                match rel.join(&name) {
                    Ok(p) => p,
                    Err(_) => continue,
                }
            };
            dirs_to_visit.push((child_rel, entry.path()));
        }
    }

    result
}

/// Compute `effective_accepts` for a single directory by loading its
/// dirmeta + ancestor chain, mirroring the `accepts_commands` pattern.
fn compute_effective_for_dir(
    ctx: &ProjectContext,
    dir: &ProjectPath,
    catalog: &AliasCatalog,
) -> Option<EffectiveAccepts> {
    let own_doc = load_dirmeta(&ctx.fs, dir).ok()?;
    let own = own_doc.and_then(|doc| {
        extract_accepts(&doc.extra)
            .ok()
            .flatten()
            .map(|e| e.accepts)
    });

    let mut chain_raws = Vec::new();
    let mut cursor = dir.parent();
    while let Some(ancestor) = cursor {
        if let Ok(Some(doc)) = load_dirmeta(&ctx.fs, &ancestor)
            && let Ok(Some(extraction)) = extract_accepts(&doc.extra)
        {
            chain_raws.push(extraction.accepts);
        }
        cursor = ancestor.parent();
    }

    let chain_refs: Vec<_> = chain_raws.iter().collect();
    compute_effective_accepts(own.as_ref(), &chain_refs, catalog).ok()?
}

/// Recursively collect all directory paths in the project (sorted).
fn collect_all_dirs(ctx: &ProjectContext) -> Vec<String> {
    let mut result = Vec::new();
    let mut stack: Vec<(String, PathBuf)> = vec![(String::new(), ctx.root.root().to_path_buf())];

    while let Some((rel, abs)) = stack.pop() {
        if !rel.is_empty() {
            result.push(rel.clone());
        }
        let Ok(entries) = std::fs::read_dir(&abs) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            stack.push((child, entry.path()));
        }
    }

    result.sort();
    result
}
