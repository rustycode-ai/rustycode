//! Gotcha tracking: pattern matching for known pitfalls during skill execution.
//!
//! Skills declare known pitfalls via `gotchas` in YAML frontmatter. When a
//! skill is activated, relevant gotchas are surfaced as warnings to the agent,
//! preventing common failure modes before they occur.

use serde::{Deserialize, Serialize};

/// Severity level for a gotcha warning.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum GotchaSeverity {
    /// Informational: worth knowing but unlikely to cause failure.
    Info,
    /// Warning: may cause unexpected behavior if not addressed.
    #[default]
    Warning,
    /// Critical: very likely to cause failure if not addressed.
    Critical,
}

impl std::fmt::Display for GotchaSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A single gotcha (known pitfall) associated with a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gotcha {
    /// Human-readable description of the pitfall.
    pub description: String,
    /// Optional keywords that trigger this warning. If empty, it is always
    /// surfaced as a general warning when the skill is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Severity level.
    #[serde(default)]
    pub severity: GotchaSeverity,
    /// Suggested mitigation or workaround.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mitigation: Option<String>,
}

impl Gotcha {
    /// Create a gotcha from a description alone (no explicit keywords).
    /// Word-matching from the description will be used for relevance.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into().trim().to_string(),
            keywords: Vec::new(),
            severity: GotchaSeverity::default(),
            mitigation: None,
        }
    }

    /// Create a gotcha with explicit keywords for matching.
    pub fn with_keywords(description: impl Into<String>, keywords: Vec<String>) -> Self {
        Self {
            description: description.into().trim().to_string(),
            keywords,
            severity: GotchaSeverity::default(),
            mitigation: None,
        }
    }

    /// Create a gotcha with all fields.
    pub fn with_severity_and_mitigation(
        description: impl Into<String>,
        keywords: Vec<String>,
        severity: GotchaSeverity,
        mitigation: Option<String>,
    ) -> Self {
        Self {
            description: description.into().trim().to_string(),
            keywords,
            severity,
            mitigation,
        }
    }

    /// Whether this gotcha is relevant to the given context.
    /// If keywords are set, matches by keyword. Otherwise, matches by
    /// significant words from the description (length > 3).
    pub fn matches(&self, context: &str) -> bool {
        self.relevance_score(context) > 0.0
    }

    /// Score how relevant this gotcha is to the given context (0.0 = not relevant).
    pub fn relevance_score(&self, context: &str) -> f64 {
        let context_lower = context.to_lowercase();
        let mut score = 0.0;

        if self.keywords.is_empty() {
            // Fall back to matching significant words from description
            // Common stopwords / generic verbs that should not increase relevance on their own
            const STOPWORDS: &[&str] = &[
                "parse", "extract", "read", "write", "process", "cause", "may", "return", "check",
                "handle", "use", "ensure", "scan", "scanned",
            ];

            for raw_word in self.description.to_lowercase().split_whitespace() {
                // Trim punctuation and non-alphanumeric characters
                let word = raw_word.trim_matches(|c: char| !c.is_alphanumeric());
                if word.len() > 3 && !STOPWORDS.contains(&word) {
                    if context_lower.contains(word) {
                        score += 0.5;
                    } else if let Some(singular) = word.strip_suffix('s') {
                        // simple singularization: drop trailing 's' (handles 'pdfs' -> 'pdf')
                        if !singular.is_empty() && context_lower.contains(singular) {
                            score += 0.5;
                        }
                    }
                }
            }
        } else {
            for keyword in &self.keywords {
                if context_lower.contains(&keyword.to_lowercase()) {
                    score += 1.0;
                }
            }
        }

        score
    }

    /// Format this gotcha as a warning string for agent output.
    pub fn format_warning(&self) -> String {
        self.mitigation.as_ref().map_or_else(
            || format!("{}: {}", self.severity, self.description),
            |mitigation| {
                format!(
                    "{}: {} (mitigation: {})",
                    self.severity, self.description, mitigation
                )
            },
        )
    }
}

/// A collection of gotchas for a skill, providing registry and detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GotchaRegistry {
    gotchas: Vec<Gotcha>,
}

impl GotchaRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry from raw description strings.
    /// Empty and whitespace-only strings are filtered out.
    pub fn from_descriptions(descriptions: &[String]) -> Self {
        let gotchas = descriptions
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Gotcha::new(s.to_string()))
            .collect();
        Self { gotchas }
    }

    /// Create a registry from pre-built Gotcha instances.
    #[allow(clippy::missing_const_for_fn)]
    pub fn from_gotchas(gotchas: Vec<Gotcha>) -> Self {
        Self { gotchas }
    }

    /// Register a gotcha.
    pub fn register(&mut self, gotcha: Gotcha) {
        self.gotchas.push(gotcha);
    }

    /// Find gotchas relevant to the given context.
    /// Returns all gotchas that match the context, sorted by severity (highest first)
    /// then by relevance (highest first).
    /// If no gotchas match by keyword/relevance, returns all gotchas as general warnings.
    pub fn detect_relevant(&self, context: &str) -> Vec<&Gotcha> {
        let mut scored: Vec<(&Gotcha, f64)> = self
            .gotchas
            .iter()
            .map(|g| (g, g.relevance_score(context)))
            .collect();

        let any_match = scored.iter().any(|(_, s)| *s > 0.0);

        if any_match {
            scored.retain(|(_, s)| *s > 0.0);
            // Sort by severity (Critical > Warning > Info) then by relevance
            scored.sort_by(|a, b| {
                b.0.severity
                    .cmp(&a.0.severity)
                    .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            });
            scored.into_iter().map(|(g, _)| g).collect()
        } else {
            // No keyword match: return all as general warnings
            let mut all: Vec<&Gotcha> = self.gotchas.iter().collect();
            all.sort_by_key(|b| std::cmp::Reverse(b.severity));
            all
        }
    }

    /// Number of registered gotchas.
    pub const fn len(&self) -> usize {
        self.gotchas.len()
    }

    /// Whether there are no registered gotchas.
    pub const fn is_empty(&self) -> bool {
        self.gotchas.is_empty()
    }

    /// Get all registered gotchas.
    pub fn all(&self) -> &[Gotcha] {
        &self.gotchas
    }

    /// Format all gotchas relevant to a context as a single warning block.
    pub fn format_warnings(&self, context: &str) -> String {
        let relevant = self.detect_relevant(context);
        if relevant.is_empty() {
            return String::new();
        }
        relevant
            .iter()
            .map(|g| g.format_warning())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_gotcha_from_string() {
        let gotcha =
            Gotcha::new("Scanned PDFs return empty silently. Check page type first.".to_string());
        assert!(!gotcha.description.is_empty());
        assert!(gotcha.keywords.is_empty());
    }

    #[test]
    fn parse_gotcha_with_keywords() {
        let gotcha = Gotcha::with_keywords(
            "Scanned PDFs return empty silently".to_string(),
            vec!["pdf".to_string(), "scan".to_string()],
        );
        assert_eq!(gotcha.keywords.len(), 2);
    }

    #[test]
    fn gotcha_matches_context_by_keyword() {
        let gotcha = Gotcha::with_keywords(
            "Scanned PDFs return empty".to_string(),
            vec!["pdf".to_string()],
        );
        assert!(gotcha.matches("read the pdf document"));
        assert!(!gotcha.matches("read the csv file"));
    }

    #[test]
    fn gotcha_matches_context_by_description_words() {
        let gotcha = Gotcha::new("Scanned PDFs return empty silently".to_string());
        // Should match when context contains significant words from description
        assert!(gotcha.matches("extract text from scanned pdfs"));
    }

    #[test]
    fn gotcha_registry_from_descriptions() {
        let registry = GotchaRegistry::from_descriptions(&[
            "Scanned PDFs return empty silently".to_string(),
            "UTF-8 encoding issues with BOM markers".to_string(),
        ]);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn gotcha_registry_detect_relevant() {
        let registry = GotchaRegistry::from_descriptions(&[
            "Scanned PDFs return empty silently. Check page type first.".to_string(),
            "UTF-8 BOM markers cause parse failures in CSV readers.".to_string(),
            "Async tasks may deadlock if the runtime is dropped prematurely.".to_string(),
        ]);
        let relevant = registry.detect_relevant("parse a pdf document for text extraction");
        assert_eq!(relevant.len(), 1);
        assert!(relevant[0].description.contains("PDF"));
    }

    #[test]
    fn gotcha_registry_detect_relevant_returns_all_if_no_keywords() {
        let registry = GotchaRegistry::from_descriptions(&["Watch out for X".to_string()]);
        let relevant = registry.detect_relevant("unrelated context");
        // With no keyword matching and short description words, returns all
        assert_eq!(relevant.len(), 1);
    }

    #[test]
    fn gotcha_format_warning() {
        let gotcha = Gotcha::new("Scanned PDFs return empty silently".to_string());
        let formatted = gotcha.format_warning();
        assert!(formatted.contains("WARNING"));
        assert!(formatted.contains("Scanned PDFs"));
    }

    #[test]
    fn gotcha_format_warning_with_mitigation() {
        let gotcha = Gotcha::with_severity_and_mitigation(
            "Scanned PDFs return empty silently".to_string(),
            vec![],
            GotchaSeverity::Critical,
            Some("Check page type before reading".to_string()),
        );
        let formatted = gotcha.format_warning();
        assert!(formatted.contains("CRITICAL"));
        assert!(formatted.contains("mitigation"));
    }

    #[test]
    fn gotcha_registry_empty() {
        let registry = GotchaRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.detect_relevant("anything").is_empty());
    }

    #[test]
    fn gotcha_matching_is_case_insensitive() {
        let gotcha = Gotcha::with_keywords("PDF issue".to_string(), vec!["pdf".to_string()]);
        assert!(gotcha.matches("PARSE THE PDF FILE"));
    }

    #[test]
    fn gotcha_serialization_roundtrip() {
        let gotcha = Gotcha::with_keywords("Test gotcha".to_string(), vec!["keyword1".to_string()]);
        let json = serde_json::to_string(&gotcha).unwrap();
        let back: Gotcha = serde_json::from_str(&json).unwrap();
        assert_eq!(back.description, "Test gotcha");
        assert_eq!(back.keywords.len(), 1);
    }

    #[test]
    fn gotcha_registry_from_mixed_strings() {
        let registry = GotchaRegistry::from_descriptions(&[
            String::new(),
            "Valid gotcha".to_string(),
            "   ".to_string(),
        ]);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn gotcha_relevance_scoring() {
        let gotcha = Gotcha::with_keywords(
            "PDF issue".to_string(),
            vec!["pdf".to_string(), "scan".to_string()],
        );
        // Both keywords match -> higher score
        let score_both = gotcha.relevance_score("scan the pdf file");
        // Only one keyword matches -> lower score
        let score_one = gotcha.relevance_score("read the pdf file");
        assert!(score_both > score_one);
    }

    #[test]
    fn severity_ordering() {
        assert!(GotchaSeverity::Critical > GotchaSeverity::Warning);
        assert!(GotchaSeverity::Warning > GotchaSeverity::Info);
    }

    #[test]
    fn severity_display() {
        assert_eq!(GotchaSeverity::Info.to_string(), "INFO");
        assert_eq!(GotchaSeverity::Warning.to_string(), "WARNING");
        assert_eq!(GotchaSeverity::Critical.to_string(), "CRITICAL");
    }

    #[test]
    fn format_warnings_block() {
        let mut registry = GotchaRegistry::new();
        registry.register(Gotcha::with_severity_and_mitigation(
            "PDF issue".to_string(),
            vec!["pdf".to_string()],
            GotchaSeverity::Critical,
            Some("Check page type".to_string()),
        ));
        registry.register(Gotcha::new("General warning".to_string()));

        let block = registry.format_warnings("parse the pdf");
        assert!(block.contains("CRITICAL"));
        assert!(block.contains("PDF issue"));
    }

    #[test]
    fn format_warnings_empty_context() {
        let registry = GotchaRegistry::new();
        assert!(registry.format_warnings("anything").is_empty());
    }

    #[test]
    fn registry_register_adds_gotcha() {
        let mut registry = GotchaRegistry::new();
        assert!(registry.is_empty());
        registry.register(Gotcha::new("First gotcha".to_string()));
        assert_eq!(registry.len(), 1);
        registry.register(Gotcha::new("Second gotcha".to_string()));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn detect_relevant_sorted_by_severity() {
        let mut registry = GotchaRegistry::new();
        registry.register(Gotcha::with_severity_and_mitigation(
            "Low severity issue".to_string(),
            vec!["test".to_string()],
            GotchaSeverity::Info,
            None,
        ));
        registry.register(Gotcha::with_severity_and_mitigation(
            "Critical issue".to_string(),
            vec!["test".to_string()],
            GotchaSeverity::Critical,
            None,
        ));
        let relevant = registry.detect_relevant("test context");
        assert_eq!(relevant.len(), 2);
        assert_eq!(relevant[0].severity, GotchaSeverity::Critical);
        assert_eq!(relevant[1].severity, GotchaSeverity::Info);
    }

    #[test]
    fn registry_serialization_roundtrip() {
        let mut registry = GotchaRegistry::new();
        registry.register(Gotcha::with_keywords(
            "Test".to_string(),
            vec!["k1".to_string()],
        ));
        let json = serde_json::to_string(&registry).unwrap();
        let back: GotchaRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.all()[0].description, "Test");
    }
}
