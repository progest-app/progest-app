//! `progest tag {add|remove|list}` — manage per-file tags.
//!
//! Each file argument is resolved to its `file_id` via the index
//! (`get_file_by_path`). Files that aren't yet in the index are
//! reported as warnings on stderr and skipped — we never silently
//! create rows here. Tag names are validated by `core::tag` before
//! any DB write.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use progest_core::fs::ProjectPath;
use progest_core::index::Index;
use progest_core::tag;
use serde::Serialize;

use crate::context::{discover_root, open_index};
use crate::output::{OutputFormat, emit_json};

pub enum TagCommand {
    Add { tag: String, files: Vec<PathBuf> },
    Remove { tag: String, files: Vec<PathBuf> },
    List { files: Vec<PathBuf> },
}

pub struct TagArgs {
    pub command: TagCommand,
    pub format: OutputFormat,
}

#[derive(Serialize)]
struct ListEntry {
    path: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct MutateEntry {
    path: String,
    ok: bool,
    message: Option<String>,
}

pub fn run(cwd: &Path, args: &TagArgs) -> Result<i32> {
    let root = discover_root(cwd)?;
    let index = open_index(&root).context("opening index")?;

    match &args.command {
        TagCommand::Add { tag, files } => mutate(&root, &index, tag, files, args.format, true),
        TagCommand::Remove { tag, files } => mutate(&root, &index, tag, files, args.format, false),
        TagCommand::List { files } => list(&root, &index, files, args.format),
    }
}

fn mutate(
    root: &progest_core::project::ProjectRoot,
    index: &dyn Index,
    tag_name: &str,
    files: &[PathBuf],
    format: OutputFormat,
    add: bool,
) -> Result<i32> {
    let mut entries = Vec::with_capacity(files.len());
    let mut had_error = false;
    for input in files {
        let path = match resolve(root, input) {
            Ok(p) => p,
            Err(e) => {
                had_error = true;
                entries.push(MutateEntry {
                    path: input.to_string_lossy().into_owned(),
                    ok: false,
                    message: Some(format!("invalid path: {e}")),
                });
                continue;
            }
        };
        let row = match index.get_file_by_path(&path) {
            Ok(Some(r)) => r,
            Ok(None) => {
                had_error = true;
                entries.push(MutateEntry {
                    path: path.as_str().into(),
                    ok: false,
                    message: Some("not in index (run `progest scan` first)".into()),
                });
                continue;
            }
            Err(e) => {
                had_error = true;
                entries.push(MutateEntry {
                    path: path.as_str().into(),
                    ok: false,
                    message: Some(format!("index error: {e}")),
                });
                continue;
            }
        };
        let result = if add {
            tag::add(index, &row.file_id, tag_name)
        } else {
            tag::remove(index, &row.file_id, tag_name)
        };
        match result {
            Ok(()) => entries.push(MutateEntry {
                path: path.as_str().into(),
                ok: true,
                message: None,
            }),
            Err(e) => {
                had_error = true;
                entries.push(MutateEntry {
                    path: path.as_str().into(),
                    ok: false,
                    message: Some(e.to_string()),
                });
            }
        }
    }

    match format {
        OutputFormat::Text => {
            let verb = if add { "add" } else { "remove" };
            for e in &entries {
                if e.ok {
                    println!("{verb} {} on {}", tag_name, e.path);
                } else if let Some(m) = &e.message {
                    eprintln!("{verb} {} skipped on {}: {m}", tag_name, e.path);
                }
            }
        }
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct Envelope<'a> {
                tag: &'a str,
                op: &'a str,
                results: &'a [MutateEntry],
            }
            let env = Envelope {
                tag: tag_name,
                op: if add { "add" } else { "remove" },
                results: &entries,
            };
            emit_json(&env, "tag")?;
        }
    }
    Ok(i32::from(had_error))
}

fn list(
    root: &progest_core::project::ProjectRoot,
    index: &dyn Index,
    files: &[PathBuf],
    format: OutputFormat,
) -> Result<i32> {
    let mut entries = Vec::with_capacity(files.len());
    let mut had_error = false;
    for input in files {
        let Ok(path) = resolve(root, input) else {
            had_error = true;
            continue;
        };
        let Ok(Some(row)) = index.get_file_by_path(&path) else {
            had_error = true;
            entries.push(ListEntry {
                path: path.as_str().into(),
                tags: vec![],
            });
            continue;
        };
        let tags = tag::list(index, &row.file_id).unwrap_or_default();
        entries.push(ListEntry {
            path: path.as_str().into(),
            tags,
        });
    }

    match format {
        OutputFormat::Text => {
            for e in &entries {
                if e.tags.is_empty() {
                    println!("{}: (no tags)", e.path);
                } else {
                    println!("{}: {}", e.path, e.tags.join(", "));
                }
            }
        }
        OutputFormat::Json => emit_json(&entries, "tag-list")?,
    }
    Ok(i32::from(had_error))
}

fn resolve(
    root: &progest_core::project::ProjectRoot,
    input: &Path,
) -> std::result::Result<ProjectPath, progest_core::fs::ProjectPathError> {
    if input.is_absolute() {
        ProjectPath::from_absolute(root.root(), input)
    } else {
        ProjectPath::from_path(input)
    }
}
