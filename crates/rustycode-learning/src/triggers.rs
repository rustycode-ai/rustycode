use crate::patterns::{TriggerCondition, TriggerType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Context information for matching triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Text content (user message, code, etc.)
    pub text: Option<String>,
    /// File path if applicable
    pub file_path: Option<String>,
    /// File type/extension
    pub file_type: Option<String>,
    /// Language
    pub language: Option<String>,
    /// Error message if present
    pub error: Option<String>,
    /// User intent
    pub intent: Option<String>,
    /// Additional context data
    pub metadata: HashMap<String, String>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            text: None,
            file_path: None,
            file_type: None,
            language: None,
            error: None,
            intent: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    pub fn with_file_path(mut self, path: String) -> Self {
        self.file_path = Some(path);
        self
    }

    pub fn with_file_type(mut self, file_type: String) -> Self {
        self.file_type = Some(file_type);
        self
    }

    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    pub fn with_intent(mut self, intent: String) -> Self {
        self.intent = Some(intent);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        match key {
            "text" => self.text.as_ref(),
            "file_path" => self.file_path.as_ref(),
            "file_type" => self.file_type.as_ref(),
            "language" => self.language.as_ref(),
            "error" => self.error.as_ref(),
            "intent" => self.intent.as_ref(),
            _ => self.metadata.get(key),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Matches triggers against contexts
pub struct TriggerMatcher {
    matchers: HashMap<String, Box<dyn TriggerMatcherFn + Send + Sync>>,
}

/// Trait for trigger matching functions
pub trait TriggerMatcherFn: Fn(&TriggerCondition, &Context) -> bool + Send + Sync {}

impl<F> TriggerMatcherFn for F where F: Fn(&TriggerCondition, &Context) -> bool + Send + Sync {}

impl TriggerMatcher {
    pub fn new() -> Self {
        let mut matchers: HashMap<String, Box<dyn TriggerMatcherFn + Send + Sync>> = HashMap::new();

        // Add built-in matchers
        matchers.insert(
            format!("{:?}", TriggerType::Keyword),
            Box::new(Self::match_keyword),
        );
        matchers.insert(
            format!("{:?}", TriggerType::FileType),
            Box::new(Self::match_file_type),
        );
        matchers.insert(
            format!("{:?}", TriggerType::ErrorPattern),
            Box::new(Self::match_error_pattern),
        );
        matchers.insert(
            format!("{:?}", TriggerType::CodePattern),
            Box::new(Self::match_code_pattern),
        );
        matchers.insert(
            format!("{:?}", TriggerType::Context),
            Box::new(Self::match_context),
        );
        matchers.insert(
            format!("{:?}", TriggerType::Intent),
            Box::new(Self::match_intent),
        );

        Self { matchers }
    }

    /// Check if a trigger matches the given context
    pub fn matches(&self, trigger: &TriggerCondition, context: &Context) -> bool {
        // Check confidence threshold
        // This is handled in storage, but we can add additional checks here

        // Get matcher for trigger type
        let trigger_type = format!("{:?}", trigger.trigger_type);
        if let Some(matcher) = self.matchers.get(&trigger_type) {
            matcher(trigger, context)
        } else {
            // Default to true for unknown trigger types
            true
        }
    }

    /// Match keyword triggers
    fn match_keyword(trigger: &TriggerCondition, context: &Context) -> bool {
        if let Some(text) = &context.text {
            regex::Regex::new(&trigger.pattern)
                .map(|re| re.is_match(text))
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Match file type triggers
    fn match_file_type(trigger: &TriggerCondition, context: &Context) -> bool {
        if let Some(file_type) = &context.file_type {
            trigger.pattern == *file_type
        } else {
            false
        }
    }

    /// Match error pattern triggers
    fn match_error_pattern(trigger: &TriggerCondition, context: &Context) -> bool {
        if let Some(error) = &context.error {
            regex::Regex::new(&trigger.pattern)
                .map(|re| re.is_match(error))
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Match code pattern triggers
    fn match_code_pattern(trigger: &TriggerCondition, context: &Context) -> bool {
        if let Some(text) = &context.text {
            regex::Regex::new(&trigger.pattern)
                .map(|re| re.is_match(text))
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Match context triggers
    fn match_context(trigger: &TriggerCondition, context: &Context) -> bool {
        // Check all context requirements
        for (key, required_value) in &trigger.context_requirements {
            if let Some(actual_value) = context.get(key) {
                if actual_value != required_value {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Match intent triggers
    fn match_intent(trigger: &TriggerCondition, context: &Context) -> bool {
        if let Some(intent) = &context.intent {
            regex::Regex::new(&trigger.pattern)
                .map(|re| re.is_match(intent))
                .unwrap_or(false)
        } else if let Some(text) = &context.text {
            // Fall back to text if no explicit intent
            regex::Regex::new(&trigger.pattern)
                .map(|re| re.is_match(text))
                .unwrap_or(false)
        } else {
            false
        }
    }
}

impl Default for TriggerMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_builder() {
        let context = Context::new()
            .with_text("Hello world".to_string())
            .with_language("rust".to_string());

        assert_eq!(context.text, Some("Hello world".to_string()));
        assert_eq!(context.language, Some("rust".to_string()));
    }

    #[test]
    fn test_keyword_matcher() {
        let matcher = TriggerMatcher::new();
        let trigger = TriggerCondition::new(
            "test".to_string(),
            TriggerType::Keyword,
            r"\bhello\b".to_string(),
        );

        let context = Context::new().with_text("hello world".to_string());
        assert!(matcher.matches(&trigger, &context));

        let context2 = Context::new().with_text("goodbye world".to_string());
        assert!(!matcher.matches(&trigger, &context2));
    }

    #[test]
    fn test_file_type_matcher() {
        let matcher = TriggerMatcher::new();
        let trigger =
            TriggerCondition::new("test".to_string(), TriggerType::FileType, "rs".to_string());

        let context = Context::new().with_file_type("rs".to_string());
        assert!(matcher.matches(&trigger, &context));

        let context2 = Context::new().with_file_type("js".to_string());
        assert!(!matcher.matches(&trigger, &context2));
    }

    #[test]
    fn test_context_default() {
        let ctx = Context::default();
        assert!(ctx.text.is_none());
        assert!(ctx.file_path.is_none());
        assert!(ctx.file_type.is_none());
        assert!(ctx.language.is_none());
        assert!(ctx.error.is_none());
        assert!(ctx.intent.is_none());
        assert!(ctx.metadata.is_empty());
    }

    #[test]
    fn test_context_builder_all_fields() {
        let ctx = Context::new()
            .with_text("code".into())
            .with_file_path("/tmp/test.rs".into())
            .with_file_type("rs".into())
            .with_language("rust".into())
            .with_error("panic".into())
            .with_intent("fix".into())
            .with_metadata("key1".into(), "val1".into());

        assert_eq!(ctx.text.as_deref(), Some("code"));
        assert_eq!(ctx.file_path.as_deref(), Some("/tmp/test.rs"));
        assert_eq!(ctx.file_type.as_deref(), Some("rs"));
        assert_eq!(ctx.language.as_deref(), Some("rust"));
        assert_eq!(ctx.error.as_deref(), Some("panic"));
        assert_eq!(ctx.intent.as_deref(), Some("fix"));
        assert_eq!(ctx.metadata.get("key1").map(|s| s.as_str()), Some("val1"));
    }

    #[test]
    fn test_context_get_named_fields() {
        let ctx = Context::new()
            .with_text("hello".into())
            .with_language("go".into())
            .with_error("oops".into());

        assert_eq!(ctx.get("text").map(|s| s.as_str()), Some("hello"));
        assert_eq!(ctx.get("language").map(|s| s.as_str()), Some("go"));
        assert_eq!(ctx.get("error").map(|s| s.as_str()), Some("oops"));
        assert!(ctx.get("file_path").is_none());
    }

    #[test]
    fn test_context_get_metadata() {
        let ctx = Context::new().with_metadata("custom".into(), "value".into());
        assert_eq!(ctx.get("custom").map(|s| s.as_str()), Some("value"));
    }

    #[test]
    fn test_context_serialization_roundtrip() {
        let ctx = Context::new()
            .with_text("test".into())
            .with_language("rust".into());
        let json = serde_json::to_string(&ctx).unwrap();
        let decoded: Context = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.text, Some("test".into()));
        assert_eq!(decoded.language, Some("rust".into()));
    }

    #[test]
    fn test_error_pattern_matcher() {
        let matcher = TriggerMatcher::new();
        let trigger = TriggerCondition::new(
            "err".into(),
            TriggerType::ErrorPattern,
            r"cannot find value".into(),
        );
        let ctx = Context::new().with_error("cannot find value `x` in this scope".into());
        assert!(matcher.matches(&trigger, &ctx));

        let ctx_no_err = Context::new().with_text("no error here".into());
        assert!(!matcher.matches(&trigger, &ctx_no_err));
    }

    #[test]
    fn test_code_pattern_matcher() {
        let matcher = TriggerMatcher::new();
        let trigger = TriggerCondition::new(
            "unwrap".into(),
            TriggerType::CodePattern,
            r"\.unwrap\(\)".into(),
        );
        let ctx = Context::new().with_text("let x = result.unwrap();".into());
        assert!(matcher.matches(&trigger, &ctx));

        let ctx_safe = Context::new().with_text("let x = result?;".into());
        assert!(!matcher.matches(&trigger, &ctx_safe));
    }

    #[test]
    fn test_intent_matcher_with_intent() {
        let matcher = TriggerMatcher::new();
        let trigger = TriggerCondition::new(
            "refactor".into(),
            TriggerType::Intent,
            r"(?i)refactor".into(),
        );
        let ctx = Context::new().with_intent("Refactor this code".into());
        assert!(matcher.matches(&trigger, &ctx));
    }

    #[test]
    fn test_intent_matcher_fallback_to_text() {
        let matcher = TriggerMatcher::new();
        let trigger = TriggerCondition::new(
            "refactor".into(),
            TriggerType::Intent,
            r"(?i)refactor".into(),
        );
        // No intent set, but text contains the word
        let ctx = Context::new().with_text("please refactor this function".into());
        assert!(matcher.matches(&trigger, &ctx));
    }

    #[test]
    fn test_trigger_matcher_default() {
        let matcher = TriggerMatcher::default();
        let trigger = TriggerCondition::new("kw".into(), TriggerType::Keyword, r"test".into());
        let ctx = Context::new().with_text("test".into());
        assert!(matcher.matches(&trigger, &ctx));
    }

    #[test]
    fn test_context_debug_format() {
        let ctx = Context::new()
            .with_text("debug".into())
            .with_language("rust".into());
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("Context"));
    }

    #[test]
    fn test_context_clone() {
        let ctx = Context::new().with_text("clone me".into());
        let cloned = ctx.clone();
        assert_eq!(cloned.text, Some("clone me".into()));
    }
}
