//! Standard response envelope shared by every subcommand.
//!
//! JSON shape mirrors actionbook's convention:
//! ```json
//! {
//!   "ok": bool,
//!   "command": "research <sub>",
//!   "context": { "session": "...", ... },
//!   "data": { ... subcommand-specific ... },
//!   "error": null | { "code": "...", "message": "...", "details": ... },
//!   "meta": { "duration_ms": 0, "warnings": [...] }
//! }
//! ```

use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub ok: bool,
    pub command: String,
    pub context: Value,
    pub data: Value,
    pub error: Option<ErrorEnvelope>,
    pub meta: Meta,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Meta {
    pub duration_ms: u64,
    pub warnings: Vec<String>,
}

impl Envelope {
    pub fn ok(command: &str, data: Value) -> Self {
        Self {
            ok: true,
            command: command.to_string(),
            context: Value::Null,
            data,
            error: None,
            meta: Meta::default(),
        }
    }

    pub fn fail(command: &str, code: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            command: command.to_string(),
            context: Value::Null,
            data: Value::Null,
            error: Some(ErrorEnvelope {
                code: code.to_string(),
                message: message.into(),
                details: Value::Null,
            }),
            meta: Meta::default(),
        }
    }

    pub fn with_context(mut self, ctx: Value) -> Self {
        self.context = ctx;
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        if let Some(err) = self.error.as_mut() {
            err.details = details;
        }
        self
    }

    /// Print this envelope as either compact JSON or human-readable plain text.
    pub fn render(&self, json_mode: bool) {
        if json_mode {
            println!("{}", serde_json::to_string(self).unwrap());
        } else {
            self.render_plain();
        }
    }

    fn render_plain(&self) {
        if self.ok {
            println!("ok {}", self.command);
            if !self.data.is_null() {
                // Render data keys as `key: value` lines — keep simple.
                if let Value::Object(map) = &self.data {
                    for (k, v) in map {
                        println!("{k}: {}", format_value(v));
                    }
                }
            }
        } else if let Some(err) = &self.error {
            eprintln!("error {}: {}", err.code, err.message);
        }
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

/// Helper: build a context object for session-scoped commands.
pub fn session_context(slug: Option<&str>) -> Value {
    match slug {
        Some(s) => json!({ "session": s }),
        None => json!({}),
    }
}

/// Standard NOT_IMPLEMENTED envelope — every stub subcommand uses this.
pub fn not_implemented(command: &str) -> Envelope {
    Envelope::fail(
        command,
        "NOT_IMPLEMENTED",
        format!("{command} is not yet implemented"),
    )
    .with_context(json!({ "command": command }))
}
