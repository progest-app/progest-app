//! Move a project file (and its `.meta` sidecar) to the OS trash.
//!
//! Uses the `trash` crate which maps to:
//! - macOS: `NSFileManager.trashItem`
//! - Linux: freedesktop trash spec
//! - Windows: `IFileOperation::DeleteItem` with `FOF_ALLOWUNDO`
//!
//! The index entry is removed so the file disappears from search /
//! views immediately. History records the operation so the log shows
//! what happened (but undo redirects the user to the OS trash).

use std::path::Path;

use serde::Serialize;
use thiserror::Error;

use crate::fs::ProjectPath;
use crate::identity::FileId;
use crate::index::Index;
use crate::meta::sidecar_path;

#[derive(Debug, Clone, Serialize)]
pub struct DeleteOutcome {
    pub path: ProjectPath,
    pub file_id: FileId,
    pub sidecar_trashed: bool,
}

#[derive(Debug, Error)]
pub enum DeleteError {
    #[error("file not found in index: {path}")]
    NotIndexed { path: ProjectPath },

    #[error("trash failed for `{path}`: {message}")]
    Trash { path: String, message: String },

    #[error("index error: {0}")]
    Index(String),
}

/// Preview what a delete would do — returns the `file_id` and whether a
/// sidecar exists. No side effects.
pub fn preview_delete(
    index: &dyn Index,
    project_root: &Path,
    path: &ProjectPath,
) -> Result<DeletePreview, DeleteError> {
    let row = index
        .get_file_by_path(path)
        .map_err(|e| DeleteError::Index(e.to_string()))?
        .ok_or_else(|| DeleteError::NotIndexed { path: path.clone() })?;

    let has_sidecar = sidecar_path(path)
        .ok()
        .is_some_and(|sp| project_root.join(sp.as_str()).exists());

    Ok(DeletePreview {
        path: path.clone(),
        file_id: row.file_id,
        has_sidecar,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct DeletePreview {
    pub path: ProjectPath,
    pub file_id: FileId,
    pub has_sidecar: bool,
}

/// Move a directory to the OS trash, removing index entries for any
/// files that were inside it.
pub fn apply_delete_dir(
    index: &dyn Index,
    project_root: &Path,
    path: &ProjectPath,
) -> Result<DeleteOutcome, DeleteError> {
    let abs = project_root.join(path.as_str());
    if !abs.is_dir() {
        return Err(DeleteError::NotIndexed { path: path.clone() });
    }

    let prefix = format!("{}/", path.as_str());
    let indexed_files = index
        .list_files()
        .map_err(|e| DeleteError::Index(e.to_string()))?;
    let mut first_id = None;
    for row in &indexed_files {
        if row.path.as_str().starts_with(&prefix) {
            if first_id.is_none() {
                first_id = Some(row.file_id);
            }
            let _ = index.delete_file(&row.file_id);
        }
    }

    trash::delete(&abs).map_err(|e| DeleteError::Trash {
        path: path.as_str().to_owned(),
        message: e.to_string(),
    })?;

    Ok(DeleteOutcome {
        path: path.clone(),
        file_id: first_id.unwrap_or_else(FileId::new_v7),
        sidecar_trashed: false,
    })
}

/// Move the file (and its `.meta` sidecar if present) to the OS trash,
/// then remove the index entry.
pub fn apply_delete(
    index: &dyn Index,
    project_root: &Path,
    path: &ProjectPath,
) -> Result<DeleteOutcome, DeleteError> {
    let row = index
        .get_file_by_path(path)
        .map_err(|e| DeleteError::Index(e.to_string()))?
        .ok_or_else(|| DeleteError::NotIndexed { path: path.clone() })?;

    let abs = project_root.join(path.as_str());
    trash::delete(&abs).map_err(|e| DeleteError::Trash {
        path: path.as_str().to_owned(),
        message: e.to_string(),
    })?;

    let sidecar_trashed = if let Ok(sp) = sidecar_path(path) {
        let abs_sc = project_root.join(sp.as_str());
        if abs_sc.exists() {
            let _ = trash::delete(&abs_sc);
            true
        } else {
            false
        }
    } else {
        false
    };

    let _ = index.delete_file(&row.file_id);

    Ok(DeleteOutcome {
        path: path.clone(),
        file_id: row.file_id,
        sidecar_trashed,
    })
}
