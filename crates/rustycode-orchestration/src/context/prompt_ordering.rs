//! Prompt Ordering Optimizer — reorders assembled prompt sections
//! to maximize cache prefix stability.
//!
//! Identifies sections by markdown heading patterns and rearranges
//! them so stable content appears first. Anthropic caches the last
//! user message by prefix match, so placing static/semi-static
//! content before dynamic content improves cache hit rates.
//!
//! Matches orchestra-2's prompt-ordering.ts implementation.

use std::collections::HashMap;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Section extracted from a prompt by heading markers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedSection {
    /// Section heading (text after ##)
    pub heading: String,
    /// Section content (including heading line)
    pub content: String,
    /// Section role for cache optimization
    pub role: SectionRole,
}

/// Role of a section for cache optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SectionRole {
    /// Static content - never changes per task
    Static,
    /// Semi-static content - changes per slice but not per task
    SemiStatic,
    /// Dynamic content - changes per task
    Dynamic,
}

/// Cache efficiency analysis results
#[derive(Debug, Clone, PartialEq)]
pub struct CacheEfficiencyAnalysis {
    /// Total characters in prompt
    pub total_chars: usize,
    /// Static content characters
    pub static_chars: usize,
    /// Semi-static content characters
    pub semi_static_chars: usize,
    /// Dynamic content characters
    pub dynamic_chars: usize,
    /// Cache efficiency (0.0 to 1.0)
    pub cache_efficiency: f64,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Reorder a prompt's sections for cache efficiency.
///
/// Extracts sections by ## heading markers, classifies them,
/// and reorders: static -> semi-static -> dynamic.
///
/// Content before the first ## heading is treated as a preamble
/// and always placed first (it's usually static instructions).
///
/// # Arguments
/// * `prompt` - The assembled prompt string
///
/// # Returns
/// Reordered prompt string
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestration::prompt_ordering::*;
///
/// let prompt = r#"
/// ## Dynamic Task
/// Task-specific content here
///
/// ## Static Template
/// Template content here
/// "#;
///
/// let reordered = reorder_for_caching(prompt);
/// // Static Template section now comes before Dynamic Task
/// ```
pub fn reorder_for_caching(prompt: &str) -> String {
    let (preamble, sections) = split_sections(prompt);

    // Nothing to reorder
    if sections.len() <= 1 {
        return prompt.to_string();
    }

    // Stable sort: sections with the same role keep their original relative order
    let mut sorted = sections;
    sorted.sort_by_key(|s| s.role);

    let mut parts = Vec::new();
    if !preamble.is_empty() {
        parts.push(preamble);
    }
    for section in sorted {
        parts.push(section.content.clone());
    }

    parts.join("\n")
}

/// Analyze a prompt's cache efficiency without reordering.
///
/// Returns stats about how much of the prompt is cacheable.
///
/// # Arguments
/// * `prompt` - The assembled prompt string
///
/// # Returns
/// Cache efficiency analysis
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestration::prompt_ordering::*;
///
/// let prompt = "## Static\nStatic content\n## Dynamic\nDynamic content";
/// let analysis = analyze_cache_efficiency(prompt);
/// println!("Cache efficiency: {:.1}%", analysis.cache_efficiency * 100.0);
/// ```
pub fn analyze_cache_efficiency(prompt: &str) -> CacheEfficiencyAnalysis {
    let (preamble, sections) = split_sections(prompt);

    let mut static_chars = preamble.len();
    let mut semi_static_chars = 0;
    let mut dynamic_chars = 0;

    for section in &sections {
        match section.role {
            SectionRole::Static => {
                static_chars += section.content.len();
            }
            SectionRole::SemiStatic => {
                semi_static_chars += section.content.len();
            }
            SectionRole::Dynamic => {
                dynamic_chars += section.content.len();
            }
        }
    }

    let total_chars = static_chars + semi_static_chars + dynamic_chars;
    let cache_efficiency = if total_chars > 0 {
        (static_chars + semi_static_chars) as f64 / total_chars as f64
    } else {
        0.0
    };

    CacheEfficiencyAnalysis {
        total_chars,
        static_chars,
        semi_static_chars,
        dynamic_chars,
        cache_efficiency,
    }
}

// ─── Internals ─────────────────────────────────────────────────────────────

/// Get the heading role map for Orchestra prompts.
///
/// Static: templates, executor constraints, system instructions
/// Semi-static: slice plan, decisions, requirements, prior summaries, overrides
/// Dynamic: task plan, resume state, carry-forward, verification
fn get_heading_roles() -> HashMap<String, SectionRole> {
    let mut roles = HashMap::new();

    // Static — never changes per task
    roles.insert("Output Template".to_string(), SectionRole::Static);
    roles.insert(
        "Executor Context Constraints".to_string(),
        SectionRole::Static,
    );
    roles.insert("Working Directory".to_string(), SectionRole::Static);
    roles.insert("Backing Source Artifacts".to_string(), SectionRole::Static);

    // Semi-static — changes per slice but not per task
    roles.insert("Slice Plan Excerpt".to_string(), SectionRole::SemiStatic);
    roles.insert("Decisions".to_string(), SectionRole::SemiStatic);
    roles.insert("Requirements".to_string(), SectionRole::SemiStatic);
    roles.insert("Prior Task Summaries".to_string(), SectionRole::SemiStatic);
    roles.insert("Overrides".to_string(), SectionRole::SemiStatic);
    roles.insert("Project Knowledge".to_string(), SectionRole::SemiStatic);
    roles.insert("Dependency Summaries".to_string(), SectionRole::SemiStatic);

    // Dynamic — changes per task
    roles.insert("Inlined Task Plan".to_string(), SectionRole::Dynamic);
    roles.insert("Resume State".to_string(), SectionRole::Dynamic);
    roles.insert("Carry-Forward Context".to_string(), SectionRole::Dynamic);
    roles.insert("Verification".to_string(), SectionRole::Dynamic);
    roles.insert("Verification Evidence".to_string(), SectionRole::Dynamic);

    roles
}

/// Extract the heading text from a line like "## Some Heading" or "## UNIT: Execute Task ...".
///
/// Returns the full text after "## " for role lookup.
fn extract_heading_text(line: &str) -> String {
    line.trim_start_matches("##")
        .trim_start()
        .trim()
        .to_string()
}

/// Classify a heading by matching against known roles.
///
/// Uses substring matching so headings like "## UNIT: Execute Task T1.1" don't match
/// but "## Inlined Task Plan" does. Unknown headings default to "dynamic".
fn classify_heading(heading: &str) -> SectionRole {
    let roles = get_heading_roles();

    for (key, role) in &roles {
        if heading == *key || heading.starts_with(key) {
            return *role;
        }
    }

    SectionRole::Dynamic
}

/// Split a prompt into sections at ## heading boundaries.
///
/// Sub-headings (### and deeper) stay with their parent ## section.
/// Returns a preamble (content before first ##) and an array of sections.
fn split_sections(prompt: &str) -> (String, Vec<ExtractedSection>) {
    let lines: Vec<&str> = prompt.lines().collect();
    let mut preamble = String::new();
    let mut sections = Vec::new();
    let mut current_heading = String::new();
    let mut current_content: Vec<String> = Vec::new();

    for line in lines {
        // Match ## headings but NOT ### or deeper
        if line.starts_with("## ") && !line.starts_with("### ") {
            // Flush previous section
            if !current_heading.is_empty() {
                sections.push(ExtractedSection {
                    heading: current_heading.clone(),
                    content: current_content.join("\n"),
                    role: classify_heading(&current_heading),
                });
            } else if !current_content.is_empty() {
                preamble = current_content.join("\n");
            }
            current_heading = extract_heading_text(line);
            current_content = vec![line.to_string()];
        } else {
            current_content.push(line.to_string());
        }
    }

    // Flush last section
    if !current_heading.is_empty() {
        let heading = current_heading;
        let role = classify_heading(&heading);
        sections.push(ExtractedSection {
            heading,
            content: current_content.join("\n"),
            role,
        });
    } else if !current_content.is_empty() {
        preamble = current_content.join("\n");
    }

    (preamble, sections)
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_heading_text() {
        assert_eq!(extract_heading_text("## Some Heading"), "Some Heading");
        assert_eq!(extract_heading_text("##  Extra Spaces  "), "Extra Spaces");
        assert_eq!(
            extract_heading_text("##UNIT: Execute Task"),
            "UNIT: Execute Task"
        );
    }

    #[test]
    fn test_classify_heading() {
        assert_eq!(classify_heading("Output Template"), SectionRole::Static);
        assert_eq!(classify_heading("Decisions"), SectionRole::SemiStatic);
        assert_eq!(classify_heading("Inlined Task Plan"), SectionRole::Dynamic);
        assert_eq!(classify_heading("Unknown Heading"), SectionRole::Dynamic);
        assert_eq!(classify_heading("Decisions Extra"), SectionRole::SemiStatic);
    }

    #[test]
    fn test_split_sections_no_headings() {
        let prompt = "Just some text\nwithout any headings";
        let (preamble, sections) = split_sections(prompt);
        assert_eq!(preamble, prompt);
        assert!(sections.is_empty());
    }

    #[test]
    fn test_split_sections_with_headings() {
        let prompt =
            "Preamble text\n## First Section\nContent here\n## Second Section\nMore content";
        let (preamble, sections) = split_sections(prompt);
        assert_eq!(preamble, "Preamble text");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].heading, "First Section");
        assert_eq!(sections[1].heading, "Second Section");
    }

    #[test]
    fn test_split_sections_preserves_subheadings() {
        let prompt = "## Main Section\nContent\n### Subsection\nSub content\nMore main content";
        let (_preamble, sections) = split_sections(prompt);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].content.contains("### Subsection"));
        assert!(sections[0].content.contains("Sub content"));
    }

    #[test]
    fn test_reorder_for_caching_no_headings() {
        let prompt = "Just some text\nwithout headings";
        let reordered = reorder_for_caching(prompt);
        assert_eq!(reordered, prompt);
    }

    #[test]
    fn test_reorder_for_caching_single_section() {
        let prompt = "## Only Section\nContent here";
        let reordered = reorder_for_caching(prompt);
        assert_eq!(reordered, prompt);
    }

    #[test]
    fn test_reorder_for_caching_reorder() {
        let prompt = "## Inlined Task Plan\nDynamic content\n\n## Output Template\nStatic content";
        let reordered = reorder_for_caching(prompt);
        // Static (Output Template) should come before dynamic (Inlined Task Plan)
        let pos_static = reordered.find("## Output Template").unwrap();
        let pos_dynamic = reordered.find("## Inlined Task Plan").unwrap();
        assert!(pos_static < pos_dynamic);
    }

    #[test]
    fn test_reorder_for_caching_preserves_preamble() {
        let prompt = "Preamble text\n## Section\nContent";
        let reordered = reorder_for_caching(prompt);
        assert!(reordered.starts_with("Preamble text"));
    }

    #[test]
    fn test_analyze_cache_efficiency_all_static() {
        let prompt =
            "## Output Template\nStatic content\n\n## Executor Context Constraints\nMore static";
        let analysis = analyze_cache_efficiency(prompt);
        // All sections are static, but we need to count the heading lines too
        assert!(analysis.static_chars > 0);
        assert_eq!(analysis.semi_static_chars, 0);
        assert_eq!(analysis.dynamic_chars, 0);
        assert_eq!(analysis.cache_efficiency, 1.0);
    }

    #[test]
    fn test_analyze_cache_efficiency_mixed() {
        let prompt = "## Output Template\nStatic\n\n## Inlined Task Plan\nDynamic";
        let analysis = analyze_cache_efficiency(prompt);
        assert!(analysis.static_chars > 0);
        assert!(analysis.dynamic_chars > 0);
        assert!(analysis.cache_efficiency > 0.0);
        assert!(analysis.cache_efficiency < 1.0);
    }

    #[test]
    fn test_analyze_cache_efficiency_empty() {
        let prompt = "";
        let analysis = analyze_cache_efficiency(prompt);
        assert_eq!(analysis.total_chars, 0);
        assert_eq!(analysis.static_chars, 0);
        assert_eq!(analysis.cache_efficiency, 0.0);
    }

    #[test]
    fn test_reorder_stable_sort() {
        // Test that sections with the same role maintain relative order
        let prompt = "## Dynamic A\nA\n\n## Dynamic B\nB\n\n## Static\nStatic";
        let reordered = reorder_for_caching(prompt);
        // Find positions of dynamic sections
        let pos_a = reordered.find("## Dynamic A").unwrap();
        let pos_b = reordered.find("## Dynamic B").unwrap();
        assert!(pos_a < pos_b); // A should still come before B
    }
}
