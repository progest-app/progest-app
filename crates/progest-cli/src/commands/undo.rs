//! `progest undo` / `progest redo` — history-driven replay.
//!
//! Reads the undo/redo stack in `.progest/local/history.db`, peeks at
//! the entry the pointer is aimed at (head for undo, the next-consumed
//! entry for redo), dispatches the replay to the matching driver —
//! currently only `Rename::apply_without_history` — and then flips the
//! `consumed` flag via `Store::undo` / `Store::redo`.
//!
//! The driver-then-flip ordering keeps the log consistent: if the FS
//! replay fails (permission denied, target exists, etc.), history is
//! untouched and the user can retry.
//!
//! Default mode unwinds an entire `group_id` in one invocation (bulk
//! rename, sequence). `--entry` limits the action to the single head
//! entry so pipelines that want per-op granularity can ask for it.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use progest_core::delete::apply_delete;
use progest_core::fs::StdFileSystem;
use progest_core::history::{Entry, Operation, SqliteStore as HistoryStore, Store as _};
use progest_core::index::{Index, SqliteIndex};
use progest_core::meta::{MetaStore, StdMetaStore, sidecar_path};
use progest_core::rename::{Rename, RenameOp, RenamePreview};
use progest_core::tag;
use serde::Serialize;

use crate::context::{discover_root, open_history, open_index};
use crate::output::{OutputFormat, emit_json};

/// Which side of the stack the command is operating on. Carried
/// through the same function so the two subcommands share every
/// dispatch rule (group collection, driver dispatch, reporting).
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Undo,
    Redo,
}

impl Direction {
    fn label(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Redo => "redo",
        }
    }
}

pub struct UndoRedoArgs {
    pub entry_only: bool,
    pub format: OutputFormat,
    pub direction: Direction,
}

pub fn run(cwd: &Path, args: &UndoRedoArgs) -> Result<i32> {
    let root = discover_root(cwd)?;

    let history = open_history(&root)?;

    // Peek at the target entry without flipping consumed yet.
    let target = peek_target(&history, args.direction)?;
    let Some(target) = target else {
        emit_nothing(args.format, args.direction);
        return Ok(0);
    };

    // Collect the contiguous same-group entries from the stack in the
    // order the driver will handle them.
    let plan = collect_plan(&history, &target, args)?;

    let fs = StdFileSystem::new(root.root().to_path_buf());
    let index = open_index(&root)?;
    let meta: StdMetaStore<StdFileSystem> = StdMetaStore::new(fs.clone());

    let mut replayed: Vec<ReportRow> = Vec::new();
    for entry in &plan {
        let op = match args.direction {
            Direction::Undo => &entry.inverse,
            Direction::Redo => &entry.op,
        };
        dispatch_op(op, &fs, &index, &meta, root.root())
            .with_context(|| format!("replaying entry {}", entry.id))?;
        // FS succeeded — flip the stack entry. If this fails the user
        // sees the error directly; history + disk are now mismatched
        // but `progest doctor` will reconcile on a later pass.
        match args.direction {
            Direction::Undo => {
                history
                    .undo()
                    .with_context(|| format!("flipping consumed for entry {}", entry.id))?;
            }
            Direction::Redo => {
                history
                    .redo()
                    .with_context(|| format!("flipping consumed for entry {}", entry.id))?;
            }
        }
        replayed.push(ReportRow {
            entry_id: entry.id,
            op_kind: op_kind_label(op),
            summary: summarize(op),
            group_id: entry.group_id.clone(),
        });
    }

    emit(args.format, args.direction, &replayed);
    Ok(0)
}

// --- planning --------------------------------------------------------------

fn peek_target(history: &HistoryStore, dir: Direction) -> Result<Option<Entry>> {
    match dir {
        Direction::Undo => history.head().context("reading history head"),
        Direction::Redo => {
            // `Store::list` returns newest-first. The next redoable
            // entry is the *oldest* consumed entry strictly newer than
            // the current pointer — equivalent to scanning from the
            // tail of the list. We walk the list in reverse (oldest-
            // first) and return the first consumed one.
            let mut all = history
                .list(usize::MAX)
                .context("listing history entries")?;
            all.reverse();
            Ok(all.into_iter().find(|e| e.consumed))
        }
    }
}

fn collect_plan(history: &HistoryStore, head: &Entry, args: &UndoRedoArgs) -> Result<Vec<Entry>> {
    if args.entry_only || head.group_id.is_none() {
        return Ok(vec![head.clone()]);
    }

    let group = head.group_id.as_deref().unwrap();
    let all = history
        .list(usize::MAX)
        .context("listing history entries")?;

    match args.direction {
        Direction::Undo => {
            // Newest-first: walk from head id downward through the
            // same group while entries remain non-consumed.
            let mut batch: Vec<Entry> = all
                .into_iter()
                .filter(|e| e.id <= head.id && !e.consumed && e.group_id.as_deref() == Some(group))
                .collect();
            // Store::undo always pops the current pointer. To replay
            // in disk order we want oldest-first — but Store::undo
            // walks newest-first, so we keep the newest-first order
            // for the *undo path* and let each call pop the current
            // head.
            batch.sort_by_key(|e| std::cmp::Reverse(e.id));
            Ok(batch)
        }
        Direction::Redo => {
            // Oldest-first: walk upward from the target's id through
            // the same group while entries remain consumed.
            let mut batch: Vec<Entry> = all
                .into_iter()
                .filter(|e| e.id >= head.id && e.consumed && e.group_id.as_deref() == Some(group))
                .collect();
            // Store::redo picks the next-consumed entry just above
            // the pointer — oldest-first matches that progression.
            batch.sort_by_key(|e| e.id);
            Ok(batch)
        }
    }
}

// --- dispatch --------------------------------------------------------------

fn dispatch_op(
    op: &Operation,
    fs: &StdFileSystem,
    index: &SqliteIndex,
    meta: &StdMetaStore<StdFileSystem>,
    project_root: &Path,
) -> Result<()> {
    match op {
        Operation::Rename { from, to, rule_id } => {
            let rename_op = RenameOp {
                from: from.clone(),
                to: to.clone(),
                rule_id: rule_id.clone(),
                group_id: None,
                conflicts: Vec::new(),
            };
            let preview = RenamePreview {
                ops: vec![rename_op],
            };
            let driver = Rename::new_without_history(fs, index);
            driver.apply(&preview).context("applying rename replay")?;
            Ok(())
        }
        Operation::TagAdd { path, tag } => {
            let row = index
                .get_file_by_path(path)
                .context("index lookup for tag replay")?
                .ok_or_else(|| anyhow!("file `{}` not in index", path.as_str()))?;
            tag::add(index, &row.file_id, tag).map_err(|e| anyhow!("tag add replay: {e}"))?;
            Ok(())
        }
        Operation::TagRemove { path, tag } => {
            let row = index
                .get_file_by_path(path)
                .context("index lookup for tag replay")?
                .ok_or_else(|| anyhow!("file `{}` not in index", path.as_str()))?;
            tag::remove(index, &row.file_id, tag).map_err(|e| anyhow!("tag remove replay: {e}"))?;
            Ok(())
        }
        Operation::MetaEdit { path, after, .. } => {
            let sidecar = sidecar_path(path)
                .map_err(|e| anyhow!("sidecar path for `{}`: {e}", path.as_str()))?;
            meta.save(&sidecar, after)
                .map_err(|e| anyhow!("meta restore for `{}`: {e}", path.as_str()))?;
            Ok(())
        }
        Operation::Import {
            path,
            is_inverse: true,
        } => {
            apply_delete(index, project_root, path)
                .map_err(|e| anyhow!("import undo (trash) for `{}`: {e}", path.as_str()))?;
            Ok(())
        }
        Operation::Import {
            path,
            is_inverse: false,
        } => Err(anyhow!(
            "redo of import for `{}` requires re-importing the original file, which is not automated",
            path.as_str()
        )),
    }
}

// --- output ----------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ReportRow {
    entry_id: i64,
    op_kind: &'static str,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_id: Option<String>,
}

fn emit_nothing(fmt: OutputFormat, dir: Direction) {
    match fmt {
        OutputFormat::Text => println!("(nothing to {})", dir.label()),
        OutputFormat::Json => {
            // Empty array keeps the wire contract: consumers can
            // always iterate the top-level array.
            println!("[]");
        }
    }
}

fn emit(fmt: OutputFormat, dir: Direction, rows: &[ReportRow]) {
    match fmt {
        OutputFormat::Text => {
            println!(
                "{}d {} entr{}",
                dir.label(),
                rows.len(),
                if rows.len() == 1 { "y" } else { "ies" }
            );
            for r in rows {
                let group = r
                    .group_id
                    .as_deref()
                    .map(|g| format!(" [{g}]"))
                    .unwrap_or_default();
                println!("  #{} {} {}{}", r.entry_id, r.op_kind, r.summary, group);
            }
        }
        OutputFormat::Json => {
            if let Err(e) = emit_json(&rows, "undo/redo") {
                eprintln!("error: {e}");
            }
        }
    }
}

fn op_kind_label(op: &Operation) -> &'static str {
    op.kind().as_str()
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
