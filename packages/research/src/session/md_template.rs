//! Initial session.md template generation.
//!
//! Must emit the canonical `SOURCES_START_MARKER` / `SOURCES_END_MARKER`
//! pair between a `## Sources` heading; subsequent `research add` rewrites
//! the content between markers and must find them intact.

use super::layout::{SOURCES_END_MARKER, SOURCES_START_MARKER};

/// Render a session.md template. If `parent_slug` + `parent_overview` are
/// both provided (from a `--from <parent>` fork) a `## Context (from <parent>)`
/// block is inserted between Preset and Sources so the LLM editing the child
/// session can see what the parent was about.
pub fn render_with_context(
    topic: &str,
    preset: &str,
    parent_slug: Option<&str>,
    parent_overview: Option<&str>,
) -> String {
    let context_block = match (parent_slug, parent_overview) {
        (Some(p), Some(o)) => format!("\n## Context (from {p})\n{o}\n\n"),
        _ => String::new(),
    };
    format!(
        "# Research: {topic}\n\
         \n\
         ## Objective\n\
         <!-- fill in before synthesize -->\n\
         \n\
         ## Preset\n\
         {preset}\n\
         \n\
         {context_block}\
         ## Sources\n\
         {SOURCES_START_MARKER}\n\
         _(auto-managed by `research add` — do not hand-edit between markers)_\n\
         {SOURCES_END_MARKER}\n\
         \n\
         ## Overview\n\
         <!-- required by `research synthesize`; describe the main story here -->\n\
         \n\
         ## Findings\n\
         <!-- `### Title` + body, one heading per finding -->\n\
         \n\
         ## Notes\n\
         <!-- free-form prose; become the Detailed Analysis section -->\n\
         "
    )
}

pub fn render(topic: &str, preset: &str) -> String {
    render_with_context(topic, preset, None, None)
}

#[cfg(test)]
mod tests {
    use super::super::layout::locate_sources_block;
    use super::*;

    #[test]
    fn template_contains_both_markers() {
        let md = render("Some Topic", "tech");
        assert!(md.contains("# Research: Some Topic"));
        assert!(md.contains("## Preset"));
        assert!(md.contains("tech"));
        let range = locate_sources_block(&md).unwrap();
        assert!(!md[range].is_empty());
    }

    #[test]
    fn template_with_context_has_parent_overview() {
        let md = render_with_context(
            "Child",
            "tech",
            Some("parent-slug"),
            Some("Parent overview sentence."),
        );
        assert!(md.contains("## Context (from parent-slug)"));
        assert!(md.contains("Parent overview sentence."));
        // Markers must still be valid
        assert!(locate_sources_block(&md).is_ok());
    }

    #[test]
    fn template_without_context_omits_block() {
        let md = render("Solo", "tech");
        assert!(!md.contains("## Context"));
    }
}
