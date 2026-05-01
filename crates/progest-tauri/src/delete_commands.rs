//! IPC commands for file deletion (OS trash).

use progest_core::delete::{apply_delete, apply_delete_dir, preview_delete};
use progest_core::fs::ProjectPath;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::no_project_error;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct DeletePreviewWire {
    pub path: String,
    pub file_id: String,
    pub has_sidecar: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteOutcomeWire {
    pub path: String,
    pub file_id: String,
    pub sidecar_trashed: bool,
}

#[tauri::command]
pub async fn file_delete_preview(
    path: String,
    app: AppHandle,
) -> Result<DeletePreviewWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let project_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let preview = preview_delete(&ctx.index, ctx.root.root(), &project_path)
            .map_err(|e| format!("{e}"))?;

        Ok(DeletePreviewWire {
            path: preview.path.as_str().to_owned(),
            file_id: preview.file_id.to_string(),
            has_sidecar: preview.has_sidecar,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn file_delete_apply(path: String, app: AppHandle) -> Result<DeleteOutcomeWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let project_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let outcome =
            apply_delete(&ctx.index, ctx.root.root(), &project_path).map_err(|e| format!("{e}"))?;

        Ok(DeleteOutcomeWire {
            path: outcome.path.as_str().to_owned(),
            file_id: outcome.file_id.to_string(),
            sidecar_trashed: outcome.sidecar_trashed,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn dir_delete_apply(path: String, app: AppHandle) -> Result<DeleteOutcomeWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let project_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let outcome = apply_delete_dir(&ctx.index, ctx.root.root(), &project_path)
            .map_err(|e| format!("{e}"))?;

        Ok(DeleteOutcomeWire {
            path: outcome.path.as_str().to_owned(),
            file_id: outcome.file_id.to_string(),
            sidecar_trashed: outcome.sidecar_trashed,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
