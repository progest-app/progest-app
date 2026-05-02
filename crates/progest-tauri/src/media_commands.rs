//! IPC command for file media info (dimensions, duration, codec).

use progest_core::media;
use tauri::{AppHandle, Manager};

use crate::commands::no_project_error;
use crate::state::AppState;

#[tauri::command]
pub async fn file_media_info(path: String, app: AppHandle) -> Result<media::MediaInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;
        let abs = ctx.root.root().join(&path);
        Ok(media::probe(&abs))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
