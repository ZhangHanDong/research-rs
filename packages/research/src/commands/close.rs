use chrono::Utc;
use serde_json::json;

use crate::output::Envelope;
use crate::session::{active, config, event::SessionEvent, log};

const CMD: &str = "research close";

pub fn run(slug_arg: Option<&str>) -> Envelope {
    let slug = match slug_arg {
        Some(s) => s.to_string(),
        None => match active::get_active() {
            Some(s) => s,
            None => {
                return Envelope::fail(
                    CMD,
                    "NO_ACTIVE_SESSION",
                    "no active session — pass <slug> or run `research new` first",
                );
            }
        },
    };

    if !config::exists(&slug) {
        return Envelope::fail(CMD, "SESSION_NOT_FOUND", format!("no session '{slug}'"))
            .with_context(json!({ "session": slug }));
    }

    // Mutate session.toml: set closed_at.
    let mut cfg = match config::read(&slug) {
        Ok(c) => c,
        Err(e) => return Envelope::fail(CMD, "IO_ERROR", format!("read session.toml: {e}")),
    };
    if cfg.closed_at.is_none() {
        cfg.closed_at = Some(Utc::now());
        if let Err(e) = config::write(&slug, &cfg) {
            return Envelope::fail(CMD, "IO_ERROR", format!("write session.toml: {e}"));
        }
    }

    let ev = SessionEvent::SessionClosed {
        timestamp: Utc::now(),
        note: None,
    };
    if let Err(e) = log::append(&slug, &ev) {
        return Envelope::fail(CMD, "IO_ERROR", format!("append session_closed: {e}"));
    }

    // Clear .active if this slug was active.
    if active::get_active().as_deref() == Some(slug.as_str()) {
        let _ = active::clear_active();
    }

    Envelope::ok(
        CMD,
        json!({
            "slug": slug,
            "closed_at": cfg.closed_at,
        }),
    )
    .with_context(json!({ "session": slug }))
}
