use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::types::AiError;

const TIMEOUT: Duration = Duration::from_secs(30);

// ── Trait ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ProviderResponse {
    pub content: String,
    pub model: String,
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    fn send(
        &self,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<ProviderResponse, AiError>;
}

// ── Anthropic ───────────────────────────────────────────────────────

const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 1024;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// # Panics
    ///
    /// Panics if the TLS backend fails to initialize (should not happen
    /// in normal operation).
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(TIMEOUT)
                .build()
                .expect("TLS backend init failed"),
            api_key,
            base_url: base_url.unwrap_or_else(|| ANTHROPIC_BASE.to_string()),
        }
    }
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    model: String,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    text: String,
}

impl Provider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn send(
        &self,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<ProviderResponse, AiError> {
        let body = AnthropicRequest {
            model,
            max_tokens: DEFAULT_MAX_TOKENS,
            system: system_prompt,
            messages: vec![AnthropicMessage {
                role: "user",
                content: user_prompt,
            }],
        };

        let url = format!("{}/v1/messages", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| AiError::HttpError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status != 200 {
            let body = resp.text().unwrap_or_default();
            return Err(AiError::ApiError { status, body });
        }

        let parsed: AnthropicResponse = resp
            .json()
            .map_err(|e| AiError::ParseError(format!("Anthropic response: {e}")))?;

        let text: String = parsed.content.into_iter().map(|b| b.text).collect();

        Ok(ProviderResponse {
            content: text,
            model: parsed.model,
        })
    }
}

// ── OpenAI ──────────────────────────────────────────────────────────

const OPENAI_BASE: &str = "https://api.openai.com";

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    /// # Panics
    ///
    /// Panics if the TLS backend fails to initialize (should not happen
    /// in normal operation).
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(TIMEOUT)
                .build()
                .expect("TLS backend init failed"),
            api_key,
            base_url: base_url.unwrap_or_else(|| OPENAI_BASE.to_string()),
        }
    }
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<OpenAiMessage<'a>>,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: String,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageContent,
}

#[derive(Deserialize)]
struct OpenAiMessageContent {
    content: String,
}

impl Provider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn send(
        &self,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<ProviderResponse, AiError> {
        let body = OpenAiRequest {
            model,
            max_tokens: DEFAULT_MAX_TOKENS,
            messages: vec![
                OpenAiMessage {
                    role: "system",
                    content: system_prompt,
                },
                OpenAiMessage {
                    role: "user",
                    content: user_prompt,
                },
            ],
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| AiError::HttpError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status != 200 {
            let body = resp.text().unwrap_or_default();
            return Err(AiError::ApiError { status, body });
        }

        let parsed: OpenAiResponse = resp
            .json()
            .map_err(|e| AiError::ParseError(format!("OpenAI response: {e}")))?;

        let text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        Ok(ProviderResponse {
            content: text,
            model: parsed.model,
        })
    }
}

// ── Mock (test) ─────────────────────────────────────────────────────

#[cfg(test)]
pub struct MockProvider {
    pub response: Result<ProviderResponse, AiError>,
}

#[cfg(test)]
impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn send(
        &self,
        _model: &str,
        _system_prompt: &str,
        _user_prompt: &str,
    ) -> Result<ProviderResponse, AiError> {
        match &self.response {
            Ok(r) => Ok(ProviderResponse {
                content: r.content.clone(),
                model: r.model.clone(),
            }),
            Err(e) => Err(AiError::HttpError(format!("{e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_request_body_structure() {
        let body = AnthropicRequest {
            model: "claude-sonnet-4-20250514",
            max_tokens: 1024,
            system: "You are a helper.",
            messages: vec![AnthropicMessage {
                role: "user",
                content: "Hello",
            }],
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "claude-sonnet-4-20250514");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["system"], "You are a helper.");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
    }

    #[test]
    fn openai_request_body_structure() {
        let body = OpenAiRequest {
            model: "gpt-4.1-mini",
            max_tokens: 1024,
            messages: vec![
                OpenAiMessage {
                    role: "system",
                    content: "You are a helper.",
                },
                OpenAiMessage {
                    role: "user",
                    content: "Hello",
                },
            ],
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "gpt-4.1-mini");
        assert_eq!(json["messages"].as_array().unwrap().len(), 2);
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["role"], "user");
    }

    #[test]
    fn anthropic_response_parsing() {
        let raw = r#"{
            "content": [{"type": "text", "text": "[{\"value\": \"test.psd\"}]"}],
            "model": "claude-sonnet-4-20250514",
            "role": "assistant"
        }"#;
        let parsed: AnthropicResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.content.len(), 1);
        assert!(parsed.content[0].text.contains("test.psd"));
        assert_eq!(parsed.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn openai_response_parsing() {
        let raw = r#"{
            "choices": [{
                "message": {"role": "assistant", "content": "[{\"value\": \"test.psd\"}]"},
                "finish_reason": "stop"
            }],
            "model": "gpt-4.1-mini"
        }"#;
        let parsed: OpenAiResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        assert!(parsed.choices[0].message.content.contains("test.psd"));
        assert_eq!(parsed.model, "gpt-4.1-mini");
    }

    #[test]
    fn mock_provider_returns_canned_response() {
        let mock = MockProvider {
            response: Ok(ProviderResponse {
                content: r#"[{"value":"foo.psd"}]"#.into(),
                model: "mock-v1".into(),
            }),
        };
        let resp = mock.send("m", "sys", "user").unwrap();
        assert_eq!(resp.content, r#"[{"value":"foo.psd"}]"#);
    }

    #[test]
    fn mock_provider_returns_error() {
        let mock = MockProvider {
            response: Err(AiError::HttpError("timeout".into())),
        };
        let err = mock.send("m", "sys", "user").unwrap_err();
        assert!(matches!(err, AiError::HttpError(_)));
    }
}
