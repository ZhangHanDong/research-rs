use chrono::Utc;
use serde_json::json;
use std::fs;

use crate::output::Envelope;
use crate::session::{active, config, event::SessionEvent, layout, log};

const CMD: &str = "research resume";

pub fn run(slug: &str) -> Envelope {
    if !config::exists(slug) {
        return Envelope::fail(CMD, "SESSION_NOT_FOUND", format!("no session '{slug}'"))
            .with_context(json!({ "session": slug }));
    }

    if let Err(e) = active::set_active(slug) {
        return Envelope::fail(CMD, "IO_ERROR", format!("set active: {e}"));
    }

    let ev = SessionEvent::SessionResumed {
        timestamp: Utc::now(),
        note: None,
    };
    if let Err(e) = log::append(slug, &ev) {
        return Envelope::fail(CMD, "IO_ERROR", format!("append session_resumed: {e}"));
    }

    // Print session.md + last 10 events as the "resume context" for LLMs.
    if let Ok(md) = fs::read_to_string(layout::session_md(slug)) {
        println!("{md}");
    }
    let events = log::read_all(slug).unwrap_or_default();
    let tail: Vec<&SessionEvent> = events.iter().rev().take(10).rev().collect::<Vec<_>>();
    if !tail.is_empty() {
        println!("--- recent events ({}) ---", tail.len());
        for ev in &tail {
            if let Ok(s) = serde_json::to_string(ev) {
                println!("{s}");
            }
        }
    }

    Envelope::ok(
        CMD,
        json!({
            "slug": slug,
            "recent_events": tail.len() as u32,
            "active": true,
        }),
    )
    .with_context(json!({ "session": slug }))
}
