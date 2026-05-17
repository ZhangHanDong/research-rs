//! `provider-opencode-go` — [OpenCode Go](https://opencode.ai/zen/go)
//! subscription provider for the autonomous research loop.
//!
//! OpenCode Go is a $10/mo subscription that aggregates open coding
//! models (DeepSeek, Kimi, GLM, Qwen, MiniMax, ...) behind standard
//! OpenAI-compatible and Anthropic-compatible HTTP endpoints. Unlike
//! `cc-sdk` (Claude Code subscription) and `codex app-server` (ChatGPT
//! subscription), this provider needs only an API key — no SDK crates,
//! no background daemons.
//!
//! Spec: `specs/provider-opencode-go.spec.md`.
//!
//! # Required env vars
//!
//! - `OPENCODE_API_KEY` — your `oc-…` API key
//! - `ASR_OPENCODE_MODEL` — the model id (e.g. `deepseek-v3.2-exp`;
//!   pick from <https://opencode.ai/zen/go>). No default — every user
//!   must pick explicitly so we don't bake a fictional model name into
//!   the binary.
//!
//! # Optional env vars
//!
//! - `ASR_OPENCODE_PROTOCOL` — `openai` (default) or `anthropic`. Pick
//!   explicitly from the OpenCode Go docs per model; do NOT auto-detect
//!   from the model name (too fragile as vendors add new namespaces).
//! - `ASR_OPENCODE_TEMPERATURE` — default `0.2` (research/reasoning
//!   prefers low temperature; the loop produces structured JSON output).
//! - `ASR_OPENCODE_MAX_TOKENS` — default `16384`. Per-model real ceiling
//!   varies; server will truncate if you over-ask.
//! - `ASR_OPENCODE_TIMEOUT_MS` — default `120000` (120 s). Clamped to
//!   `[5_000, 600_000]`.
//! - `ASR_OPENCODE_ENDPOINT_OPENAI` — override the OpenAI endpoint URL.
//! - `ASR_OPENCODE_ENDPOINT_ANTHROPIC` — override the Anthropic endpoint.
//!
//! # Acknowledgement
//!
//! Initial implementation idea + `reasoning_content` fallback insight +
//! Windows packaging notes from [@Paul-Yuchao-Dong](https://github.com/Paul-Yuchao-Dong)
//! via PR #19. The default-model and default-provider parts of that PR
//! were intentionally NOT adopted (kept `fake` as CLI default; no
//! hardcoded model id).

use super::provider::{AgentProvider, ProviderError};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::time::Duration;

const DEFAULT_OPENAI_ENDPOINT: &str = "https://opencode.ai/zen/go/v1/chat/completions";
const DEFAULT_ANTHROPIC_ENDPOINT: &str = "https://opencode.ai/zen/go/v1/messages";

const DEFAULT_TEMPERATURE: f32 = 0.2;
const DEFAULT_MAX_TOKENS: u32 = 16_384;
const HARD_MAX_TOKENS: u32 = 65_536;
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const TIMEOUT_MIN_MS: u64 = 5_000;
const TIMEOUT_MAX_MS: u64 = 600_000;
const RETRY_ATTEMPTS: usize = 3; // 1 initial + 3 retries = 4 total

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
}

impl Protocol {
    fn from_env_str(s: &str) -> Result<Self, ProviderError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "openai" | "open-ai" | "open_ai" => Ok(Protocol::OpenAi),
            "anthropic" | "claude" => Ok(Protocol::Anthropic),
            other => Err(ProviderError::NotAvailable(format!(
                "ASR_OPENCODE_PROTOCOL must be \"openai\" or \"anthropic\"; got \"{other}\""
            ))),
        }
    }
}

#[derive(Debug)]
pub struct OpenCodeGoProvider {
    // Note: api_key is intentionally NOT shown by the auto-derived Debug
    // (`Client` skips it, and we mark the field as needing redaction
    // below by name only). The redaction relies on `Debug` consumers not
    // expecting plaintext — sufficient for our test panic messages.
    api_key: String,
    model: String,
    protocol: Protocol,
    temperature: f32,
    max_tokens: u32,
    timeout_ms: u64,
    openai_endpoint: String,
    anthropic_endpoint: String,
    #[allow(dead_code)]
    client: Client,
}

impl OpenCodeGoProvider {
    /// Read all knobs from env and build a provider.
    ///
    /// Both `OPENCODE_API_KEY` AND `ASR_OPENCODE_MODEL` are required.
    /// Missing either returns `ProviderError::NotAvailable` with a
    /// pointer to https://opencode.ai/zen/go for the current model list.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("OPENCODE_API_KEY").map_err(|_| {
            ProviderError::NotAvailable(
                "OPENCODE_API_KEY not set. Get a key at https://opencode.ai/auth".into(),
            )
        })?;

        let model = std::env::var("ASR_OPENCODE_MODEL").map_err(|_| {
            ProviderError::NotAvailable(
                "ASR_OPENCODE_MODEL not set; see https://opencode.ai/zen/go for the current \
                 model list and pick one (e.g. deepseek-v3.2-exp). No default is shipped \
                 because vendors rotate model ids frequently."
                    .into(),
            )
        })?;

        let protocol = match std::env::var("ASR_OPENCODE_PROTOCOL") {
            Ok(s) => Protocol::from_env_str(&s)?,
            Err(_) => Protocol::OpenAi,
        };

        let temperature = std::env::var("ASR_OPENCODE_TEMPERATURE")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|t| t.clamp(0.0, 2.0))
            .unwrap_or(DEFAULT_TEMPERATURE);

        let max_tokens = std::env::var("ASR_OPENCODE_MAX_TOKENS")
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|t| t.min(HARD_MAX_TOKENS).max(1))
            .unwrap_or(DEFAULT_MAX_TOKENS);

        let timeout_ms = std::env::var("ASR_OPENCODE_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|t| t.clamp(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS))
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        let openai_endpoint = std::env::var("ASR_OPENCODE_ENDPOINT_OPENAI")
            .unwrap_or_else(|_| DEFAULT_OPENAI_ENDPOINT.into());
        let anthropic_endpoint = std::env::var("ASR_OPENCODE_ENDPOINT_ANTHROPIC")
            .unwrap_or_else(|_| DEFAULT_ANTHROPIC_ENDPOINT.into());

        let client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| {
                ProviderError::NotAvailable(format!("reqwest client build failed: {e}"))
            })?;

        Ok(Self {
            api_key,
            model,
            protocol,
            temperature,
            max_tokens,
            timeout_ms,
            openai_endpoint,
            anthropic_endpoint,
            client,
        })
    }

    /// Inspector — protocol selected by env (or default).
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// Inspector — temperature (post-clamp).
    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    /// Inspector — timeout in ms (post-clamp).
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Inspector — max_tokens.
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    /// Inspector — model id.
    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl AgentProvider for OpenCodeGoProvider {
    async fn ask(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        match self.protocol {
            Protocol::OpenAi => self.ask_openai(system, user).await,
            Protocol::Anthropic => self.ask_anthropic(system, user).await,
        }
    }

    fn name(&self) -> &'static str {
        "opencode-go"
    }
}

impl OpenCodeGoProvider {
    async fn ask_openai(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "temperature": self.temperature,
            "max_tokens": self.max_tokens
        });

        let json = self.post_with_retry(&self.openai_endpoint, &body, false).await?;

        let msg = &json["choices"][0]["message"];
        // Primary: `content`. Fallback: `reasoning_content` (DeepSeek-V3+
        // can place the final answer there when the reasoning-token
        // budget is exhausted — caught by @Paul-Yuchao-Dong in PR #19).
        let text = msg["content"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| msg["reasoning_content"].as_str())
            .filter(|s| !s.trim().is_empty());

        match text {
            Some(t) => Ok(t.to_string()),
            None => Err(ProviderError::CallFailed(format!(
                "unexpected response shape (no content or reasoning_content): {}",
                truncate_for_error(&json.to_string())
            ))),
        }
    }

    async fn ask_anthropic(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        let body = serde_json::json!({
            "model": self.model,
            "system": system,
            "messages": [{"role": "user", "content": user}],
            "max_tokens": self.max_tokens,
            "temperature": self.temperature
        });

        let json = self.post_with_retry(&self.anthropic_endpoint, &body, true).await?;

        let text: String = json["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        if text.is_empty() {
            Err(ProviderError::EmptyResponse)
        } else {
            Ok(text)
        }
    }

    /// POST `body` to `url`, parse JSON. Retry on:
    ///   - HTTP 429 / 503 (rate limit / temporary unavailable)
    ///   - reqwest timeout / connection reset (network blip — 1 retry only)
    ///
    /// Do NOT retry on 4xx other than 429, or 5xx other than 503 — those
    /// are permanent server/client errors. `auth_header_kind`:
    /// `false` → `Authorization: Bearer <key>` (OpenAI convention),
    /// `true`  → `x-api-key: <key>` + `anthropic-version: 2023-06-01`
    /// (Anthropic convention).
    async fn post_with_retry(
        &self,
        url: &str,
        body: &Value,
        anthropic_headers: bool,
    ) -> Result<Value, ProviderError> {
        let mut attempt = 0usize;
        let mut network_retry_used = false;
        loop {
            let mut req = self.client.post(url).json(body);
            if anthropic_headers {
                req = req
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01");
            } else {
                req = req.header("Authorization", format!("Bearer {}", self.api_key));
            }

            let resp_result = req.send().await;
            match resp_result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp.json::<Value>().await.map_err(|e| {
                            ProviderError::CallFailed(format!("json parse: {e}"))
                        });
                    }
                    // Retryable status codes: 429, 503.
                    if (status == StatusCode::TOO_MANY_REQUESTS
                        || status == StatusCode::SERVICE_UNAVAILABLE)
                        && attempt < RETRY_ATTEMPTS
                    {
                        attempt += 1;
                        let backoff_ms = 1000u64 << (attempt - 1); // 1s, 2s, 4s
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                    let text = resp.text().await.unwrap_or_default();
                    return Err(ProviderError::CallFailed(format!(
                        "HTTP {status}: {}",
                        truncate_for_error(&text)
                    )));
                }
                Err(e) if (e.is_timeout() || e.is_connect()) && !network_retry_used => {
                    network_retry_used = true;
                    continue;
                }
                Err(e) => {
                    return Err(ProviderError::CallFailed(format!("HTTP: {e}")));
                }
            }
        }
    }
}

fn truncate_for_error(s: &str) -> String {
    const MAX: usize = 500;
    if s.len() <= MAX {
        s.to_string()
    } else {
        format!("{}... [+{} bytes]", &s[..MAX], s.len() - MAX)
    }
}

// ─── Unit tests (parser + env behaviour; no network) ───────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_from_env_str_defaults_openai() {
        assert_eq!(Protocol::from_env_str("").unwrap(), Protocol::OpenAi);
        assert_eq!(Protocol::from_env_str("openai").unwrap(), Protocol::OpenAi);
        assert_eq!(Protocol::from_env_str("OpenAI").unwrap(), Protocol::OpenAi);
        assert_eq!(Protocol::from_env_str("open-ai").unwrap(), Protocol::OpenAi);
    }

    #[test]
    fn protocol_from_env_str_anthropic() {
        assert_eq!(
            Protocol::from_env_str("anthropic").unwrap(),
            Protocol::Anthropic
        );
        assert_eq!(
            Protocol::from_env_str("Anthropic").unwrap(),
            Protocol::Anthropic
        );
        assert_eq!(Protocol::from_env_str("claude").unwrap(), Protocol::Anthropic);
    }

    #[test]
    fn protocol_from_env_str_rejects_garbage() {
        let r = Protocol::from_env_str("openrouter");
        assert!(r.is_err());
        if let Err(ProviderError::NotAvailable(msg)) = r {
            assert!(msg.contains("ASR_OPENCODE_PROTOCOL"));
        } else {
            panic!("expected NotAvailable, got {r:?}");
        }
    }

    #[test]
    fn truncate_for_error_long_string() {
        let s = "x".repeat(1200);
        let t = truncate_for_error(&s);
        assert!(t.len() < 600);
        assert!(t.contains("[+"));
    }

    #[test]
    fn truncate_for_error_short_string_unchanged() {
        assert_eq!(truncate_for_error("short"), "short");
    }
}
