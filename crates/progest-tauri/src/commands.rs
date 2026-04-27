//! Tauri IPC command surface for the M3 command palette.
//!
//! Three search commands plus a single `app_info` snapshot the React
//! shell calls once on boot to decide between the empty state and the
//! palette UI. Every command returns a wire type that lives in this
//! file (no leaky `core::*` re-exports across the boundary) so the
//! TypeScript side can mirror it without pulling in transitive Rust
//! types.
//!
//! Errors are returned as plain `String` payloads — Tauri serializes
//! them into `Promise.reject(...)` on the JS side. Tagged variants for
//! the structured "no project" / "parse failed" cases live on each
//! response type so the UI can branch on a discriminator instead of
//! string-matching the error text.

use std::collections::BTreeMap;

use chrono::Utc;
use progest_core::fs::ProjectPath;
use progest_core::search::history::{
    HistoryDocument, HistoryEntry, HistoryError, append as history_append, clear as history_clear,
    load as load_history, save as save_history,
};
use progest_core::search::{
    CustomFieldKind, CustomFields, RichSearchHit, execute, parse, plan, project_hits, validate,
};
use serde::Serialize;
use tauri::State;

use crate::state::{AppState, ProjectContext, ProjectInfo};

const SEARCH_HISTORY_PATH: &str = ".progest/local/search-history.json";

/// Top-level boot snapshot — currently just the attached project.
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub project: Option<ProjectInfo>,
}

/// Response shape for `search_execute`.
///
/// `parse_error: Some(_)` ⇒ `hits` is empty and `warnings` is empty;
/// `parse_error: None` ⇒ the query reached the executor (zero hits is
/// still a success).
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub hits: Vec<RichSearchHit>,
    pub warnings: Vec<String>,
    pub parse_error: Option<ParseErrorPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParseErrorPayload {
    pub message: String,
    pub column: Option<usize>,
}

/// Wire shape for one history entry. Mirrors `core::search::history`
/// but stays in this crate so the frontend doesn't import from core.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntryWire {
    pub query: String,
    pub ts: String,
}

impl From<HistoryEntry> for HistoryEntryWire {
    fn from(e: HistoryEntry) -> Self {
        Self {
            query: e.query,
            ts: e.ts.to_rfc3339(),
        }
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let guard = state.project.lock().expect("project mutex poisoned");
    AppInfo {
        project: guard.as_ref().map(ProjectInfo::from_context),
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn search_execute(query: String, state: State<'_, AppState>) -> Result<SearchResponse, String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;

    let parsed = match parse(&query) {
        Ok(p) => p,
        Err(e) => {
            return Ok(SearchResponse {
                query,
                hits: Vec::new(),
                warnings: Vec::new(),
                parse_error: Some(ParseErrorPayload {
                    message: e.to_string(),
                    column: e.column(),
                }),
            });
        }
    };
    let schema = load_schema(ctx).unwrap_or_default();
    let validated = validate(&parsed, &schema);
    let planned = plan(&validated);

    let hits = ctx
        .index
        .with_connection(|conn| execute(conn, &planned))
        .map_err(|e| format!("execute search: {e}"))?;
    let rich = project_hits(&ctx.index, &hits).map_err(|e| format!("project search hits: {e}"))?;

    let warnings: Vec<String> = validated.warnings.iter().map(ToString::to_string).collect();

    // Successful executions auto-record into the recent-query log.
    // Logging is best-effort: a write failure should not turn a
    // good search into a UI error.
    if let Err(e) = record_history(ctx, &query) {
        tracing::warn!("could not append to search history: {e}");
    }

    Ok(SearchResponse {
        query,
        hits: rich,
        warnings,
        parse_error: None,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn search_history_list(state: State<'_, AppState>) -> Result<Vec<HistoryEntryWire>, String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    let path = ProjectPath::new(SEARCH_HISTORY_PATH).map_err(|e| format!("path: {e}"))?;
    let doc = match load_history(&ctx.fs, &path) {
        Ok(d) => d,
        Err(HistoryError::NotFound) => HistoryDocument::default(),
        Err(e) => return Err(format!("load history: {e}")),
    };
    Ok(doc
        .entries
        .into_iter()
        .map(HistoryEntryWire::from)
        .collect())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn search_history_clear(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.project.lock().expect("project mutex poisoned");
    let ctx = guard.as_ref().ok_or_else(no_project_error)?;
    let path = ProjectPath::new(SEARCH_HISTORY_PATH).map_err(|e| format!("path: {e}"))?;
    let mut doc = match load_history(&ctx.fs, &path) {
        Ok(d) => d,
        Err(HistoryError::NotFound) => HistoryDocument::default(),
        Err(e) => return Err(format!("load history: {e}")),
    };
    history_clear(&mut doc);
    save_history(&ctx.fs, &path, &doc).map_err(|e| format!("save history: {e}"))?;
    Ok(())
}

fn record_history(ctx: &ProjectContext, query: &str) -> Result<(), String> {
    let path = ProjectPath::new(SEARCH_HISTORY_PATH).map_err(|e| format!("path: {e}"))?;
    let mut doc = match load_history(&ctx.fs, &path) {
        Ok(d) => d,
        Err(HistoryError::NotFound) => HistoryDocument::default(),
        Err(e) => return Err(format!("load history: {e}")),
    };
    history_append(&mut doc, query, Utc::now());
    save_history(&ctx.fs, &path, &doc).map_err(|e| format!("save history: {e}"))?;
    Ok(())
}

fn no_project_error() -> String {
    "no_project: launch progest-desktop from inside a Progest project, or set PROGEST_PROJECT"
        .to_string()
}

/// Mirror of `progest-cli`'s schema-loader. Pulled in here so the IPC
/// layer can validate against custom-field types without round-tripping
/// through the CLI binary. Bad TOML silently degrades to "no schema",
/// matching the CLI behavior for parity.
fn load_schema(ctx: &ProjectContext) -> Option<CustomFields> {
    let path = ctx.root.schema_toml();
    let text = std::fs::read_to_string(&path).ok()?;
    parse_schema_toml(&text)
}

fn parse_schema_toml(text: &str) -> Option<CustomFields> {
    #[derive(serde::Deserialize)]
    struct Doc {
        #[serde(default)]
        custom_fields: BTreeMap<String, FieldEntry>,
    }
    #[derive(serde::Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase")]
    enum FieldEntry {
        String,
        Int,
        Enum { values: Vec<String> },
    }
    let doc: Doc = toml::from_str(text).ok()?;
    let mut schema = CustomFields::new();
    for (name, entry) in doc.custom_fields {
        let kind = match entry {
            FieldEntry::String => CustomFieldKind::String,
            FieldEntry::Int => CustomFieldKind::Int,
            FieldEntry::Enum { values } => CustomFieldKind::Enum { values },
        };
        schema.insert(name, kind);
    }
    Some(schema)
}
