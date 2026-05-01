//! IPC commands that back the file inspector — tag add/remove and
//! `[notes]` body read/write.
//!
//! Tag mutations route through `core::tag` (validation + index write).
//! Notes are persisted to the `.meta` sidecar via `core::meta::MetaStore`,
//! and the index's search-projection `notes` column is refreshed in the
//! same call so FTS queries pick up the new body without waiting for the
//! next reconcile.
//!
//! All commands operate on the currently attached project — no project
//! attached returns the same `no_project:` discriminator the rest of the
//! palette / inspector codepaths use.

use std::str::FromStr;

use progest_core::fs::ProjectPath;
use progest_core::history::{AppendRequest, Operation, Store as _};
use progest_core::identity::FileId;
use progest_core::index::{Index, SearchProjection};
use progest_core::meta::{MetaDocument, MetaStore, NotesSection, StdMetaStore, sidecar_path};
use progest_core::tag;
use serde::Serialize;
use tauri::State;
use toml::Table;

use crate::commands::no_project_error;
use crate::state::AppState;

/// Wire shape for `notes_read`.
#[derive(Debug, Clone, Serialize)]
pub struct NotesReadResponse {
    pub path: String,
    pub body: String,
    /// `true` when the sidecar exists at all. Lets the inspector
    /// distinguish "no notes yet" from "we couldn't read the sidecar"
    /// even though both surface as an empty body string.
    pub sidecar_exists: bool,
}

/// Add `tag` to the file identified by `file_id`. Validates the tag
/// shape via `core::tag::validate_tag` and is idempotent at the index
/// layer (re-adding a tag is a no-op).
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn tag_add(file_id: String, tag: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    let id = parse_file_id(&file_id)?;
    tag::add(&ctx.index, &id, &tag).map_err(|e| format!("add tag: {e}"))?;
    if let Some(path) = resolve_path(&ctx.index, &id) {
        let _ = ctx.history.append(&AppendRequest::new(Operation::TagAdd {
            path,
            tag: tag.clone(),
        }));
    }
    Ok(())
}

/// Remove `tag` from the file identified by `file_id`. Missing
/// (file, tag) pairs are no-ops — same contract as `core::tag::remove`.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn tag_remove(file_id: String, tag: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    let id = parse_file_id(&file_id)?;
    tag::remove(&ctx.index, &id, &tag).map_err(|e| format!("remove tag: {e}"))?;
    if let Some(path) = resolve_path(&ctx.index, &id) {
        let _ = ctx
            .history
            .append(&AppendRequest::new(Operation::TagRemove {
                path,
                tag: tag.clone(),
            }));
    }
    Ok(())
}

/// Return every distinct tag in the project, sorted alphabetically.
/// Used by the inspector's tag autocomplete to suggest existing tags.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn tag_list_all(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    ctx.index
        .list_all_tags()
        .map_err(|e| format!("list tags: {e}"))
}

/// Read the `[notes].body` for the file at `path` (project-relative).
/// Returns an empty body for files that don't have a sidecar yet —
/// the inspector's textarea starts blank in that case rather than
/// surfacing an error.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn notes_read(path: String, state: State<'_, AppState>) -> Result<NotesReadResponse, String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    let project_path = ProjectPath::new(&path).map_err(|e| format!("path `{path}`: {e}"))?;
    let sidecar = sidecar_path(&project_path).map_err(|e| format!("sidecar path: {e}"))?;
    let meta = StdMetaStore::new(ctx.fs.clone());
    if !meta.exists(&sidecar) {
        return Ok(NotesReadResponse {
            path,
            body: String::new(),
            sidecar_exists: false,
        });
    }
    let doc = meta
        .load(&sidecar)
        .map_err(|e| format!("load sidecar `{}`: {e}", sidecar.as_str()))?;
    let body = doc
        .notes
        .as_ref()
        .map(|n| n.body.clone())
        .unwrap_or_default();
    Ok(NotesReadResponse {
        path,
        body,
        sidecar_exists: true,
    })
}

/// Write `body` into the file's `[notes]` section. Requires the file
/// to have a sidecar already — reconcile creates one for every tracked
/// asset on first scan, so this is normally true for any file that
/// reached the inspector. The index's `notes` projection column is
/// updated in the same call so search picks up the change without a
/// reconcile.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn notes_write(path: String, body: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    let project_path = ProjectPath::new(&path).map_err(|e| format!("path `{path}`: {e}"))?;
    let sidecar = sidecar_path(&project_path).map_err(|e| format!("sidecar path: {e}"))?;
    let meta = StdMetaStore::new(ctx.fs.clone());
    if !meta.exists(&sidecar) {
        return Err(format!(
            "no sidecar at `{}` — run `progest scan` to track this file before adding notes",
            sidecar.as_str()
        ));
    }
    let before: MetaDocument = meta
        .load(&sidecar)
        .map_err(|e| format!("load sidecar: {e}"))?;
    let mut doc = before.clone();
    update_notes(&mut doc, &body);
    meta.save(&sidecar, &doc)
        .map_err(|e| format!("save sidecar: {e}"))?;
    if before != doc {
        let _ = ctx.history.append(&AppendRequest::new(Operation::MetaEdit {
            path: project_path.clone(),
            before: Box::new(before),
            after: Box::new(doc.clone()),
        }));
    }

    // Mirror the new body into the search-projection column so FTS
    // reflects the edit immediately. Best-effort: a row that's not in
    // the index yet (file was added between reconcile and now) just
    // returns Ok(()) on the lookup branch — we skip the projection
    // update rather than erroring.
    if let Some(row) = ctx
        .index
        .get_file_by_path(&project_path)
        .map_err(|e| format!("index lookup: {e}"))?
    {
        let projection = build_projection_with_notes(&row, &body);
        ctx.index
            .set_search_projection(&row.file_id, &projection)
            .map_err(|e| format!("update search projection: {e}"))?;
    }
    Ok(())
}

fn parse_file_id(s: &str) -> Result<FileId, String> {
    FileId::from_str(s).map_err(|e| format!("invalid file_id `{s}`: {e}"))
}

fn resolve_path(index: &dyn Index, file_id: &FileId) -> Option<ProjectPath> {
    index.get_file(file_id).ok().flatten().map(|row| row.path)
}

/// Replace `doc.notes` with the new body, preserving any extra keys
/// on the existing `[notes]` table (e.g. fields a future build might
/// add) so a round-trip via the inspector doesn't drop unknown data.
fn update_notes(doc: &mut MetaDocument, body: &str) {
    let trimmed_empty = body.is_empty();
    match (doc.notes.as_mut(), trimmed_empty) {
        (Some(existing), _) => {
            existing.body = body.to_string();
        }
        (None, false) => {
            doc.notes = Some(NotesSection {
                body: body.to_string(),
                extra: Table::new(),
            });
        }
        (None, true) => {
            // Writing an empty body to a file that had no [notes]
            // section is a no-op — leave the field absent so the
            // sidecar stays minimal.
        }
    }
}

/// Build a fresh [`SearchProjection`] from the existing index row plus
/// the new notes body. Reconcile is the only other writer, so we
/// reconstruct the row state rather than reading the projection back
/// (the trait doesn't expose a getter for it today).
fn build_projection_with_notes(row: &progest_core::index::FileRow, body: &str) -> SearchProjection {
    SearchProjection {
        name: row.path.file_name().map(str::to_string),
        ext: row.path.extension().map(str::to_string),
        notes: if body.is_empty() {
            None
        } else {
            Some(body.to_string())
        },
        updated_at: None,
        is_orphan: false,
    }
}
