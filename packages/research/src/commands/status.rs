use serde_json::json;

use crate::output::Envelope;
use crate::session::{active, config, event::SessionEvent, log};

const CMD: &str = "research status";

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

    let cfg = match config::read(&slug) {
        Ok(c) => c,
        Err(e) => return Envelope::fail(CMD, "IO_ERROR", format!("read session.toml: {e}")),
    };

    let events = log::read_all(&slug).unwrap_or_default();
    let mut attempted = 0u32;
    let mut accepted = 0u32;
    let mut rejected = 0u32;
    let mut synthesized = false;
    for ev in &events {
        match ev {
            SessionEvent::SourceAttempted { .. } => attempted += 1,
            SessionEvent::SourceAccepted { .. } => accepted += 1,
            SessionEvent::SourceRejected { .. } => rejected += 1,
            SessionEvent::SynthesizeCompleted { .. } => synthesized = true,
            _ => {}
        }
    }

    Envelope::ok(
        CMD,
        json!({
            "slug": cfg.slug,
            "topic": cfg.topic,
            "preset": cfg.preset,
            "created_at": cfg.created_at,
            "closed_at": cfg.closed_at,
            "status": if cfg.is_closed() { "closed" } else { "open" },
            "sources": {
                "attempted": attempted,
                "accepted": accepted,
                "rejected": rejected,
            },
            "synthesized": synthesized,
        }),
    )
    .with_context(json!({ "session": slug }))
}
