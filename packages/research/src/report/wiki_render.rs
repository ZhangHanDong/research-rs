//! Render `<session>/wiki/*.md` pages into HTML for the rich-report
//! template. Produces one `<section class="wiki-page" id="wiki-<slug>">`
//! per page, ordered alphabetically.
//!
//! Responsibilities:
//! - Strip the YAML frontmatter from the page body (the structured
//!   fields already surfaced in `coverage`; the HTML doesn't need to
//!   echo them).
//! - Run the body through `markdown::render_body` so diagram-inline,
//!   aside-extraction, and section-numbering conventions all apply
//!   uniformly.
//! - Rewrite `[[slug]]` inline links into anchor references
//!   (`<a href="#wiki-<slug>">slug</a>`) for existing pages, or a
//!   `<span class="wiki-broken" title="...">` marker for broken ones.
//! - Extract the first `<h1>` (if any) as the section title; fall back
//!   to the slug.
//!
//! The wiki section is emitted as a single concatenated HTML string so
//! synthesize can drop it into the template between the numbered
//! sections and Sources. Bilingual injection runs over the full
//! concatenated body+wiki HTML downstream — no wiki-specific hook.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::report::markdown::{self, RenderError};
use crate::session::wiki;

#[derive(Debug, Clone, Default)]
pub struct WikiRender {
    pub html: String,
    pub page_count: u32,
    pub broken_links: u32,
    pub warnings: Vec<String>,
}

pub fn render_wiki(slug: &str, session_dir: &Path) -> Result<WikiRender, RenderError> {
    let page_slugs: Vec<String> = wiki::list_pages(slug);
    if page_slugs.is_empty() {
        return Ok(WikiRender::default());
    }
    let page_set: HashSet<&str> = page_slugs.iter().map(String::as_str).collect();

    let mut out = String::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut broken_links: u32 = 0;
    out.push_str(r#"<section class="wiki-root"><h2><span class="section-num">WIKI</span><span>Entity & concept pages</span></h2>"#);

    for page_slug in &page_slugs {
        let body = match wiki::read_page(slug, page_slug) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(format!("wiki_read_error: {page_slug}: {e}"));
                continue;
            }
        };
        let (_fm, rest) = wiki::split_frontmatter(&body);
        // Render the body via the shared markdown pipeline so any
        // `![...](diagrams/x.svg)` inside a wiki page inlines exactly
        // like it would in a numbered section.
        let rendered = markdown::render_body(rest, session_dir)?;
        warnings.extend(rendered.warnings.iter().cloned());
        let with_links = rewrite_wiki_links(&rendered.body_html, &page_set, &mut broken_links);
        let title = extract_title(&rendered.body_html).unwrap_or_else(|| page_slug.clone());
        out.push_str(&format!(
            r#"<section class="wiki-page" id="wiki-{page_slug}"><h3>{title}</h3>"#
        ));
        out.push_str(&with_links);
        out.push_str("</section>");
    }

    out.push_str("</section>");
    Ok(WikiRender {
        html: out,
        page_count: page_slugs.len() as u32,
        broken_links,
        warnings,
    })
}

fn rewrite_wiki_links(html: &str, valid_slugs: &HashSet<&str>, broken: &mut u32) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\[\[([a-z0-9_-]+)\]\]").expect("wiki link regex"));
    re.replace_all(html, |caps: &regex::Captures| {
        let target = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if valid_slugs.contains(target) {
            format!(r##"<a class="wiki-link" href="#wiki-{target}">{target}</a>"##)
        } else {
            *broken += 1;
            format!(
                r#"<span class="wiki-broken" title="no wiki page named {target}">[[{target}]]</span>"#
            )
        }
    })
    .into_owned()
}

fn extract_title(html: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"<h1[^>]*>([\s\S]*?)</h1>").expect("h1 regex"));
    let caps = re.captures(html)?;
    // Strip nested tags inside <h1>.
    let raw = caps.get(1)?.as_str();
    let stripped = tag_strip_re().replace_all(raw, "").trim().to_string();
    if stripped.is_empty() { None } else { Some(stripped) }
}

fn tag_strip_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag strip regex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_replaces_valid_links_as_anchors() {
        let mut set = HashSet::new();
        set.insert("scheduler");
        set.insert("task-system");
        let mut broken = 0u32;
        let out = rewrite_wiki_links(
            "See [[scheduler]] and [[task-system]].",
            &set,
            &mut broken,
        );
        assert_eq!(broken, 0);
        assert!(out.contains(r##"href="#wiki-scheduler""##));
        assert!(out.contains(r##"href="#wiki-task-system""##));
        assert!(!out.contains("[[scheduler]]"));
    }

    #[test]
    fn rewrite_flags_broken_links() {
        let set: HashSet<&str> = HashSet::from(["existing"]);
        let mut broken = 0u32;
        let out = rewrite_wiki_links("see [[missing]] page", &set, &mut broken);
        assert_eq!(broken, 1);
        assert!(out.contains(r#"class="wiki-broken""#));
        assert!(out.contains("no wiki page named missing"));
    }

    #[test]
    fn extract_title_picks_first_h1_stripped() {
        let html = "<h1>Scheduler <em>(multi-thread)</em></h1><p>body</p>";
        assert_eq!(extract_title(html), Some("Scheduler (multi-thread)".into()));
    }

    #[test]
    fn extract_title_none_when_no_h1() {
        assert!(extract_title("<p>no heading</p>").is_none());
    }
}
