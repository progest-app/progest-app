//! IPC commands for thumbnail retrieval.
//!
//! Returns base64-encoded data URLs so the frontend can use them
//! directly as `<img src>` without asset-protocol scope configuration.
//!
//! Async + `spawn_blocking` so base64 encoding of many thumbnails
//! doesn't freeze the UI.

use std::collections::HashMap;
use std::str::FromStr;

use base64::Engine;
use progest_core::identity::FileId;
use progest_core::index::Index;
use progest_core::thumbnail::{
    CacheKey, DEFAULT_CACHE_MAX_BYTES, DEFAULT_MAX_DIM, ThumbnailCache,
    generate_batch_with_progress, requests_from_index,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::no_project_error;
use crate::progress::ProgressEvent;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ThumbnailUrlsResponse {
    pub urls: HashMap<String, String>,
}

#[tauri::command]
pub async fn thumbnail_paths(
    file_ids: Vec<String>,
    app: AppHandle,
) -> Result<ThumbnailUrlsResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let cache = ThumbnailCache::new(
            ctx.root.root().join(".progest/thumbs"),
            DEFAULT_CACHE_MAX_BYTES,
        );

        let mut urls = HashMap::new();

        for fid_str in &file_ids {
            let Ok(file_id) = FileId::from_str(fid_str) else {
                continue;
            };
            let Ok(Some(row)) = ctx.index.get_file(&file_id) else {
                continue;
            };

            let key = CacheKey {
                file_id: row.file_id,
                fingerprint: row.fingerprint,
                size: DEFAULT_MAX_DIM,
            };

            if let Some(abs_path) = cache.get(&key)
                && let Ok(bytes) = std::fs::read(&abs_path)
            {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                urls.insert(fid_str.clone(), format!("data:image/webp;base64,{b64}"));
            }
        }

        Ok(ThumbnailUrlsResponse { urls })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[derive(Debug, Clone, Serialize)]
pub struct ThumbnailGenerateResponse {
    pub generated: u64,
    pub cached: u64,
    pub skipped: u64,
}

/// Generate thumbnails for all indexed files, reporting progress via
/// the Tauri channel. Runs on the blocking thread pool so the UI
/// stays responsive during heavy image/video processing.
#[tauri::command]
pub async fn thumbnail_generate(
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    app: AppHandle,
) -> Result<ThumbnailGenerateResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let cache = ThumbnailCache::new(
            ctx.root.root().join(".progest/thumbs"),
            DEFAULT_CACHE_MAX_BYTES,
        );

        let requests = requests_from_index(&ctx.index, ctx.root.root(), DEFAULT_MAX_DIM);
        let report =
            generate_batch_with_progress(&requests, &cache, false, &|current, total, msg| {
                let _ = on_progress.send(ProgressEvent {
                    current,
                    total,
                    message: msg.to_string(),
                });
            });

        Ok(ThumbnailGenerateResponse {
            generated: report.generated as u64,
            cached: report.cached as u64,
            skipped: report.skipped as u64,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
