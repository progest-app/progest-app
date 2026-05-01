//! IPC commands for file/directory CRUD (create, rename, move).

use progest_core::create::{create_dir, create_file};
use progest_core::fs::{FileSystem, ProjectPath};
use progest_core::index::{Index, SearchProjection};
use progest_core::meta::StdMetaStore;
use progest_core::naming::FillMode;
use progest_core::naming::types::{NameCandidate, Segment};
use progest_core::reconcile::Reconciler;
use progest_core::rename;
use progest_core::rename::apply::Rename;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::no_project_error;
use crate::state::AppState;

// ── Create ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CreateOutcomeWire {
    pub path: String,
    pub kind: String,
}

#[tauri::command]
pub async fn fs_create_dir(path: String, app: AppHandle) -> Result<CreateOutcomeWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let project_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let outcome = create_dir(&ctx.fs, &project_path).map_err(|e| format!("{e}"))?;

        Ok(CreateOutcomeWire {
            path: outcome.path.as_str().to_owned(),
            kind: outcome.kind.to_owned(),
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn fs_create_file(path: String, app: AppHandle) -> Result<CreateOutcomeWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let project_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let outcome = create_file(&ctx.fs, &project_path).map_err(|e| format!("{e}"))?;

        // Run a quick reconcile so the new file gets a .meta sidecar
        // and an index entry immediately.
        let meta_store = StdMetaStore::new(ctx.fs.clone());
        let reconciler = Reconciler::new(&ctx.fs, &meta_store, &ctx.index);
        let _ = reconciler.full_scan();

        Ok(CreateOutcomeWire {
            path: outcome.path.as_str().to_owned(),
            kind: outcome.kind.to_owned(),
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ── Rename ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RenameOutcomeWire {
    pub from: String,
    pub to: String,
}

fn name_candidate_from_basename(basename: &str) -> NameCandidate {
    let (stem, ext) = match basename.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), Some(e.to_string())),
        None => (basename.to_string(), None),
    };
    NameCandidate {
        segments: vec![Segment::Literal(stem)],
        ext,
    }
}

/// Rename a single file or directory.
#[tauri::command]
pub async fn fs_rename(
    from: String,
    new_name: String,
    app: AppHandle,
) -> Result<RenameOutcomeWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let from_path =
            ProjectPath::new(&from).map_err(|e| format!("invalid path `{from}`: {e}"))?;

        let candidate = name_candidate_from_basename(&new_name);
        let request = rename::RenameRequest::new(from_path.clone(), candidate);

        let preview = rename::build_preview(&[request], &FillMode::Skip, &ctx.fs)
            .map_err(|e| format!("preview: {e}"))?;

        if !preview.is_clean() {
            let msgs: Vec<String> = preview
                .conflicting_ops()
                .flat_map(|op| op.conflicts.iter().map(|c| c.message.clone()))
                .collect();
            return Err(format!("conflicts: {}", msgs.join("; ")));
        }

        let driver = Rename::new_without_history(&ctx.fs, &ctx.index);
        let outcome = driver.apply(&preview).map_err(|e| format!("apply: {e}"))?;

        let to_path = outcome
            .applied
            .first()
            .map(|op| op.to.as_str().to_owned())
            .unwrap_or_default();

        // Update the index name/ext columns so FlatView shows the new
        // name immediately (Rename::apply updates path but not name).
        if let Some(op) = outcome.applied.first()
            && let Ok(Some(row)) = ctx.index.get_file_by_path(&op.to)
        {
            let name = op.to.file_name().map(str::to_string);
            let ext = op.to.extension().map(str::to_ascii_lowercase);
            let _ = ctx.index.set_search_projection(
                &row.file_id,
                &SearchProjection {
                    name,
                    ext,
                    notes: None,
                    updated_at: None,
                    is_orphan: false,
                },
            );
        }

        Ok(RenameOutcomeWire {
            from: from_path.as_str().to_owned(),
            to: to_path,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ── Move ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MoveOutcomeWire {
    pub from: String,
    pub to: String,
}

/// Move a file or directory to a different parent directory.
#[tauri::command]
pub async fn fs_move(
    path: String,
    dest_dir: String,
    app: AppHandle,
) -> Result<MoveOutcomeWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let from_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let basename = from_path
            .file_name()
            .ok_or_else(|| "cannot move project root".to_string())?;

        let new_path_str = if dest_dir.is_empty() {
            basename.to_string()
        } else {
            format!("{dest_dir}/{basename}")
        };

        if from_path.as_str() == new_path_str {
            return Err("source and destination are the same".into());
        }

        let to_path =
            ProjectPath::new(&new_path_str).map_err(|e| format!("invalid destination: {e}"))?;
        ctx.fs
            .rename(&from_path, &to_path)
            .map_err(|e| format!("move failed: {e}"))?;

        // Move the .meta sidecar if it exists.
        if let (Ok(from_meta), Ok(to_meta)) = (
            progest_core::meta::sidecar_path(&from_path),
            progest_core::meta::sidecar_path(&to_path),
        ) && ctx.fs.exists(&from_meta)
        {
            let _ = ctx.fs.rename(&from_meta, &to_meta);
        }

        // Update index: delete old path, reconcile will pick up new path.
        if let Ok(Some(row)) = ctx.index.get_file_by_path(&from_path) {
            let _ = ctx.index.delete_file(&row.file_id);
        }

        Ok(MoveOutcomeWire {
            from: from_path.as_str().to_owned(),
            to: to_path.as_str().to_owned(),
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ── Open / Reveal ───────────────────────────────────────────────────

/// Open a file with the OS default application.
#[tauri::command]
pub async fn fs_open(path: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;
        let abs = ctx.root.root().join(&path);
        open::that(&abs).map_err(|e| format!("open `{path}`: {e}"))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Reveal a file or directory in the OS file manager (Finder / Explorer).
#[tauri::command]
pub async fn fs_reveal(path: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;
        let abs = ctx.root.root().join(&path);
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg("-R")
                .arg(&abs)
                .spawn()
                .map_err(|e| format!("reveal: {e}"))?;
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg("/select,")
                .arg(&abs)
                .spawn()
                .map_err(|e| format!("reveal: {e}"))?;
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let parent = abs.parent().unwrap_or(&abs);
            open::that(parent).map_err(|e| format!("reveal: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Return the absolute path for clipboard copy.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn fs_abs_path(path: &str, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    Ok(ctx.root.root().join(path).display().to_string())
}
