//! IPC commands that create new Progest projects from the desktop app.
//!
//! Three commands cover the flow:
//!
//! - [`project_init_preview`] — pre-flight inspection: target validation,
//!   already-initialized detection, predicted file count, artifact list.
//!   Pure read; safe to call any time the user changes the picker.
//! - [`project_init_new`] — create a new directory under `parent/name` and
//!   initialize it. Errors before touching disk if `name` is invalid.
//! - [`project_init_existing`] — initialize at an existing path. Caller is
//!   responsible for confirming the target with the user first.
//!
//! Both write commands run an initial `Reconciler::full_scan` after init
//! so the index is populated by the time the new project is attached and
//! the UI swaps to the `FlatView`. The user always lands on a "real" view —
//! never on an empty index that would only fill in after a separate scan.

use std::path::Path;

use progest_core::fs::StdFileSystem;
use progest_core::index::SqliteIndex;
use progest_core::meta::StdMetaStore;
use progest_core::project::{InitPreview, ProjectError, ProjectRoot, initialize, preview_init};
use progest_core::reconcile::Reconciler;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::progress::ProgressEvent;
use crate::recent;
use crate::state::{AppState, ProjectContext, ProjectInfo};

/// Wire shape mirroring [`InitPreview`] — the React side branches on
/// these flags to decide between confirm / open-existing / disabled UI.
#[derive(Debug, Clone, Serialize)]
pub struct InitPreviewWire {
    pub target_path: String,
    pub target_exists: bool,
    pub is_existing_project: bool,
    pub predicted_file_count: Option<u64>,
    pub artifacts: Vec<&'static str>,
    pub gitignore_exists: bool,
}

impl From<InitPreview> for InitPreviewWire {
    fn from(p: InitPreview) -> Self {
        Self {
            target_path: p.target_path.display().to_string(),
            target_exists: p.target_exists,
            is_existing_project: p.is_existing_project,
            predicted_file_count: p.predicted_file_count,
            artifacts: p.artifacts,
            gitignore_exists: p.gitignore_exists,
        }
    }
}

/// Result of a successful init — the freshly attached project plus the
/// scan stats so the UI can show "Initialized; indexed N files."
#[derive(Debug, Clone, Serialize)]
pub struct InitResultWire {
    pub project: ProjectInfo,
    pub scanned: u64,
    pub added: u64,
    pub orphan_metas: u64,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn project_init_preview(path: String) -> Result<InitPreviewWire, String> {
    let preview = preview_init(Path::new(&path)).map_err(format_project_error)?;
    Ok(preview.into())
}

#[tauri::command]
pub async fn project_init_new(
    parent: String,
    name: String,
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    app: AppHandle,
) -> Result<InitResultWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let parent_path = Path::new(&parent);
        if !parent_path.is_dir() {
            return Err(format!("parent directory does not exist: `{parent}`"));
        }
        validate_project_name(&name)?;

        let target = parent_path.join(&name);
        if target.exists() {
            return Err(format!(
                "target already exists: `{}` — pick a different name or initialize the existing directory instead",
                target.display(),
            ));
        }

        init_and_attach(&target, &name, &on_progress, &app)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn project_init_existing(
    path: String,
    name: Option<String>,
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    app: AppHandle,
) -> Result<InitResultWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = Path::new(&path);
        if !target.is_dir() {
            return Err(format!("not a directory: `{path}`"));
        }
        let display_name = name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| target_basename(target));
        validate_project_name(&display_name)?;

        init_and_attach(target, &display_name, &on_progress, &app)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

fn init_and_attach(
    target: &Path,
    name: &str,
    on_progress: &tauri::ipc::Channel<ProgressEvent>,
    app: &AppHandle,
) -> Result<InitResultWire, String> {
    let root = initialize(target, name).map_err(format_project_error)?;
    let scan_stats = run_initial_scan(&root, on_progress)?;
    let ctx = ProjectContext::open(root)?;
    let info = ProjectInfo::from_context(&ctx);

    if let Err(e) = recent::record(
        std::path::Path::new(&info.root),
        &info.name,
        chrono::Utc::now(),
    ) {
        tracing::warn!("could not write recent-projects log: {e}");
    }

    let state = app.state::<AppState>();
    let mut guard = state.project.lock().expect("project mutex poisoned");
    *guard = Some(ctx);

    Ok(InitResultWire {
        project: info,
        scanned: scan_stats.scanned,
        added: scan_stats.added,
        orphan_metas: scan_stats.orphan_metas,
    })
}

struct ScanStats {
    scanned: u64,
    added: u64,
    orphan_metas: u64,
}

fn run_initial_scan(
    root: &ProjectRoot,
    on_progress: &tauri::ipc::Channel<ProgressEvent>,
) -> Result<ScanStats, String> {
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let meta = StdMetaStore::new(fs.clone());
    let index = SqliteIndex::open(&root.index_db())
        .map_err(|e| format!("opening index `{}`: {e}", root.index_db().display()))?;
    let reconciler = Reconciler::new(&fs, &meta, &index);
    let report = reconciler
        .full_scan_with_progress(&|current, total, msg| {
            let _ = on_progress.send(ProgressEvent {
                current,
                total,
                message: msg.to_string(),
            });
        })
        .map_err(|e| format!("initial scan failed: {e}"))?;
    Ok(ScanStats {
        scanned: u64::try_from(report.added() + report.updated() + report.unchanged())
            .unwrap_or(u64::MAX),
        added: u64::try_from(report.added()).unwrap_or(u64::MAX),
        orphan_metas: u64::try_from(report.orphan_metas.len()).unwrap_or(u64::MAX),
    })
}

fn validate_project_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("project name is empty".to_string());
    }
    if trimmed.starts_with('.') {
        return Err("project name may not start with `.`".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("project name may not contain path separators".to_string());
    }
    Ok(())
}

fn target_basename(target: &Path) -> String {
    target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string()
}

fn format_project_error(e: ProjectError) -> String {
    match e {
        ProjectError::AlreadyInitialized { root } => format!(
            "already_initialized: Progest project already exists at `{}` — open it instead",
            root.display()
        ),
        ProjectError::NotFound { start } => format!(
            "not_found: no Progest project found at or above `{}`",
            start.display()
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validator_rejects_bad_names() {
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("   ").is_err());
        assert!(validate_project_name(".hidden").is_err());
        assert!(validate_project_name("a/b").is_err());
        assert!(validate_project_name("a\\b").is_err());
        assert!(validate_project_name("Demo").is_ok());
        assert!(validate_project_name("my-project_2026").is_ok());
    }
}
