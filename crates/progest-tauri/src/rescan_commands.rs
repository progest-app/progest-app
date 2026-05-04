//! IPC command for full project rescan (reconcile + lint + thumbnail).

use progest_core::accepts::{AliasCatalog, SchemaLoad, load_alias_catalog};
use progest_core::fs::ProjectPath;
use progest_core::index::Index;
use progest_core::lint::{LintOptions, lint_paths_with_progress, write_to_index};
use progest_core::meta::StdMetaStore;
use progest_core::naming::{CleanupConfig, extract_cleanup_config};
use progest_core::project::ProjectDocument;
use progest_core::reconcile::Reconciler;
use progest_core::rules::{
    BUILTIN_COMPOUND_EXTS, CompiledRuleSet, RuleSetLayer, RuleSource, compile_ruleset,
    load_document,
};
use progest_core::thumbnail::{
    DEFAULT_CACHE_MAX_BYTES, DEFAULT_MAX_DIM, ThumbnailCache, generate_batch_with_progress,
    requests_from_index,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::no_project_error;
use crate::progress::ProgressEvent;
use crate::state::{AppState, ProjectContext};

#[derive(Debug, Clone, Serialize)]
pub struct RescanResponse {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub orphan_metas: usize,
    pub lint_naming: usize,
    pub lint_placement: usize,
    pub lint_sequence: usize,
    pub thumb_generated: usize,
    pub thumb_cached: usize,
}

#[tauri::command]
pub async fn rescan_project(
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    app: AppHandle,
) -> Result<RescanResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let meta = StdMetaStore::new(ctx.fs.clone());

        // Phase 1: reconcile
        let _ = on_progress.send(ProgressEvent {
            current: 0,
            total: 0,
            message: "Scanning files\u{2026}".to_string(),
        });
        let hide_meta = load_meta_hidden(ctx);
        let reconciler = Reconciler::new(&ctx.fs, &meta, &ctx.index).with_hide_meta(hide_meta);
        let report = reconciler
            .full_scan_with_progress(&|current, total, msg| {
                let _ = on_progress.send(ProgressEvent {
                    current,
                    total,
                    message: msg.to_string(),
                });
            })
            .map_err(|e| format!("reconcile: {e}"))?;

        // Phase 2: lint
        let _ = on_progress.send(ProgressEvent {
            current: 0,
            total: 0,
            message: "Running lint\u{2026}".to_string(),
        });
        let rows = ctx
            .index
            .list_files()
            .map_err(|e| format!("list indexed files: {e}"))?;
        let paths: Vec<ProjectPath> = rows.iter().map(|r| r.path.clone()).collect();

        let ruleset = load_ruleset(ctx)?;
        let catalog = load_alias_catalog_for_ctx(ctx);
        let cleanup = load_cleanup(ctx)?;
        let opts = LintOptions {
            ruleset: &ruleset,
            alias_catalog: &catalog,
            compound_exts: BUILTIN_COMPOUND_EXTS,
            cleanup_cfg: &cleanup,
            explain: false,
        };
        let lint_report = lint_paths_with_progress(
            meta.filesystem(),
            &meta,
            &paths,
            &opts,
            &|current, total, msg| {
                let _ = on_progress.send(ProgressEvent {
                    current,
                    total,
                    message: msg.to_string(),
                });
            },
        )
        .map_err(|e| format!("lint: {e}"))?;

        let visited: Vec<_> = rows.into_iter().map(|r| r.file_id).collect();
        write_to_index(&ctx.index, &visited, &lint_report)
            .map_err(|e| format!("write violations: {e}"))?;

        // Phase 3: thumbnails
        let _ = on_progress.send(ProgressEvent {
            current: 0,
            total: 0,
            message: "Generating thumbnails\u{2026}".to_string(),
        });
        let cache = ThumbnailCache::new(ctx.root.thumbs_dir(), DEFAULT_CACHE_MAX_BYTES);
        let reqs = requests_from_index(&ctx.index, ctx.root.root(), DEFAULT_MAX_DIM);
        let thumb_report =
            generate_batch_with_progress(&reqs, &cache, false, &|current, total, msg| {
                let _ = on_progress.send(ProgressEvent {
                    current,
                    total,
                    message: msg.to_string(),
                });
            });

        Ok(RescanResponse {
            added: report.added(),
            updated: report.updated(),
            removed: report.removed(),
            unchanged: report.unchanged(),
            orphan_metas: report.orphan_metas.len(),
            lint_naming: lint_report.naming.len(),
            lint_placement: lint_report.placement.len(),
            lint_sequence: lint_report.sequence.len(),
            thumb_generated: thumb_report.generated,
            thumb_cached: thumb_report.cached,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

fn load_ruleset(ctx: &ProjectContext) -> Result<CompiledRuleSet, String> {
    let path = ctx.root.rules_toml();
    if !path.exists() {
        return compile_ruleset(vec![]).map_err(|e| format!("compile empty ruleset: {e}"));
    }
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read `{}`: {e}", path.display()))?;
    let doc = load_document(&raw).map_err(|e| format!("parse rules.toml: {e}"))?;
    let layer = RuleSetLayer {
        source: RuleSource::ProjectWide,
        base_dir: ProjectPath::root(),
        rules: doc.rules,
    };
    compile_ruleset(vec![layer]).map_err(|e| format!("compile ruleset: {e}"))
}

fn load_alias_catalog_for_ctx(ctx: &ProjectContext) -> AliasCatalog {
    let path = ctx.root.schema_toml();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return AliasCatalog::builtin();
    };
    match load_alias_catalog(&text) {
        Ok(SchemaLoad { catalog, .. }) => catalog,
        Err(_) => AliasCatalog::builtin(),
    }
}

fn load_cleanup(ctx: &ProjectContext) -> Result<CleanupConfig, String> {
    let path = ctx.root.project_toml();
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read `{}`: {e}", path.display()))?;
    let doc =
        ProjectDocument::from_toml_str(&raw).map_err(|e| format!("parse project.toml: {e}"))?;
    let (cfg, _warns) =
        extract_cleanup_config(&doc.extra).map_err(|e| format!("read [cleanup]: {e}"))?;
    Ok(cfg)
}

fn load_meta_hidden(ctx: &ProjectContext) -> bool {
    let Ok(text) = std::fs::read_to_string(ctx.root.project_toml()) else {
        return true;
    };
    let Ok(doc) = ProjectDocument::from_toml_str(&text) else {
        return true;
    };
    doc.meta.hidden
}
