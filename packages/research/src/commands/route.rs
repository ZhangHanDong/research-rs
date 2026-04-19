use serde_json::json;
use std::path::Path;

use crate::output::Envelope;
use crate::route::{self, Classification};

const CMD: &str = "research route";

pub fn run(
    url: &str,
    prefer: Option<&str>,
    rules: Option<&str>,
    preset: Option<&str>,
) -> Envelope {
    let prefer_browser = match prefer {
        None | Some("auto") => false,
        Some("browser") => true,
        Some(other) => {
            return Envelope::fail(
                CMD,
                "INVALID_ARGUMENT",
                format!("--prefer must be 'browser' or 'auto', got '{other}'"),
            );
        }
    };

    let rules_path = rules.map(Path::new);
    let compiled = match route::load_preset(preset, rules_path) {
        Ok(p) => p,
        Err(e) => {
            return Envelope::fail(CMD, "PRESET_ERROR", e.message.clone())
                .with_details(json!({
                    "sub_code": e.sub_code.as_str(),
                    "path": e.path,
                }));
        }
    };

    let classification = match route::classify(&compiled, url, prefer_browser) {
        Ok(c) => c,
        Err(msg) => {
            return Envelope::fail(CMD, "INVALID_ARGUMENT", msg);
        }
    };

    let r = classification.route();
    let class_label = match &classification {
        Classification::Matched(_) => "matched",
        Classification::Fallback(_) => "fallback",
        Classification::Forced(_) => "forced",
    };
    Envelope::ok(
        CMD,
        json!({
            "url": r.url,
            "executor": r.executor.as_str(),
            "kind": r.kind,
            "command_template": r.command_template,
            "hints": { "wait_hint": null, "rewrite_url": null },
            "classification": class_label,
            "preset": compiled.name,
        }),
    )
    .with_context(json!({ "url": r.url }))
}
