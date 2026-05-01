use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Provider ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    Anthropic,
    #[serde(alias = "openai")]
    OpenAi,
}

impl AiProvider {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
        }
    }

    #[must_use]
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-sonnet-4-20250514",
            Self::OpenAi => "gpt-4.1-mini",
        }
    }
}

impl fmt::Display for AiProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiConfig {
    pub provider: AiProvider,
    pub model: String,
    pub audit_log: bool,
    pub glossary: Vec<String>,
}

impl Default for AiConfig {
    fn default() -> Self {
        let provider = AiProvider::Anthropic;
        Self {
            model: provider.default_model().to_string(),
            provider,
            audit_log: true,
            glossary: Vec::new(),
        }
    }
}

// ── Suggestion type ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionType {
    Naming,
    Tags,
    Notes,
    Placement,
}

impl fmt::Display for SuggestionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Naming => f.write_str("naming"),
            Self::Tags => f.write_str("tags"),
            Self::Notes => f.write_str("notes"),
            Self::Placement => f.write_str("placement"),
        }
    }
}

// ── Privacy flags ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PrivacyFlags {
    pub include_notes: bool,
}

// ── Context ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct AiContext {
    pub file_name: String,
    pub file_extension: Option<String>,
    pub parent_dir: String,
    pub sibling_names: Vec<String>,
    pub existing_tags: Vec<String>,
    pub project_tag_vocabulary: Vec<String>,
    pub notes_body: Option<String>,
    pub rule_summaries: Vec<String>,
    pub project_dirs: Vec<String>,
    pub dir_accepts: Vec<(String, Vec<String>)>,
    pub glossary: Vec<String>,
}

// ── Suggestion / Response ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSuggestion {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub suggestion_type: SuggestionType,
    pub suggestions: Vec<AiSuggestion>,
    pub model: String,
    pub provider: AiProvider,
    pub elapsed_ms: u64,
}

// ── Error ───────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI not configured: set [ai] in project.toml")]
    NotConfigured,

    #[error("no API key found for provider `{provider}`")]
    NoApiKey { provider: String },

    #[error("keychain error: {0}")]
    KeychainError(String),

    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("API returned error {status}: {body}")]
    ApiError { status: u16, body: String },

    #[error("failed to parse API response: {0}")]
    ParseError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("audit log I/O: {0}")]
    AuditError(String),

    #[error("context error: {0}")]
    ContextError(String),
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_serde_round_trip() {
        for (provider, expected) in [
            (AiProvider::Anthropic, r#""anthropic""#),
            (AiProvider::OpenAi, r#""openai""#),
        ] {
            let json = serde_json::to_string(&provider).unwrap();
            assert_eq!(json, expected);
            let back: AiProvider = serde_json::from_str(&json).unwrap();
            assert_eq!(back, provider);
        }
    }

    #[test]
    fn provider_openai_alias() {
        let p: AiProvider = serde_json::from_str(r#""openai""#).unwrap();
        assert_eq!(p, AiProvider::OpenAi);
    }

    #[test]
    fn suggestion_type_serde_round_trip() {
        for ty in [
            SuggestionType::Naming,
            SuggestionType::Tags,
            SuggestionType::Notes,
            SuggestionType::Placement,
        ] {
            let json = serde_json::to_string(&ty).unwrap();
            let back: SuggestionType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn suggestion_serde_optional_explanation() {
        let s = AiSuggestion {
            value: "foo_bar.psd".into(),
            explanation: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("explanation"));

        let s2 = AiSuggestion {
            value: "foo_bar.psd".into(),
            explanation: Some("follows snake_case".into()),
        };
        let json2 = serde_json::to_string(&s2).unwrap();
        assert!(json2.contains("explanation"));
        let back: AiSuggestion = serde_json::from_str(&json2).unwrap();
        assert_eq!(back, s2);
    }

    #[test]
    fn default_config() {
        let cfg = AiConfig::default();
        assert_eq!(cfg.provider, AiProvider::Anthropic);
        assert_eq!(cfg.model, "claude-sonnet-4-20250514");
        assert!(cfg.audit_log);
        assert!(cfg.glossary.is_empty());
    }

    #[test]
    fn provider_default_models() {
        assert_eq!(
            AiProvider::Anthropic.default_model(),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(AiProvider::OpenAi.default_model(), "gpt-4.1-mini");
    }
}
