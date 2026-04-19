//! Build a json-ui Report document from parsed session state.
//!
//! Canonical 8 sections (per research-synthesize.spec.md):
//! 1. BrandHeader
//! 2. Section "Overview" — Prose
//! 3. Section "Key Findings" — ContributionList
//! 4. (optional) Section "Metrics" — MetricsGrid
//! 5. Section "Analysis" — Prose from Notes
//! 6. (optional) Section "Conclusion" — Prose
//! 7. Section "Sources" — LinkGroup
//! 8. Section "Methodology" — Callout
//! + BrandFooter

use chrono::Utc;
use serde_json::{Value, json};

use crate::session::{
    event::SessionEvent,
    md_parser::{self, Finding, Metric},
};

/// Input bundle describing everything needed to build a report.
pub struct ReportInput<'a> {
    pub topic: &'a str,
    pub preset: &'a str,
    pub md: &'a str,
    pub events: &'a [SessionEvent],
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildError {
    MissingOverview,
}

pub struct ReportBuild {
    pub json: Value,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub executor_breakdown: ExecutorBreakdown,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutorBreakdown {
    pub postagent: u32,
    pub browser: u32,
}

pub fn build(input: &ReportInput) -> Result<ReportBuild, BuildError> {
    let sections = md_parser::parse_sections(input.md);
    let overview = sections
        .get("Overview")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !looks_like_placeholder(s))
        .ok_or(BuildError::MissingOverview)?;

    let mut children: Vec<Value> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // 1. BrandHeader
    children.push(json!({
        "type": "BrandHeader",
        "props": {
            "badge": "Research Report",
            "poweredBy": "Actionbook / research CLI"
        }
    }));

    // 2. Overview
    children.push(section("Overview", "paper", vec![prose(overview)]));

    // 3. Key Findings
    let findings: Vec<Finding> = sections
        .get("Findings")
        .map(|s| md_parser::parse_findings(s))
        .unwrap_or_default();
    if findings.is_empty() {
        warnings.push("no findings recorded (`## Findings` section empty or missing)".into());
        children.push(section(
            "Key Findings",
            "star",
            vec![prose("_(no findings recorded)_")],
        ));
    } else {
        let items: Vec<Value> = findings
            .iter()
            .enumerate()
            .map(|(i, f)| {
                json!({
                    "badge": format!("{}", i + 1),
                    "title": f.title,
                    "description": f.body
                })
            })
            .collect();
        children.push(section(
            "Key Findings",
            "star",
            vec![json!({
                "type": "ContributionList",
                "props": { "items": items }
            })],
        ));
    }

    // 4. Metrics (optional)
    if let Some(body) = sections.get("Metrics") {
        let metrics: Vec<Metric> = md_parser::parse_metrics(body);
        if !metrics.is_empty() {
            let entries: Vec<Value> = metrics
                .iter()
                .map(|m| {
                    let mut entry = json!({
                        "label": m.label,
                        "value": m.value,
                    });
                    if let Some(s) = &m.suffix {
                        entry["suffix"] = json!(s);
                    } else {
                        entry["suffix"] = json!("");
                    }
                    entry
                })
                .collect();
            children.push(section(
                "Metrics",
                "chart",
                vec![json!({
                    "type": "MetricsGrid",
                    "props": { "metrics": entries, "cols": 3 }
                })],
            ));
        }
    }

    // 5. Analysis (Notes section)
    if let Some(body) = sections.get("Notes") {
        if !body.trim().is_empty() {
            children.push(section("Analysis", "bulb", vec![prose(body)]));
        }
    }

    // 6. Conclusion (optional)
    if let Some(body) = sections.get("Conclusion") {
        if !body.trim().is_empty() {
            children.push(section("Conclusion", "info", vec![prose(body)]));
        }
    }

    // 7. Sources + gather stats
    let mut accepted_count = 0u32;
    let mut rejected_count = 0u32;
    let mut breakdown = ExecutorBreakdown::default();
    let mut links: Vec<Value> = Vec::new();
    for ev in input.events {
        match ev {
            SessionEvent::SourceAccepted {
                url,
                kind,
                executor,
                trust_score,
                ..
            } => {
                accepted_count += 1;
                match executor.as_str() {
                    "postagent" => breakdown.postagent += 1,
                    "browser" => breakdown.browser += 1,
                    _ => {}
                }
                links.push(json!({
                    "href": url,
                    "label": format!("[{kind} · trust {trust_score:.1}] {url}"),
                    "icon": match executor.as_str() {
                        "postagent" => "code",
                        _ => "book",
                    }
                }));
            }
            SessionEvent::SourceRejected { .. } => {
                rejected_count += 1;
            }
            _ => {}
        }
    }
    children.push(section(
        "Sources",
        "link",
        vec![json!({
            "type": "LinkGroup",
            "props": { "links": links }
        })],
    ));

    // 8. Methodology — structured data fields so tests don't rely on string match.
    let methodology_text = format!(
        "Total accepted: {accepted} (postagent: {pa}, browser: {br}) · Rejected: {rj} · Preset: {preset}",
        accepted = accepted_count,
        pa = breakdown.postagent,
        br = breakdown.browser,
        rj = rejected_count,
        preset = input.preset,
    );
    children.push(json!({
        "type": "Section",
        "props": { "title": "Methodology", "icon": "info" },
        "children": [
            {
                "type": "Callout",
                "props": {
                    "type": "note",
                    "title": "Source inventory",
                    "content": methodology_text,
                    "data": {
                        "accepted_total": accepted_count,
                        "accepted_postagent": breakdown.postagent,
                        "accepted_browser": breakdown.browser,
                        "rejected_total": rejected_count,
                        "preset": input.preset,
                    }
                }
            }
        ]
    }));

    // BrandFooter
    children.push(json!({
        "type": "BrandFooter",
        "props": {
            "timestamp": Utc::now().to_rfc3339(),
            "attribution": "Powered by Actionbook + postagent",
            "disclaimer": "Generated by the research CLI. Verify critical claims against upstream sources."
        }
    }));

    let root = json!({
        "type": "Report",
        "props": { "theme": "auto" },
        "children": children,
    });

    Ok(ReportBuild {
        json: root,
        accepted_count,
        rejected_count,
        executor_breakdown: breakdown,
        warnings,
    })
}

fn section(title: &str, icon: &str, children: Vec<Value>) -> Value {
    json!({
        "type": "Section",
        "props": { "title": title, "icon": icon },
        "children": children,
    })
}

fn prose(content: &str) -> Value {
    json!({
        "type": "Prose",
        "props": { "content": content }
    })
}

fn looks_like_placeholder(s: &str) -> bool {
    let t = s.trim();
    // Heuristic: only HTML-comment placeholder or very short.
    (t.starts_with("<!--") && t.ends_with("-->")) || t.len() < 10
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_md() -> &'static str {
        "\
# Research: Topic

## Overview
Overview body in two sentences. Good enough.

## Findings
### A
Body A.

### B
Body B.

## Notes
Analytical prose here.
"
    }

    fn empty_events() -> Vec<SessionEvent> {
        Vec::new()
    }

    #[test]
    fn missing_overview_errors() {
        let md = "## Findings\n### A\nbody\n";
        let r = build(&ReportInput {
            topic: "T",
            preset: "tech",
            md,
            events: &empty_events(),
        });
        assert_eq!(r.err(), Some(BuildError::MissingOverview));
    }

    #[test]
    fn placeholder_overview_treated_as_missing() {
        let md = "## Overview\n<!-- fill me -->\n";
        assert_eq!(
            build(&ReportInput {
                topic: "T",
                preset: "tech",
                md,
                events: &empty_events(),
            })
            .err(),
            Some(BuildError::MissingOverview)
        );
    }

    #[test]
    fn findings_render_as_contribution_list() {
        let out = build(&ReportInput {
            topic: "T",
            preset: "tech",
            md: sample_md(),
            events: &empty_events(),
        })
        .unwrap();
        let children = out.json["children"].as_array().unwrap();
        let findings_section = children
            .iter()
            .find(|c| c["props"]["title"] == "Key Findings")
            .unwrap();
        let list = &findings_section["children"][0];
        assert_eq!(list["type"], "ContributionList");
        assert_eq!(list["props"]["items"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn methodology_counts_are_structured() {
        let events = vec![
            SessionEvent::SourceAccepted {
                timestamp: Utc::now(),
                url: "https://a".into(),
                kind: "hn-item".into(),
                executor: "postagent".into(),
                raw_path: "raw/1.json".into(),
                bytes: 100,
                trust_score: 2.0,
                note: None,
            },
            SessionEvent::SourceAccepted {
                timestamp: Utc::now(),
                url: "https://b".into(),
                kind: "browser-fallback".into(),
                executor: "browser".into(),
                raw_path: "raw/2.json".into(),
                bytes: 800,
                trust_score: 1.0,
                note: None,
            },
            SessionEvent::SourceRejected {
                timestamp: Utc::now(),
                url: "https://c".into(),
                kind: "k".into(),
                executor: "browser".into(),
                reason: crate::session::event::RejectReason::WrongUrl,
                observed_url: None,
                observed_bytes: None,
                rejected_raw_path: None,
                note: None,
            },
        ];
        let out = build(&ReportInput {
            topic: "T",
            preset: "tech",
            md: sample_md(),
            events: &events,
        })
        .unwrap();
        assert_eq!(out.accepted_count, 2);
        assert_eq!(out.rejected_count, 1);
        assert_eq!(out.executor_breakdown.postagent, 1);
        assert_eq!(out.executor_breakdown.browser, 1);

        let children = out.json["children"].as_array().unwrap();
        let m = children
            .iter()
            .find(|c| c["props"]["title"] == "Methodology")
            .unwrap();
        let data = &m["children"][0]["props"]["data"];
        assert_eq!(data["accepted_total"], 2);
        assert_eq!(data["accepted_postagent"], 1);
        assert_eq!(data["accepted_browser"], 1);
        assert_eq!(data["rejected_total"], 1);
        assert_eq!(data["preset"], "tech");
    }

    #[test]
    fn sources_section_skips_rejected() {
        let events = vec![
            SessionEvent::SourceAccepted {
                timestamp: Utc::now(),
                url: "https://a".into(),
                kind: "hn-item".into(),
                executor: "postagent".into(),
                raw_path: "raw/1.json".into(),
                bytes: 100,
                trust_score: 2.0,
                note: None,
            },
            SessionEvent::SourceRejected {
                timestamp: Utc::now(),
                url: "https://b".into(),
                kind: "k".into(),
                executor: "browser".into(),
                reason: crate::session::event::RejectReason::WrongUrl,
                observed_url: None,
                observed_bytes: None,
                rejected_raw_path: None,
                note: None,
            },
        ];
        let out = build(&ReportInput {
            topic: "T",
            preset: "tech",
            md: sample_md(),
            events: &events,
        })
        .unwrap();
        let children = out.json["children"].as_array().unwrap();
        let s = children
            .iter()
            .find(|c| c["props"]["title"] == "Sources")
            .unwrap();
        let links = s["children"][0]["props"]["links"].as_array().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0]["href"], "https://a");
    }
}
