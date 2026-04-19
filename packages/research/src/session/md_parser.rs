//! Minimal markdown section parser for session.md.
//!
//! We only care about ATX `##` headings as section boundaries, and `###`
//! sub-headings inside `## Findings` as one-finding-per-heading. No full
//! CommonMark parser needed — the input is templated + LLM-edited and the
//! structure is uniform. This avoids an extra dependency.

use std::collections::HashMap;

/// Convenience: pull out the `## Overview` section body from a session.md,
/// or None if missing / empty / just placeholder.
pub fn extract_overview(md: &str) -> Option<String> {
    let sections = parse_sections(md);
    let body = sections.get("Overview")?.trim();
    if body.is_empty() {
        return None;
    }
    // Placeholder-only (a single HTML comment) shouldn't propagate.
    if body.starts_with("<!--") && body.ends_with("-->") && !body.contains('\n') {
        return None;
    }
    Some(body.to_string())
}

/// Parse top-level `## <name>` sections. Returns a map of section name to
/// body text (without the heading line itself; trimmed).
pub fn parse_sections(md: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut current: Option<String> = None;
    let mut buf = String::new();
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // Flush previous section.
            if let Some(name) = current.take() {
                out.insert(name, buf.trim().to_string());
            }
            current = Some(rest.trim().to_string());
            buf.clear();
        } else if current.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
        // lines before the first `## ` heading are ignored (e.g. H1 title)
    }
    if let Some(name) = current.take() {
        out.insert(name, buf.trim().to_string());
    }
    out
}

/// Represents one finding parsed from the `## Findings` section.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub title: String,
    pub body: String,
}

/// Parse `### Heading\nbody...` blocks inside a Findings section body.
pub fn parse_findings(section_body: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut current_title: Option<String> = None;
    let mut buf = String::new();
    for line in section_body.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            if let Some(title) = current_title.take() {
                let body = buf.trim().to_string();
                if !title.is_empty() {
                    out.push(Finding { title, body });
                }
            }
            current_title = Some(rest.trim().to_string());
            buf.clear();
        } else if current_title.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if let Some(title) = current_title {
        let body = buf.trim().to_string();
        if !title.is_empty() {
            out.push(Finding { title, body });
        }
    }
    out
}

/// Parse simple `- label: value [suffix]` metric lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub label: String,
    pub value: String,
    pub suffix: Option<String>,
}

pub fn parse_metrics(section_body: &str) -> Vec<Metric> {
    let mut out = Vec::new();
    for line in section_body.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) else {
            continue;
        };
        let Some((label, tail)) = rest.split_once(':') else {
            continue;
        };
        let tail = tail.trim();
        // `NN suffix` or just `NN`
        let (value, suffix) = match tail.split_once(' ') {
            Some((v, s)) => (v.trim().to_string(), Some(s.trim().to_string())),
            None => (tail.to_string(), None),
        };
        out.push(Metric {
            label: label.trim().to_string(),
            value,
            suffix,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Research: Topic

## Overview
Overview body.

## Findings
### Finding A
Body for A.

### Finding B
Body for B.

## Metrics
- Throughput: 1.5 req/s
- Count: 42

## Notes
Long notes here.
";

    #[test]
    fn sections_are_parsed() {
        let m = parse_sections(SAMPLE);
        assert!(m.contains_key("Overview"));
        assert!(m.contains_key("Findings"));
        assert!(m.contains_key("Metrics"));
        assert!(m.contains_key("Notes"));
        assert_eq!(m["Overview"], "Overview body.");
    }

    #[test]
    fn findings_parsed() {
        let m = parse_sections(SAMPLE);
        let findings = parse_findings(&m["Findings"]);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].title, "Finding A");
        assert_eq!(findings[0].body, "Body for A.");
        assert_eq!(findings[1].title, "Finding B");
    }

    #[test]
    fn metrics_parsed() {
        let m = parse_sections(SAMPLE);
        let metrics = parse_metrics(&m["Metrics"]);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].label, "Throughput");
        assert_eq!(metrics[0].value, "1.5");
        assert_eq!(metrics[0].suffix.as_deref(), Some("req/s"));
        assert_eq!(metrics[1].suffix, None);
    }

    #[test]
    fn missing_section_returns_none() {
        let md = "## Only\nbody\n";
        let m = parse_sections(md);
        assert!(!m.contains_key("Overview"));
    }
}
