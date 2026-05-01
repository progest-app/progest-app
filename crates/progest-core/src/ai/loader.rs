use toml::{Table, Value};

use super::types::{AiConfig, AiProvider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiConfigWarning {
    UnknownKey(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiConfigError {
    #[error("invalid provider `{0}`: expected \"anthropic\" or \"openai\"")]
    InvalidProvider(String),

    #[error("wrong type for key `{key}`: expected {expected}")]
    WrongType { key: String, expected: String },
}

const KNOWN_KEYS: &[&str] = &["provider", "model", "audit_log", "glossary"];

/// Parse `[ai]` from [`ProjectDocument::extra`].
///
/// Returns `Ok((default, []))` when the section is absent.
pub fn extract_ai_config(extra: &Table) -> Result<(AiConfig, Vec<AiConfigWarning>), AiConfigError> {
    let Some(Value::Table(ai)) = extra.get("ai") else {
        return Ok((AiConfig::default(), Vec::new()));
    };

    let mut warnings = Vec::new();

    for key in ai.keys() {
        if !KNOWN_KEYS.contains(&key.as_str()) {
            warnings.push(AiConfigWarning::UnknownKey(key.clone()));
        }
    }

    let provider = match ai.get("provider") {
        Some(Value::String(s)) => match s.as_str() {
            "anthropic" => AiProvider::Anthropic,
            "openai" => AiProvider::OpenAi,
            other => return Err(AiConfigError::InvalidProvider(other.to_string())),
        },
        Some(_) => {
            return Err(AiConfigError::WrongType {
                key: "provider".into(),
                expected: "string".into(),
            });
        }
        None => AiProvider::Anthropic,
    };

    let model = match ai.get("model") {
        Some(Value::String(s)) => s.clone(),
        Some(_) => {
            return Err(AiConfigError::WrongType {
                key: "model".into(),
                expected: "string".into(),
            });
        }
        None => provider.default_model().to_string(),
    };

    let audit_log = match ai.get("audit_log") {
        Some(Value::Boolean(b)) => *b,
        Some(_) => {
            return Err(AiConfigError::WrongType {
                key: "audit_log".into(),
                expected: "boolean".into(),
            });
        }
        None => true,
    };

    let glossary = match ai.get("glossary") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    _ => {
                        return Err(AiConfigError::WrongType {
                            key: "glossary".into(),
                            expected: "array of strings".into(),
                        });
                    }
                }
            }
            out
        }
        Some(_) => {
            return Err(AiConfigError::WrongType {
                key: "glossary".into(),
                expected: "array of strings".into(),
            });
        }
        None => Vec::new(),
    };

    Ok((
        AiConfig {
            provider,
            model,
            audit_log,
            glossary,
        },
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(toml_str: &str) -> Table {
        toml_str.parse::<Table>().unwrap()
    }

    #[test]
    fn absent_section_returns_defaults() {
        let (cfg, warns) = extract_ai_config(&Table::new()).unwrap();
        assert_eq!(cfg, AiConfig::default());
        assert!(warns.is_empty());
    }

    #[test]
    fn full_section_round_trips() {
        let t = table(
            r#"
[ai]
provider = "openai"
model = "gpt-4.1"
audit_log = false
glossary = ["VFX", "comp"]
"#,
        );
        let (cfg, warns) = extract_ai_config(&t).unwrap();
        assert!(warns.is_empty());
        assert_eq!(cfg.provider, AiProvider::OpenAi);
        assert_eq!(cfg.model, "gpt-4.1");
        assert!(!cfg.audit_log);
        assert_eq!(cfg.glossary, vec!["VFX", "comp"]);
    }

    #[test]
    fn unknown_keys_produce_warnings() {
        let t = table(
            r#"
[ai]
provider = "anthropic"
future_field = 42
"#,
        );
        let (_, warns) = extract_ai_config(&t).unwrap();
        assert_eq!(
            warns,
            vec![AiConfigWarning::UnknownKey("future_field".into())]
        );
    }

    #[test]
    fn invalid_provider_is_error() {
        let t = table(
            r#"
[ai]
provider = "gemini"
"#,
        );
        let err = extract_ai_config(&t).unwrap_err();
        assert!(matches!(err, AiConfigError::InvalidProvider(s) if s == "gemini"));
    }

    #[test]
    fn wrong_type_is_error() {
        let t = table(
            r"
[ai]
provider = 42
",
        );
        let err = extract_ai_config(&t).unwrap_err();
        assert!(matches!(err, AiConfigError::WrongType { key, .. } if key == "provider"));
    }

    #[test]
    fn missing_provider_defaults_to_anthropic() {
        let t = table(
            r#"
[ai]
model = "custom-model"
"#,
        );
        let (cfg, _) = extract_ai_config(&t).unwrap();
        assert_eq!(cfg.provider, AiProvider::Anthropic);
        assert_eq!(cfg.model, "custom-model");
    }

    #[test]
    fn glossary_wrong_element_type() {
        let t = table(
            r"
[ai]
glossary = [1, 2, 3]
",
        );
        let err = extract_ai_config(&t).unwrap_err();
        assert!(matches!(err, AiConfigError::WrongType { key, .. } if key == "glossary"));
    }

    #[test]
    fn audit_log_wrong_type() {
        let t = table(
            r#"
[ai]
audit_log = "yes"
"#,
        );
        let err = extract_ai_config(&t).unwrap_err();
        assert!(matches!(err, AiConfigError::WrongType { key, .. } if key == "audit_log"));
    }
}
