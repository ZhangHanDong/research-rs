use serde_json::json;
use std::fs;

use crate::output::Envelope;
use crate::session::layout;

const CMD: &str = "research show";

pub fn run(slug: &str) -> Envelope {
    let path = layout::session_md(slug);
    if !path.exists() {
        return Envelope::fail(CMD, "SESSION_NOT_FOUND", format!("no session '{slug}'"))
            .with_context(json!({ "session": slug }));
    }
    match fs::read_to_string(&path) {
        Ok(text) => {
            // Plain-text path prints the raw md; JSON path returns it as a string.
            println!("{text}");
            Envelope::ok(CMD, json!({ "slug": slug, "bytes": text.len() }))
                .with_context(json!({ "session": slug }))
        }
        Err(e) => Envelope::fail(CMD, "IO_ERROR", format!("read session.md: {e}"))
            .with_context(json!({ "session": slug })),
    }
}
