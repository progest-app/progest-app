use serde::Serialize;
use tauri::{AppHandle, Manager};

use progest_core::ai::{
    self, AiConfig, AiProvider, PrivacyFlags, SuggestionType, extract_ai_config,
};
use progest_core::fs::{ProjectPath, StdFileSystem};
use progest_core::meta::StdMetaStore;
use progest_core::naming::types::{NameCandidate, Segment};
use progest_core::project::ProjectDocument;
use progest_core::rename;

use crate::state::AppState;

fn no_project_error() -> String {
    "no Progest project loaded".into()
}

// ── Wire types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AiSuggestionWire {
    pub value: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiSuggestResponse {
    pub suggestions: Vec<AiSuggestionWire>,
    pub model: String,
    pub provider: String,
    pub elapsed_ms: u64,
    pub suggestion_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiConfigResponse {
    pub provider: String,
    pub model: String,
    pub audit_log: bool,
    pub has_key: bool,
    pub glossary: Vec<String>,
}

// ── Commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_suggest(
    path: String,
    suggestion_type: String,
    include_notes: bool,
    app: AppHandle,
) -> Result<AiSuggestResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let stype = parse_suggestion_type(&suggestion_type)?;
        let file_path =
            ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;
        let config = load_config(ctx)?;
        let privacy = PrivacyFlags { include_notes };
        let fs = StdFileSystem::new(ctx.root.root().to_path_buf());
        let meta_store = StdMetaStore::new(fs);
        let local = ctx.root.dot_dir().join("local");

        let response = ai::suggest(
            &file_path,
            stype,
            &privacy,
            ctx.root.root(),
            &ctx.index,
            &meta_store,
            &config,
            &local,
        )
        .map_err(|e| e.to_string())?;

        Ok(AiSuggestResponse {
            suggestions: response
                .suggestions
                .into_iter()
                .map(|s| AiSuggestionWire {
                    value: s.value,
                    explanation: s.explanation,
                })
                .collect(),
            model: response.model,
            provider: response.provider.to_string(),
            elapsed_ms: response.elapsed_ms,
            suggestion_type: suggestion_type.clone(),
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[derive(Debug, Clone, Serialize)]
pub struct AiRenameResult {
    pub old_path: String,
    pub new_path: String,
}

#[tauri::command]
pub async fn ai_apply_rename(
    path: String,
    new_name: String,
    app: AppHandle,
) -> Result<AiRenameResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let from = ProjectPath::new(&path).map_err(|e| format!("invalid path `{path}`: {e}"))?;

        let (stem, ext) = match new_name.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), Some(e.to_string())),
            None => (new_name.clone(), None),
        };
        let candidate = NameCandidate {
            segments: vec![Segment::Literal(stem)],
            ext,
        };

        let request = rename::RenameRequest::new(from.clone(), candidate);
        let fs = &ctx.fs;
        let preview = rename::build_preview(&[request], &progest_core::naming::FillMode::Skip, fs)
            .map_err(|e| format!("rename preview: {e}"))?;

        if !preview.is_clean() {
            let conflicts: Vec<String> = preview
                .conflicting_ops()
                .map(|op| format!("{}: {:?}", op.from, op.conflicts))
                .collect();
            return Err(format!("rename conflicts: {}", conflicts.join("; ")));
        }

        let driver = rename::Rename::new_without_history(fs, &ctx.index);
        let outcome = driver
            .apply(&preview)
            .map_err(|e| format!("rename apply: {e}"))?;

        let new_path = outcome
            .applied
            .first()
            .map(|op| op.to.as_str().to_string())
            .unwrap_or_default();

        Ok(AiRenameResult {
            old_path: path,
            new_path,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn ai_set_key(provider: String, key: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let prov = parse_provider(&provider)?;
        ai::store_api_key(prov, &key).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn ai_delete_key(provider: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let prov = parse_provider(&provider)?;
        ai::delete_api_key(prov).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn ai_get_config(app: AppHandle) -> Result<AiConfigResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let config = load_config(ctx)?;
        let has_key = ai::has_api_key(config.provider);

        Ok(AiConfigResponse {
            provider: config.provider.to_string(),
            model: config.model,
            audit_log: config.audit_log,
            has_key,
            glossary: config.glossary,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn ai_set_config(
    provider: Option<String>,
    model: Option<String>,
    audit_log: Option<bool>,
    app: AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.project.lock().expect("project mutex poisoned");
        let ctx = guard.as_ref().ok_or_else(no_project_error)?;

        let toml_path = ctx.root.project_toml();
        let text = std::fs::read_to_string(&toml_path)
            .map_err(|e| format!("reading {}: {e}", toml_path.display()))?;
        let mut doc: toml::Table = text
            .parse()
            .map_err(|e| format!("parsing project.toml: {e}"))?;

        let ai = doc
            .entry("ai")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or("project.toml [ai] is not a table")?;

        if let Some(p) = &provider {
            parse_provider(p)?;
            ai.insert("provider".into(), toml::Value::String(p.clone()));
        }
        if let Some(m) = &model {
            ai.insert("model".into(), toml::Value::String(m.clone()));
        }
        if let Some(a) = audit_log {
            ai.insert("audit_log".into(), toml::Value::Boolean(a));
        }

        let new_text =
            toml::to_string_pretty(&doc).map_err(|e| format!("serializing project.toml: {e}"))?;
        std::fs::write(&toml_path, new_text)
            .map_err(|e| format!("writing {}: {e}", toml_path.display()))?;

        Ok(())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ── Helpers ─────────────────────────────────────────────────────────

fn parse_suggestion_type(s: &str) -> Result<SuggestionType, String> {
    match s {
        "naming" => Ok(SuggestionType::Naming),
        "tags" => Ok(SuggestionType::Tags),
        "notes" => Ok(SuggestionType::Notes),
        "placement" => Ok(SuggestionType::Placement),
        other => Err(format!("unknown suggestion type `{other}`")),
    }
}

fn parse_provider(s: &str) -> Result<AiProvider, String> {
    match s {
        "anthropic" => Ok(AiProvider::Anthropic),
        "openai" => Ok(AiProvider::OpenAi),
        other => Err(format!("unknown provider `{other}`")),
    }
}

fn load_config(ctx: &crate::state::ProjectContext) -> Result<AiConfig, String> {
    let toml_path = ctx.root.project_toml();
    let text = std::fs::read_to_string(&toml_path)
        .map_err(|e| format!("reading {}: {e}", toml_path.display()))?;
    let doc =
        ProjectDocument::from_toml_str(&text).map_err(|e| format!("parsing project.toml: {e}"))?;
    let (config, _warnings) =
        extract_ai_config(&doc.extra).map_err(|e| format!("[ai] config: {e}"))?;
    Ok(config)
}
