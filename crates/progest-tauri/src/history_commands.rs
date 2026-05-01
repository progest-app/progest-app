//! IPC commands for history undo/redo.

use progest_core::delete::apply_delete;
use progest_core::history::{Entry, Operation, Store as _};
use progest_core::index::Index;
use progest_core::meta::{MetaStore, StdMetaStore, sidecar_path};
use progest_core::tag;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::no_project_error;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntryWire {
    pub id: i64,
    pub ts: String,
    pub op_kind: String,
    pub summary: String,
    pub consumed: bool,
    pub group_id: Option<String>,
}

impl From<&Entry> for HistoryEntryWire {
    fn from(e: &Entry) -> Self {
        Self {
            id: e.id,
            ts: e.ts.clone(),
            op_kind: e.op.kind().as_str().to_owned(),
            summary: summarize(&e.op),
            consumed: e.consumed,
            group_id: e.group_id.clone(),
        }
    }
}

#[tauri::command]
pub async fn history_list(
    limit: Option<usize>,
    app: AppHandle,
) -> Result<Vec<HistoryEntryWire>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;
        let entries = ctx
            .history
            .list(limit.unwrap_or(50))
            .map_err(|e| format!("history list: {e}"))?;
        Ok(entries.iter().map(HistoryEntryWire::from).collect())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn history_undo(app: AppHandle) -> Result<Vec<HistoryEntryWire>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let head = ctx
            .history
            .head()
            .map_err(|e| format!("history head: {e}"))?;
        let Some(head) = head else {
            return Ok(vec![]);
        };

        let plan = collect_group_plan(&ctx.history, &head, Direction::Undo)?;
        let meta = StdMetaStore::new(ctx.fs.clone());
        let mut replayed = Vec::new();

        for entry in &plan {
            dispatch_op(&entry.inverse, &ctx.index, &meta, ctx.root.root())
                .map_err(|e| format!("replay entry #{}: {e}", entry.id))?;
            ctx.history
                .undo()
                .map_err(|e| format!("flip entry #{}: {e}", entry.id))?;
            replayed.push(HistoryEntryWire::from(entry));
        }

        Ok(replayed)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn history_redo(app: AppHandle) -> Result<Vec<HistoryEntryWire>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let all = ctx
            .history
            .list(usize::MAX)
            .map_err(|e| format!("history list: {e}"))?;
        let oldest_first: Vec<Entry> = all.into_iter().rev().collect();
        let target = oldest_first.iter().find(|e| e.consumed);
        let Some(target) = target else {
            return Ok(vec![]);
        };
        let target = target.clone();

        let plan = collect_group_plan(&ctx.history, &target, Direction::Redo)?;
        let meta = StdMetaStore::new(ctx.fs.clone());
        let mut replayed = Vec::new();

        for entry in &plan {
            dispatch_op(&entry.op, &ctx.index, &meta, ctx.root.root())
                .map_err(|e| format!("replay entry #{}: {e}", entry.id))?;
            ctx.history
                .redo()
                .map_err(|e| format!("flip entry #{}: {e}", entry.id))?;
            replayed.push(HistoryEntryWire::from(entry));
        }

        Ok(replayed)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[derive(Clone, Copy)]
enum Direction {
    Undo,
    Redo,
}

fn collect_group_plan(
    history: &progest_core::history::SqliteStore,
    head: &Entry,
    dir: Direction,
) -> Result<Vec<Entry>, String> {
    if head.group_id.is_none() {
        return Ok(vec![head.clone()]);
    }
    let group = head.group_id.as_deref().unwrap();
    let all = history
        .list(usize::MAX)
        .map_err(|e| format!("history list: {e}"))?;

    match dir {
        Direction::Undo => {
            let mut batch: Vec<Entry> = all
                .into_iter()
                .filter(|e| e.id <= head.id && !e.consumed && e.group_id.as_deref() == Some(group))
                .collect();
            batch.sort_by_key(|e| std::cmp::Reverse(e.id));
            Ok(batch)
        }
        Direction::Redo => {
            let mut batch: Vec<Entry> = all
                .into_iter()
                .filter(|e| e.id >= head.id && e.consumed && e.group_id.as_deref() == Some(group))
                .collect();
            batch.sort_by_key(|e| e.id);
            Ok(batch)
        }
    }
}

fn dispatch_op(
    op: &Operation,
    index: &progest_core::index::SqliteIndex,
    meta: &StdMetaStore<progest_core::fs::StdFileSystem>,
    project_root: &std::path::Path,
) -> Result<(), String> {
    match op {
        Operation::Rename { from, to, rule_id } => {
            let rename_op = progest_core::rename::RenameOp {
                from: from.clone(),
                to: to.clone(),
                rule_id: rule_id.clone(),
                group_id: None,
                conflicts: Vec::new(),
            };
            let preview = progest_core::rename::RenamePreview {
                ops: vec![rename_op],
            };
            let fs = progest_core::fs::StdFileSystem::new(project_root.to_path_buf());
            let driver = progest_core::rename::Rename::new_without_history(&fs, index);
            driver
                .apply(&preview)
                .map_err(|e| format!("rename replay: {e}"))?;
            Ok(())
        }
        Operation::TagAdd { path, tag } => {
            let row = index
                .get_file_by_path(path)
                .map_err(|e| format!("index lookup: {e}"))?
                .ok_or_else(|| format!("file `{}` not in index", path.as_str()))?;
            tag::add(index, &row.file_id, tag).map_err(|e| format!("tag add: {e}"))?;
            Ok(())
        }
        Operation::TagRemove { path, tag } => {
            let row = index
                .get_file_by_path(path)
                .map_err(|e| format!("index lookup: {e}"))?
                .ok_or_else(|| format!("file `{}` not in index", path.as_str()))?;
            tag::remove(index, &row.file_id, tag).map_err(|e| format!("tag remove: {e}"))?;
            Ok(())
        }
        Operation::MetaEdit { path, after, .. } => {
            let sidecar = sidecar_path(path)
                .map_err(|e| format!("sidecar path for `{}`: {e}", path.as_str()))?;
            meta.save(&sidecar, after)
                .map_err(|e| format!("meta restore for `{}`: {e}", path.as_str()))?;
            Ok(())
        }
        Operation::Import {
            path,
            is_inverse: true,
        } => {
            apply_delete(index, project_root, path)
                .map_err(|e| format!("import undo (trash) for `{}`: {e}", path.as_str()))?;
            Ok(())
        }
        Operation::Import {
            path,
            is_inverse: false,
        } => Err(format!(
            "redo of import for `{}` requires re-importing the original file",
            path.as_str()
        )),
    }
}

fn summarize(op: &Operation) -> String {
    match op {
        Operation::Rename { from, to, .. } => format!("{} → {}", from.as_str(), to.as_str()),
        Operation::TagAdd { path, tag } => format!("+{} @ {}", tag, path.as_str()),
        Operation::TagRemove { path, tag } => format!("-{} @ {}", tag, path.as_str()),
        Operation::MetaEdit { path, .. } => format!("meta @ {}", path.as_str()),
        Operation::Import { path, is_inverse } => {
            if *is_inverse {
                format!("rm-import {}", path.as_str())
            } else {
                format!("import {}", path.as_str())
            }
        }
    }
}
