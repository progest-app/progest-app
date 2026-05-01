use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use super::types::{AiError, AiProvider, AiSuggestion, SuggestionType};

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub suggestion_type: SuggestionType,
    pub provider: AiProvider,
    pub model: String,
    pub file_path: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub response_text: String,
    pub suggestions: Vec<AiSuggestion>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

const LOG_FILE: &str = "ai-log.jsonl";

/// Append an audit entry to `.progest/local/ai-log.jsonl`.
///
/// Best-effort: I/O failures are returned but callers should swallow
/// them to avoid blocking the suggestion flow.
pub fn log_entry(local_dir: &Path, entry: &AuditEntry) -> Result<(), AiError> {
    let path = local_dir.join(LOG_FILE);
    let line = serde_json::to_string(entry).map_err(|e| AiError::AuditError(e.to_string()))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| AiError::AuditError(format!("{}: {e}", path.display())))?;

    writeln!(file, "{line}").map_err(|e| AiError::AuditError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::BufRead;

    use super::*;

    #[test]
    fn writes_jsonl_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let entry = AuditEntry {
            timestamp: "2026-05-01T12:00:00Z".into(),
            suggestion_type: SuggestionType::Naming,
            provider: AiProvider::Anthropic,
            model: "claude-sonnet-4-20250514".into(),
            file_path: "shots/sh010/hero.psd".into(),
            system_prompt: "You are a naming assistant.".into(),
            user_prompt: "Suggest a name.".into(),
            response_text: r#"[{"value":"hero_comp_v01.psd"}]"#.into(),
            suggestions: vec![AiSuggestion {
                value: "hero_comp_v01.psd".into(),
                explanation: None,
            }],
            elapsed_ms: 1200,
            error: None,
        };

        log_entry(dir.path(), &entry).unwrap();
        log_entry(dir.path(), &entry).unwrap();

        let file = std::fs::File::open(dir.path().join(LOG_FILE)).unwrap();
        let lines: Vec<String> = std::io::BufReader::new(file)
            .lines()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(lines.len(), 2);

        let parsed: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(parsed["file_path"], "shots/sh010/hero.psd");
        assert_eq!(parsed["elapsed_ms"], 1200);
        assert_eq!(parsed["suggestions"][0]["value"], "hero_comp_v01.psd");
    }

    #[test]
    fn missing_dir_returns_error() {
        let result = log_entry(
            Path::new("/nonexistent/path"),
            &AuditEntry {
                timestamp: String::new(),
                suggestion_type: SuggestionType::Tags,
                provider: AiProvider::OpenAi,
                model: String::new(),
                file_path: String::new(),
                system_prompt: String::new(),
                user_prompt: String::new(),
                response_text: String::new(),
                suggestions: Vec::new(),
                elapsed_ms: 0,
                error: None,
            },
        );
        assert!(result.is_err());
    }
}
