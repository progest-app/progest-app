//! `progest view {save|delete|list}` — manage saved searches in
//! `.progest/views.toml`.

use std::path::Path;

use anyhow::{Context, Result};
use progest_core::fs::{ProjectPath, StdFileSystem};
use progest_core::search::views::{
    View, ViewError, ViewsDocument, delete as delete_view, load as load_views_doc,
    save as save_views_doc, upsert as upsert_view,
};
use serde::Serialize;

use crate::context::discover_root;
use crate::output::{OutputFormat, emit_json};

pub enum ViewCommand {
    Save {
        id: String,
        name: Option<String>,
        query: String,
        description: Option<String>,
        group_by: Option<String>,
    },
    Delete {
        id: String,
    },
    List,
}

pub struct ViewArgs {
    pub command: ViewCommand,
    pub format: OutputFormat,
}

pub fn run(cwd: &Path, args: &ViewArgs) -> Result<i32> {
    let root = discover_root(cwd)?;
    let fs = StdFileSystem::new(root.root().to_path_buf());
    let path =
        ProjectPath::new(".progest/views.toml").map_err(|e| anyhow::anyhow!("internal: {e}"))?;

    let mut doc = match load_views_doc(&fs, &path) {
        Ok(d) => d,
        Err(ViewError::NotFound) => ViewsDocument::default(),
        Err(e) => return Err(anyhow::anyhow!("read views.toml: {e}")),
    };

    match &args.command {
        ViewCommand::Save {
            id,
            name,
            query,
            description,
            group_by,
        } => {
            let view = View {
                id: id.clone(),
                name: name.clone().unwrap_or_else(|| id.clone()),
                query: query.clone(),
                description: description.clone(),
                group_by: group_by.clone(),
                sort: None,
            };
            if let Err(e) = upsert_view(&mut doc, view) {
                eprintln!("error: {e}");
                return Ok(2);
            }
            save_views_doc(&fs, &path, &doc).context("save views.toml")?;
            match args.format {
                OutputFormat::Text => println!("saved view {id}"),
                OutputFormat::Json => {
                    #[derive(Serialize)]
                    struct R<'a> {
                        op: &'static str,
                        id: &'a str,
                    }
                    emit_json(&R { op: "save", id }, "view")?;
                }
            }
            Ok(0)
        }
        ViewCommand::Delete { id } => match delete_view(&mut doc, id) {
            Ok(()) => {
                save_views_doc(&fs, &path, &doc).context("save views.toml")?;
                match args.format {
                    OutputFormat::Text => println!("deleted view {id}"),
                    OutputFormat::Json => {
                        #[derive(Serialize)]
                        struct R<'a> {
                            op: &'static str,
                            id: &'a str,
                        }
                        emit_json(&R { op: "delete", id }, "view")?;
                    }
                }
                Ok(0)
            }
            Err(ViewError::UnknownId { .. }) => {
                eprintln!("error: view {id:?} not found");
                Ok(2)
            }
            Err(e) => Err(anyhow::anyhow!("{e}")),
        },
        ViewCommand::List => {
            match args.format {
                OutputFormat::Text => {
                    if doc.views.is_empty() {
                        println!("(no saved views)");
                    } else {
                        for v in &doc.views {
                            println!("{}\t{}\t{}", v.id, v.name, v.query);
                        }
                    }
                }
                OutputFormat::Json => emit_json(&doc.views, "view-list")?,
            }
            Ok(0)
        }
    }
}
