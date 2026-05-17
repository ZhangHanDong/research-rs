//! Integration tests for `specs/provider-opencode-go.spec.md`.
//!
//! All HTTP tests use an **in-process TcpListener mock** (no `wiremock`
//! dev-dep — mirrors the `McpMock` pattern in `tests/composite_fetch.rs`)
//! so the test suite stays network-free and reproducible.
//!
//! Coverage map (spec scenario → test name):
//!
//!  1. from_env_missing_key_returns_not_available
//!  2. from_env_missing_model_returns_not_available
//!  3. from_env_with_all_required_returns_ok
//!  4. protocol_defaults_to_openai
//!  5. protocol_env_anthropic_routes_to_anthropic
//!  6. timeout_env_clamped_to_range
//!  7. temperature_parse_fail_falls_back_to_default
//!  8. openai_200_returns_content
//!  9. openai_empty_content_falls_back_to_reasoning_content
//! 10. openai_both_empty_returns_call_failed
//! 11. http_401_does_not_retry
//! 12. http_429_retries_up_to_3_times
//! 13. http_429_then_200_succeeds
//! 14. http_500_does_not_retry
//! 15. http_503_retries
//! 16. anthropic_200_returns_joined_text_blocks
//! 17. anthropic_empty_content_returns_empty_response
//! 18. name_is_opencode_go
//!
//! Note: tests #6/#7/#11-#15 mutate env vars; we use a process-level
//! `Mutex` (ENV_LOCK) to serialize them — std::env::set_var/remove_var
//! are global state, not thread-safe under `cargo test`'s default parallel
//! runner.

#![cfg(feature = "provider-opencode-go")]

use research::autoresearch::opencode_go::{OpenCodeGoProvider, Protocol};
use research::autoresearch::provider::{AgentProvider, ProviderError};

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

// Single process-wide lock so env-mutating tests don't interleave with each
// other or with the parallel test runner.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Clear every env var the provider reads so each test starts from a known
/// state. Call inside the env-lock guard.
fn clear_opencode_env() {
    for k in [
        "OPENCODE_API_KEY",
        "ASR_OPENCODE_MODEL",
        "ASR_OPENCODE_PROTOCOL",
        "ASR_OPENCODE_TEMPERATURE",
        "ASR_OPENCODE_MAX_TOKENS",
        "ASR_OPENCODE_TIMEOUT_MS",
        "ASR_OPENCODE_ENDPOINT_OPENAI",
        "ASR_OPENCODE_ENDPOINT_ANTHROPIC",
    ] {
        // Safety: serialized by env_lock(); no other thread reads/writes
        // these vars while we hold the guard.
        unsafe { std::env::remove_var(k) };
    }
}

fn setenv(k: &str, v: &str) {
    unsafe { std::env::set_var(k, v) };
}

// ═════════════════════════════════════════════════════════════════════════
// Spec scenarios 1-7: env parsing (no network)
// ═════════════════════════════════════════════════════════════════════════

#[test]
fn from_env_missing_key_returns_not_available() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    let r = OpenCodeGoProvider::from_env();
    match r {
        Err(ProviderError::NotAvailable(msg)) => assert!(
            msg.contains("OPENCODE_API_KEY"),
            "expected msg to mention OPENCODE_API_KEY; got: {msg}"
        ),
        other => panic!("expected NotAvailable, got {other:?}"),
    }
}

#[test]
fn from_env_missing_model_returns_not_available() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "test-key");
    let r = OpenCodeGoProvider::from_env();
    match r {
        Err(ProviderError::NotAvailable(msg)) => {
            assert!(msg.contains("ASR_OPENCODE_MODEL"), "msg: {msg}");
            assert!(msg.contains("opencode.ai/zen/go"), "msg should point to docs: {msg}");
        }
        other => panic!("expected NotAvailable, got {other:?}"),
    }
}

#[test]
fn from_env_with_all_required_returns_ok() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "test-key");
    setenv("ASR_OPENCODE_MODEL", "deepseek-v3.2-exp");
    let p = OpenCodeGoProvider::from_env().expect("must succeed with both vars set");
    assert_eq!(p.name(), "opencode-go");
    assert_eq!(p.model(), "deepseek-v3.2-exp");
}

#[test]
fn protocol_defaults_to_openai() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "k");
    setenv("ASR_OPENCODE_MODEL", "m");
    let p = OpenCodeGoProvider::from_env().unwrap();
    assert_eq!(p.protocol(), Protocol::OpenAi);
}

#[test]
fn protocol_env_anthropic_routes_to_anthropic() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "k");
    setenv("ASR_OPENCODE_MODEL", "m");
    setenv("ASR_OPENCODE_PROTOCOL", "anthropic");
    let p = OpenCodeGoProvider::from_env().unwrap();
    assert_eq!(p.protocol(), Protocol::Anthropic);
}

#[test]
fn timeout_env_clamped_to_range() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "k");
    setenv("ASR_OPENCODE_MODEL", "m");

    setenv("ASR_OPENCODE_TIMEOUT_MS", "999999999");
    let p = OpenCodeGoProvider::from_env().unwrap();
    assert_eq!(p.timeout_ms(), 600_000, "must clamp to 600s upper bound");

    setenv("ASR_OPENCODE_TIMEOUT_MS", "100");
    let p = OpenCodeGoProvider::from_env().unwrap();
    assert_eq!(p.timeout_ms(), 5_000, "must clamp to 5s lower bound");
}

#[test]
fn temperature_parse_fail_falls_back_to_default() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "k");
    setenv("ASR_OPENCODE_MODEL", "m");
    setenv("ASR_OPENCODE_TEMPERATURE", "not-a-number");
    let p = OpenCodeGoProvider::from_env().unwrap();
    assert!((p.temperature() - 0.2).abs() < 1e-6, "fallback to 0.2");
}

// ═════════════════════════════════════════════════════════════════════════
// HTTP mock — minimal request handler that the runtime spawns a thread for.
// Each handler is given a `RequestHandler` callback that returns the HTTP
// response body (status + body) to send. Captures the request count.
// ═════════════════════════════════════════════════════════════════════════

struct MockServer {
    endpoint: String,
    req_count: Arc<AtomicUsize>,
}

type Handler = Arc<dyn Fn(usize) -> (u16, String) + Send + Sync + 'static>;

impl MockServer {
    fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
        let addr = listener.local_addr().unwrap();
        let req_count = Arc::new(AtomicUsize::new(0));
        let req_count_clone = req_count.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let h = handler.clone();
                let rc = req_count_clone.clone();
                thread::spawn(move || {
                    let mut s = stream;
                    let mut buf = [0u8; 16384];
                    let _ = s.read(&mut buf);
                    let idx = rc.fetch_add(1, Ordering::SeqCst);
                    let (status, body) = h(idx);
                    let status_text = match status {
                        200 => "OK",
                        401 => "Unauthorized",
                        429 => "Too Many Requests",
                        500 => "Internal Server Error",
                        503 => "Service Unavailable",
                        _ => "Status",
                    };
                    let resp = format!(
                        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = s.write_all(resp.as_bytes());
                    let _ = s.flush();
                });
            }
        });
        Self {
            endpoint: format!("http://{addr}/v1/chat/completions"),
            req_count,
        }
    }

    fn count(&self) -> usize {
        self.req_count.load(Ordering::SeqCst)
    }
}

/// Build a provider pointing at a mock server endpoint (OpenAI protocol).
fn provider_to_mock(mock: &MockServer, anthropic: bool) -> OpenCodeGoProvider {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "test-key");
    setenv("ASR_OPENCODE_MODEL", "test-model");
    setenv("ASR_OPENCODE_TIMEOUT_MS", "5000"); // fast for tests
    if anthropic {
        setenv("ASR_OPENCODE_PROTOCOL", "anthropic");
        setenv("ASR_OPENCODE_ENDPOINT_ANTHROPIC", &mock.endpoint);
    } else {
        setenv("ASR_OPENCODE_ENDPOINT_OPENAI", &mock.endpoint);
    }
    OpenCodeGoProvider::from_env().expect("provider construction must succeed")
}

fn run_ask(p: OpenCodeGoProvider) -> Result<String, ProviderError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(p.ask("system", "user"))
}

// ═════════════════════════════════════════════════════════════════════════
// Spec scenarios 8-17: HTTP behavior (in-process mock)
// ═════════════════════════════════════════════════════════════════════════

#[test]
fn openai_200_returns_content() {
    let mock = MockServer::start(Arc::new(|_| {
        (200, r#"{"choices":[{"message":{"content":"hello world"}}]}"#.to_string())
    }));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert_eq!(r.unwrap(), "hello world");
}

#[test]
fn openai_empty_content_falls_back_to_reasoning_content() {
    let mock = MockServer::start(Arc::new(|_| {
        (
            200,
            r#"{"choices":[{"message":{"content":"","reasoning_content":"hi"}}]}"#.to_string(),
        )
    }));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert_eq!(r.unwrap(), "hi");
}

#[test]
fn openai_both_empty_returns_call_failed() {
    let mock = MockServer::start(Arc::new(|_| {
        (
            200,
            r#"{"choices":[{"message":{"content":"","reasoning_content":""}}]}"#.to_string(),
        )
    }));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    match r {
        Err(ProviderError::CallFailed(msg)) => {
            assert!(msg.contains("unexpected response shape"), "msg: {msg}")
        }
        other => panic!("expected CallFailed, got {other:?}"),
    }
}

#[test]
fn http_401_does_not_retry() {
    let mock = MockServer::start(Arc::new(|_| (401, r#"{"error":"bad key"}"#.to_string())));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert!(matches!(r, Err(ProviderError::CallFailed(_))));
    assert_eq!(mock.count(), 1, "401 must not retry");
}

#[test]
fn http_429_retries_up_to_3_times() {
    let mock = MockServer::start(Arc::new(|_| (429, r#"{"error":"rate"}"#.to_string())));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert!(matches!(r, Err(ProviderError::CallFailed(_))));
    assert_eq!(mock.count(), 4, "429 must retry 3 times → 4 total");
}

#[test]
fn http_429_then_200_succeeds() {
    let mock = MockServer::start(Arc::new(|idx| {
        if idx < 2 {
            (429, r#"{"error":"rate"}"#.to_string())
        } else {
            (200, r#"{"choices":[{"message":{"content":"ok"}}]}"#.to_string())
        }
    }));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert_eq!(r.unwrap(), "ok");
    assert_eq!(mock.count(), 3, "should succeed on 3rd attempt");
}

#[test]
fn http_500_does_not_retry() {
    let mock = MockServer::start(Arc::new(|_| (500, r#"{"error":"oops"}"#.to_string())));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert!(matches!(r, Err(ProviderError::CallFailed(_))));
    assert_eq!(mock.count(), 1, "500 is permanent → no retry");
}

#[test]
fn http_503_retries() {
    let mock = MockServer::start(Arc::new(|_| (503, r#"{"error":"down"}"#.to_string())));
    let p = provider_to_mock(&mock, false);
    let r = run_ask(p);
    assert!(matches!(r, Err(ProviderError::CallFailed(_))));
    assert_eq!(mock.count(), 4, "503 retries 3 times → 4 total");
}

#[test]
fn anthropic_200_returns_joined_text_blocks() {
    let mock = MockServer::start(Arc::new(|_| {
        (
            200,
            r#"{"content":[{"type":"text","text":"foo"},{"type":"text","text":"bar"}]}"#
                .to_string(),
        )
    }));
    let p = provider_to_mock(&mock, true);
    let r = run_ask(p);
    assert_eq!(r.unwrap(), "foobar");
}

#[test]
fn anthropic_empty_content_returns_empty_response() {
    let mock = MockServer::start(Arc::new(|_| (200, r#"{"content":[]}"#.to_string())));
    let p = provider_to_mock(&mock, true);
    let r = run_ask(p);
    assert!(matches!(r, Err(ProviderError::EmptyResponse)));
}

// ═════════════════════════════════════════════════════════════════════════
// Spec scenario 18: name
// ═════════════════════════════════════════════════════════════════════════

#[test]
fn name_is_opencode_go() {
    let _g = env_lock().lock().unwrap();
    clear_opencode_env();
    setenv("OPENCODE_API_KEY", "k");
    setenv("ASR_OPENCODE_MODEL", "m");
    let p = OpenCodeGoProvider::from_env().unwrap();
    assert_eq!(p.name(), "opencode-go");
}

// ═════════════════════════════════════════════════════════════════════════
// Regression guard for the spec's "不动 --provider default" decision:
// confirms the CLI argument's default_value is still "fake" even with
// `provider-opencode-go` enabled.
// ═════════════════════════════════════════════════════════════════════════

#[test]
fn cli_loop_provider_default_is_fake_regression() {
    // Parse `ascent-research loop foo` as argv and confirm provider defaults
    // to "fake". Done via the same clap-derive enum the binary uses.
    use clap::Parser;
    use research::cli::{Cli, Commands};
    let cli = Cli::try_parse_from(["ascent-research", "loop", "foo"])
        .expect("loop foo must parse");
    match cli.command {
        Some(Commands::Loop { provider, .. }) => {
            assert_eq!(provider, "fake", "default --provider must stay \"fake\"");
        }
        other => panic!("expected Loop command, got {other:?}"),
    }

    // Make `Duration` import not dead in case the file shrinks later.
    let _ = Duration::from_millis(1);
}
