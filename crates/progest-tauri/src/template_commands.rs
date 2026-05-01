use serde::Serialize;
use tauri::{AppHandle, Manager};

use progest_core::template;

use crate::commands::no_project_error;
use crate::progress::ProgressEvent;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ExportResultWire {
    pub path: String,
    pub name: String,
    pub directories: usize,
    pub configs: Vec<String>,
    pub dirmeta: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplatePreviewWire {
    pub meta: TemplateMetaWire,
    pub directories: Vec<String>,
    pub has_rules: bool,
    pub has_schema: bool,
    pub has_views: bool,
    pub dirmeta_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateMetaWire {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResultWire {
    pub directories_created: usize,
    pub configs_written: Vec<String>,
    pub dirmeta_written: usize,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn template_export(
    out_path: String,
    include: String,
    name: Option<String>,
    app: AppHandle,
) -> Result<ExportResultWire, String> {
    let state = app.state::<AppState>();
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;

    let project_name = name.unwrap_or_else(|| {
        ctx.root
            .root()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_owned()
    });

    let options = template::ExportOptions::from_include_str(&include);
    let doc = template::export_template(&ctx.root, &project_name, &options)
        .map_err(|e| format!("export: {e}"))?;

    let toml_str = template::serialize(&doc).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&out_path, &toml_str).map_err(|e| format!("write: {e}"))?;

    let mut configs = Vec::new();
    if doc.include.rules_toml.is_some() {
        configs.push("rules.toml".to_owned());
    }
    if doc.include.schema_toml.is_some() {
        configs.push("schema.toml".to_owned());
    }
    if doc.include.views_toml.is_some() {
        configs.push("views.toml".to_owned());
    }

    Ok(ExportResultWire {
        path: out_path,
        name: doc.meta.name,
        directories: doc.directories.len(),
        configs,
        dirmeta: doc.dirmeta.len(),
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn template_preview(template_path: String) -> Result<TemplatePreviewWire, String> {
    let content =
        std::fs::read_to_string(&template_path).map_err(|e| format!("read template: {e}"))?;
    let doc = template::deserialize(&content).map_err(|e| format!("parse template: {e}"))?;

    Ok(TemplatePreviewWire {
        meta: TemplateMetaWire {
            id: doc.meta.id,
            name: doc.meta.name,
            version: doc.meta.version,
            author: doc.meta.author,
            description: doc.meta.description,
        },
        directories: doc.directories,
        has_rules: doc.include.rules_toml.is_some(),
        has_schema: doc.include.schema_toml.is_some(),
        has_views: doc.include.views_toml.is_some(),
        dirmeta_count: doc.dirmeta.len(),
    })
}

#[tauri::command]
pub async fn template_apply(
    template_path: String,
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    app: AppHandle,
) -> Result<ApplyResultWire, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let content =
            std::fs::read_to_string(&template_path).map_err(|e| format!("read template: {e}"))?;
        let doc = template::deserialize(&content).map_err(|e| format!("parse template: {e}"))?;
        let report = template::apply_template_with_progress(
            ctx.root.root(),
            &doc,
            &|current, total, msg| {
                let _ = on_progress.send(ProgressEvent {
                    current,
                    total,
                    message: msg.to_string(),
                });
            },
        )
        .map_err(|e| format!("apply: {e}"))?;

        Ok(ApplyResultWire {
            directories_created: report.directories_created,
            configs_written: report.configs_written,
            dirmeta_written: report.dirmeta_written,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}
