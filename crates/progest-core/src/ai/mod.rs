pub mod audit;
pub mod context;
pub mod keychain;
pub mod loader;
pub mod prompt;
pub mod provider;
pub mod types;

use std::path::Path;

use crate::fs::ProjectPath;
use crate::index::Index;
use crate::meta::MetaStore;

pub use keychain::{delete_api_key, get_api_key, has_api_key, store_api_key};
pub use loader::{AiConfigError, AiConfigWarning, extract_ai_config};
pub use types::*;

/// Main entry point for AI suggestions.
///
/// Fetches the API key from the OS keychain, gathers context, builds
/// prompts, calls the provider, parses the response, and writes an
/// audit log entry.
#[allow(clippy::too_many_arguments)]
pub fn suggest(
    file_path: &ProjectPath,
    suggestion_type: SuggestionType,
    privacy: &PrivacyFlags,
    project_root: &Path,
    index: &dyn Index,
    meta_store: &dyn MetaStore,
    config: &AiConfig,
    local_dir: &Path,
) -> Result<AiResponse, AiError> {
    let api_key = keychain::get_api_key(config.provider)?;
    let provider_impl: Box<dyn provider::Provider> = match config.provider {
        AiProvider::Anthropic => Box::new(provider::AnthropicProvider::new(api_key, None)),
        AiProvider::OpenAi => Box::new(provider::OpenAiProvider::new(api_key, None)),
    };
    run_suggest(
        file_path,
        suggestion_type,
        privacy,
        project_root,
        index,
        meta_store,
        config,
        local_dir,
        &*provider_impl,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_suggest(
    file_path: &ProjectPath,
    suggestion_type: SuggestionType,
    privacy: &PrivacyFlags,
    project_root: &Path,
    index: &dyn Index,
    meta_store: &dyn MetaStore,
    config: &AiConfig,
    local_dir: &Path,
    provider_impl: &dyn provider::Provider,
) -> Result<AiResponse, AiError> {
    let ctx = context::gather_context(
        file_path,
        suggestion_type,
        privacy,
        project_root,
        index,
        meta_store,
        config,
    )?;

    let (system_prompt, user_prompt) = prompt::build_prompt(suggestion_type, &ctx);

    let start = std::time::Instant::now();
    let raw_result = provider_impl.send(&config.model, &system_prompt, &user_prompt);
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    let response = match &raw_result {
        Ok(resp) => {
            let suggestions = parse_suggestions(&resp.content)?;
            Ok(AiResponse {
                suggestion_type,
                suggestions,
                model: resp.model.clone(),
                provider: config.provider,
                elapsed_ms,
            })
        }
        Err(e) => Err(AiError::HttpError(e.to_string())),
    };

    if config.audit_log {
        let entry = audit::AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            suggestion_type,
            provider: config.provider,
            model: config.model.clone(),
            file_path: file_path.as_str().to_string(),
            system_prompt,
            user_prompt,
            response_text: raw_result
                .as_ref()
                .map_or_else(ToString::to_string, |r| r.content.clone()),
            suggestions: response
                .as_ref()
                .map_or_else(|_| Vec::new(), |r| r.suggestions.clone()),
            elapsed_ms,
            error: response.as_ref().err().map(ToString::to_string),
        };
        if let Err(e) = audit::log_entry(local_dir, &entry) {
            tracing::warn!("AI audit log write failed: {e}");
        }
    }

    response
}

/// Parse the AI's text response into structured suggestions.
///
/// Handles:
/// - Bare JSON array
/// - Array wrapped in markdown code fences
/// - JSON object with a `"suggestions"` key
#[derive(serde::Deserialize)]
struct SuggestionsWrapper {
    suggestions: Vec<AiSuggestion>,
}

fn parse_suggestions(raw: &str) -> Result<Vec<AiSuggestion>, AiError> {
    let trimmed = strip_code_fences(raw.trim());

    if let Ok(arr) = serde_json::from_str::<Vec<AiSuggestion>>(trimmed) {
        return Ok(arr);
    }

    if let Ok(wrapper) = serde_json::from_str::<SuggestionsWrapper>(trimmed) {
        return Ok(wrapper.suggestions);
    }

    Err(AiError::ParseError(format!(
        "could not parse AI response as JSON suggestions: {trimmed}"
    )))
}

fn strip_code_fences(s: &str) -> &str {
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::StdFileSystem;
    use crate::index::SqliteIndex;
    use crate::meta::StdMetaStore;
    use provider::{MockProvider, ProviderResponse};

    #[test]
    fn parse_bare_json_array() {
        let raw = r#"[{"value": "hero_comp_v01.psd", "explanation": "follows convention"}]"#;
        let suggestions = parse_suggestions(raw).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].value, "hero_comp_v01.psd");
    }

    #[test]
    fn parse_fenced_json() {
        let raw = "```json\n[{\"value\": \"test.psd\"}]\n```";
        let suggestions = parse_suggestions(raw).unwrap();
        assert_eq!(suggestions.len(), 1);
    }

    #[test]
    fn parse_wrapper_object() {
        let raw = r#"{"suggestions": [{"value": "a.psd"}, {"value": "b.psd"}]}"#;
        let suggestions = parse_suggestions(raw).unwrap();
        assert_eq!(suggestions.len(), 2);
    }

    #[test]
    fn parse_garbage_returns_error() {
        let result = parse_suggestions("this is not json");
        assert!(result.is_err());
    }

    #[test]
    fn suggest_with_mock_provider() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let local = root.join(".progest/local");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(root.join("test.psd"), b"").unwrap();

        let index = SqliteIndex::open_in_memory().unwrap();
        let meta_store = StdMetaStore::new(StdFileSystem::new(root.to_path_buf()));
        let config = AiConfig::default();
        let privacy = PrivacyFlags::default();
        let file_path = ProjectPath::new("test.psd").unwrap();

        let mock = MockProvider {
            response: Ok(ProviderResponse {
                content: r#"[{"value": "test_renamed.psd", "explanation": "better name"}]"#.into(),
                model: "mock-v1".into(),
            }),
        };

        let response = run_suggest(
            &file_path,
            SuggestionType::Naming,
            &privacy,
            root,
            &index,
            &meta_store,
            &config,
            &local,
            &mock,
        )
        .unwrap();

        assert_eq!(response.suggestions.len(), 1);
        assert_eq!(response.suggestions[0].value, "test_renamed.psd");
        assert_eq!(response.provider, AiProvider::Anthropic);

        // Verify audit log was written
        let log_path = local.join("ai-log.jsonl");
        assert!(log_path.exists());
        let content = std::fs::read_to_string(log_path).unwrap();
        assert!(content.contains("test_renamed.psd"));
    }

    #[test]
    fn suggest_with_provider_error_writes_audit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let local = root.join(".progest/local");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(root.join("test.psd"), b"").unwrap();

        let index = SqliteIndex::open_in_memory().unwrap();
        let meta_store = StdMetaStore::new(StdFileSystem::new(root.to_path_buf()));
        let config = AiConfig::default();
        let privacy = PrivacyFlags::default();
        let file_path = ProjectPath::new("test.psd").unwrap();

        let mock = MockProvider {
            response: Err(AiError::ApiError {
                status: 429,
                body: "rate limited".into(),
            }),
        };

        let result = run_suggest(
            &file_path,
            SuggestionType::Tags,
            &privacy,
            root,
            &index,
            &meta_store,
            &config,
            &local,
            &mock,
        );

        assert!(result.is_err());

        let log_path = local.join("ai-log.jsonl");
        assert!(log_path.exists());
        let content = std::fs::read_to_string(log_path).unwrap();
        assert!(content.contains("\"error\""));
    }
}
