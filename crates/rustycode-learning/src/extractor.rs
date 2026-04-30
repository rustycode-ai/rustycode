use crate::error::{ExtractionError, Result};
use crate::patterns::{Pattern, PatternCategory, TriggerCondition, TriggerType};
use regex::Regex;
use rustycode_session::{Message, MessageRole, Session};
use std::collections::{HashMap, HashSet};

/// Extracts patterns from sessions and history
#[derive(Debug, Clone)]
pub struct InstinctExtractor {
    patterns: Vec<Pattern>,
    extraction_rules: Vec<ExtractionRule>,
    min_confidence: f32,
}

/// Rule for extracting patterns
#[derive(Debug, Clone)]
#[allow(dead_code)] // Kept for future use
struct ExtractionRule {
    id: String,
    name: String,
    category: PatternCategory,
    trigger_type: TriggerType,
    pattern_regex: Regex,
    context_requirements: HashMap<String, String>,
    min_examples: usize,
}

impl InstinctExtractor {
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            extraction_rules: Self::builtin_rules(),
            min_confidence: 0.3,
        }
    }

    pub const fn with_min_confidence(mut self, min_confidence: f32) -> Self {
        self.min_confidence = min_confidence;
        self
    }

    /// Extract patterns from a single session
    pub async fn extract_from_session(&self, session: &Session) -> Result<Vec<Pattern>> {
        let mut extracted = Vec::new();

        for rule in &self.extraction_rules {
            if let Some(pattern) = self.apply_rule(session, rule).await? {
                extracted.push(pattern);
            }
        }

        if extracted.is_empty() {
            return Err(ExtractionError::NoPatternsFound.into());
        }

        Ok(extracted)
    }

    /// Extract patterns from multiple sessions
    pub async fn extract_from_history(&self, history: &[Session]) -> Result<Vec<Pattern>> {
        let mut all_patterns: HashMap<String, Pattern> = HashMap::new();
        let mut pattern_examples: HashMap<String, HashSet<String>> = HashMap::new();

        // Collect patterns across all sessions
        for session in history {
            match self.extract_from_session(session).await {
                Ok(patterns) => {
                    for pattern in patterns {
                        let _entry = all_patterns
                            .entry(pattern.id.clone())
                            .or_insert_with(|| pattern.clone());

                        // Collect examples
                        for example in &pattern.examples {
                            pattern_examples
                                .entry(pattern.id.clone())
                                .or_default()
                                .insert(example.clone());
                        }
                    }
                }
                Err(_) => continue, // Skip sessions without patterns
            }
        }

        // Merge patterns and calculate confidence
        let mut merged_patterns = Vec::new();
        for (id, mut pattern) in all_patterns {
            if let Some(examples) = pattern_examples.get(&id) {
                pattern.examples = examples.iter().cloned().collect();
            }

            // Calculate confidence based on frequency and consistency
            let confidence = self.calculate_confidence(&pattern, history.len());
            pattern.confidence = confidence;

            if confidence >= self.min_confidence {
                merged_patterns.push(pattern);
            }
        }

        Ok(merged_patterns)
    }

    /// Learn a new pattern manually
    pub fn learn_pattern(&mut self, pattern: Pattern) {
        // Check if pattern already exists
        if let Some(_existing) = self.patterns.iter().find(|p| p.id == pattern.id) {
            // Merge with existing pattern
            // In a real implementation, this would be more sophisticated
        } else {
            self.patterns.push(pattern);
        }
    }

    /// Get a pattern by ID
    pub fn get_pattern(&self, id: &str) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }

    /// Get all patterns
    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Apply an extraction rule to a session
    async fn apply_rule(
        &self,
        session: &Session,
        rule: &ExtractionRule,
    ) -> Result<Option<Pattern>> {
        let mut matches = Vec::new();
        let mut examples = Vec::new();

        // Analyze messages for pattern matches
        for message in &session.messages {
            // Only analyze user messages for triggers
            if message.role != MessageRole::User {
                continue;
            }

            let text = self.extract_text(message);
            if rule.pattern_regex.is_match(&text) {
                matches.push(text.clone());
                examples.push(text);
            }
        }

        // Check if we have enough examples
        if matches.len() < rule.min_examples {
            return Ok(None);
        }

        // Create pattern from matches
        let pattern_id = format!("{:?}-{}", rule.category, uuid::Uuid::new_v4());
        let mut pattern = Pattern::new(
            pattern_id,
            rule.name.clone(),
            rule.category.clone(),
            format!("Extracted from session: {}", rule.name),
        );

        pattern.examples = examples;
        pattern.confidence = self.calculate_confidence(&pattern, 1);

        // Create trigger condition
        let mut trigger = TriggerCondition::new(
            uuid::Uuid::new_v4().to_string(),
            rule.trigger_type.clone(),
            rule.pattern_regex.as_str().to_string(),
        )
        .with_confidence_threshold(self.min_confidence);

        for (key, value) in &rule.context_requirements {
            trigger = trigger.with_context_requirement(key.clone(), value.clone());
        }

        pattern.trigger_conditions.push(trigger);

        Ok(Some(pattern))
    }

    /// Extract plain text from a message
    fn extract_text(&self, message: &Message) -> String {
        message
            .parts
            .iter()
            .filter_map(|part| {
                if let rustycode_session::MessagePart::Text { content } = part {
                    Some(content.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Calculate confidence score for a pattern
    fn calculate_confidence(&self, pattern: &Pattern, session_count: usize) -> f32 {
        let mut confidence = 0.5;

        // More examples = higher confidence
        let example_factor = (pattern.examples.len() as f32).log10() / 5.0;
        confidence += example_factor * 0.2;

        // More trigger conditions = higher confidence
        let trigger_factor = (pattern.trigger_conditions.len() as f32).log10() / 3.0;
        confidence += trigger_factor * 0.1;

        // More metadata = higher confidence
        let metadata_factor = (pattern.metadata.len() as f32).log10() / 4.0;
        confidence += metadata_factor * 0.1;

        // Found across multiple sessions = higher confidence
        if session_count > 1 {
            let session_factor = (session_count as f32).log10() / 5.0;
            confidence += session_factor * 0.2;
        }

        confidence.clamp(0.0, 1.0)
    }

    /// Built-in extraction rules
    fn builtin_rules() -> Vec<ExtractionRule> {
        vec![
            // Error handling patterns
            ExtractionRule {
                id: "error-handling-1".to_string(),
                name: "Rust Error Handling with ?".to_string(),
                category: PatternCategory::Coding,
                trigger_type: TriggerType::CodePattern,
                pattern_regex: Regex::new(r"\bunwrap\(\)|expect\(").unwrap(),
                context_requirements: {
                    let mut map = HashMap::new();
                    map.insert("language".to_string(), "rust".to_string());
                    map
                },
                min_examples: 2,
            },
            // Debugging patterns
            ExtractionRule {
                id: "debugging-1".to_string(),
                name: "Debug Build Errors".to_string(),
                category: PatternCategory::Debugging,
                trigger_type: TriggerType::ErrorPattern,
                pattern_regex: Regex::new(r"error\[E\d+]").unwrap(),
                context_requirements: {
                    let mut map = HashMap::new();
                    map.insert("context".to_string(), "compilation".to_string());
                    map
                },
                min_examples: 1,
            },
            // Testing patterns
            ExtractionRule {
                id: "testing-1".to_string(),
                name: "Unit Test Creation".to_string(),
                category: PatternCategory::Testing,
                trigger_type: TriggerType::Intent,
                pattern_regex: Regex::new(r"(?i)(write|create|add).*test").unwrap(),
                context_requirements: {
                    let mut map = HashMap::new();
                    map.insert("language".to_string(), "rust".to_string());
                    map
                },
                min_examples: 2,
            },
            // Refactoring patterns
            ExtractionRule {
                id: "refactoring-1".to_string(),
                name: "Extract Function".to_string(),
                category: PatternCategory::Refactoring,
                trigger_type: TriggerType::Intent,
                pattern_regex: Regex::new(r"(?i)extract.*function|method").unwrap(),
                context_requirements: HashMap::new(),
                min_examples: 2,
            },
            // Async patterns
            ExtractionRule {
                id: "async-1".to_string(),
                name: "Async/Await Usage".to_string(),
                category: PatternCategory::Coding,
                trigger_type: TriggerType::CodePattern,
                pattern_regex: Regex::new(r"\.await|async fn").unwrap(),
                context_requirements: {
                    let mut map = HashMap::new();
                    map.insert("language".to_string(), "rust".to_string());
                    map
                },
                min_examples: 2,
            },
            // Documentation patterns
            ExtractionRule {
                id: "docs-1".to_string(),
                name: "Generate Documentation".to_string(),
                category: PatternCategory::Documentation,
                trigger_type: TriggerType::Intent,
                pattern_regex: Regex::new(r"(?i)(add|generate|write).*docs|documentation").unwrap(),
                context_requirements: HashMap::new(),
                min_examples: 2,
            },
            // Optimization patterns
            ExtractionRule {
                id: "optimization-1".to_string(),
                name: "Performance Optimization".to_string(),
                category: PatternCategory::Optimization,
                trigger_type: TriggerType::Intent,
                pattern_regex: Regex::new(r"(?i)(optimize|improve.*performance|speed up)").unwrap(),
                context_requirements: HashMap::new(),
                min_examples: 2,
            },
        ]
    }
}

impl Default for InstinctExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_new() {
        let ext = InstinctExtractor::new();
        assert!(ext.patterns.is_empty());
        assert!((ext.min_confidence - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_extractor_default() {
        let ext = InstinctExtractor::default();
        assert!(ext.patterns.is_empty());
    }

    #[test]
    fn test_extractor_with_min_confidence() {
        let ext = InstinctExtractor::new().with_min_confidence(0.6);
        assert!((ext.min_confidence - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_learn_and_get_pattern() {
        let mut ext = InstinctExtractor::new();
        let pattern = Pattern::new(
            "test-pattern".into(),
            "Test".into(),
            PatternCategory::Coding,
            "desc".into(),
        );

        ext.learn_pattern(pattern);
        assert_eq!(ext.patterns().len(), 1);

        let found = ext.get_pattern("test-pattern");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test");
    }

    #[test]
    fn test_get_pattern_not_found() {
        let ext = InstinctExtractor::new();
        assert!(ext.get_pattern("nonexistent").is_none());
    }

    #[test]
    fn test_learn_duplicate_pattern_does_not_add() {
        let mut ext = InstinctExtractor::new();
        let p1 = Pattern::new(
            "dup".into(),
            "First".into(),
            PatternCategory::Testing,
            "d".into(),
        );
        let p2 = Pattern::new(
            "dup".into(),
            "Second".into(),
            PatternCategory::Coding,
            "d".into(),
        );

        ext.learn_pattern(p1);
        ext.learn_pattern(p2);
        // Should not grow (existing is found but merge is a no-op)
        assert_eq!(ext.patterns().len(), 1);
        // Name should remain "First" since merge is no-op
        assert_eq!(ext.get_pattern("dup").unwrap().name, "First");
    }

    #[test]
    fn test_builtin_rules_count() {
        let ext = InstinctExtractor::new();
        // Should have extraction rules loaded
        assert!(!ext.extraction_rules.is_empty());
    }
}
