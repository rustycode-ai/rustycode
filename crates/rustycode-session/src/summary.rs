//! Session summarization for context preservation
//!
//! This module provides summarization capabilities to preserve important
//! context when compacting sessions.

use crate::session::Session;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during summary generation
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SummaryError {
    #[error("No messages to summarize")]
    EmptySession,

    #[error("LLM provider error: {0}")]
    LlmError(String),

    #[error("Summary generation failed: {0}")]
    GenerationError(String),
}

/// Summary of a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// Summary text
    pub text: String,

    /// Messages that were summarized
    pub message_count: usize,

    /// Token count before summarization
    pub original_tokens: usize,

    /// Token count after summarization
    pub summary_tokens: usize,

    /// Key points extracted
    pub key_points: Vec<String>,

    /// Files mentioned
    pub files_mentioned: Vec<String>,

    /// Decisions made
    pub decisions_made: Vec<String>,

    /// Summary generation timestamp
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl Summary {
    /// Calculate token reduction percentage
    pub fn reduction_percentage(&self) -> f64 {
        if self.original_tokens == 0 {
            0.0
        } else {
            ((self.original_tokens - self.summary_tokens) as f64 / self.original_tokens as f64)
                * 100.0
        }
    }

    /// Convert to a message part
    pub fn to_message_content(&self) -> String {
        format!(
            "<summary>\n{}\n\nKey Points:\n- {}\n\nFiles: {}\n\nDecisions: {}\n</summary>",
            self.text,
            self.key_points.join("\n- "),
            self.files_mentioned.join(", "),
            self.decisions_made.join(", ")
        )
    }
}

/// Summary generator configuration
#[derive(Debug, Clone)]
pub struct SummaryConfig {
    /// Maximum summary length in tokens
    pub max_tokens: usize,

    /// Whether to extract key points
    pub extract_key_points: bool,

    /// Whether to extract file mentions
    pub extract_files: bool,

    /// Whether to extract decisions
    pub extract_decisions: bool,

    /// Custom summary prompt (optional)
    pub custom_prompt: Option<String>,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            extract_key_points: true,
            extract_files: true,
            extract_decisions: true,
            custom_prompt: None,
        }
    }
}

/// Summary generator
pub struct SummaryGenerator {
    config: SummaryConfig,
}

impl SummaryGenerator {
    pub const fn new(config: SummaryConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(SummaryConfig::default())
    }

    /// Generate a summary of the session
    pub async fn generate(&self, session: &Session) -> Result<Summary, SummaryError> {
        if session.messages.is_empty() {
            return Err(SummaryError::EmptySession);
        }

        let original_tokens = session.estimate_tokens();

        // For now, create a simple summary without LLM
        // In a full implementation, this would use an LLM provider
        let text = self.generate_simple_summary(session)?;

        let key_points = if self.config.extract_key_points {
            self.extract_key_points(session)
        } else {
            Vec::new()
        };

        let files_mentioned = if self.config.extract_files {
            session.context.files_touched.clone()
        } else {
            Vec::new()
        };

        let decisions_made = if self.config.extract_decisions {
            session.context.decisions.clone()
        } else {
            Vec::new()
        };

        let summary_tokens = text.len() / 4; // Rough estimate

        Ok(Summary {
            text,
            message_count: session.message_count(),
            original_tokens,
            summary_tokens,
            key_points,
            files_mentioned,
            decisions_made,
            generated_at: chrono::Utc::now(),
        })
    }

    /// Generate a simple summary without LLM
    fn generate_simple_summary(&self, session: &Session) -> Result<String, SummaryError> {
        let mut summary = String::new();

        // Add task
        if let Some(ref task) = session.context.task {
            summary.push_str(&format!("Task: {task}\n"));
        }

        // Add phase
        if let Some(ref phase) = session.context.current_phase {
            summary.push_str(&format!("Current Phase: {phase}\n"));
        }

        // Add conversation overview
        summary.push_str(&format!(
            "Conversation contains {} messages spanning {} turns.\n",
            session.message_count(),
            session.message_count() / 2
        ));

        // Add files touched
        if !session.context.files_touched.is_empty() {
            summary.push_str(&format!(
                "Files worked on: {}\n",
                session.context.files_touched.join(", ")
            ));
        }

        // Add decisions
        if !session.context.decisions.is_empty() {
            summary.push_str(&format!(
                "Key decisions: {}\n",
                session.context.decisions.join("; ")
            ));
        }

        // Add error resolutions
        if !session.context.errors_resolved.is_empty() {
            summary.push_str(&format!(
                "Errors resolved: {}\n",
                session.context.errors_resolved.join("; ")
            ));
        }

        Ok(summary)
    }

    /// Extract key points from session
    fn extract_key_points(&self, session: &Session) -> Vec<String> {
        let mut points = Vec::new();

        // Extract from context
        if let Some(ref task) = session.context.task {
            points.push(format!("Working on: {task}"));
        }

        if let Some(ref phase) = session.context.current_phase {
            points.push(format!("Current phase: {phase}"));
        }

        // Extract from messages (simple heuristic)
        for message in &session.messages {
            let text = message.text();

            // Look for common patterns
            if text.to_lowercase().contains("decided to") {
                if let Some(idx) = text.find("decided to") {
                    let start = idx;
                    let end = (start + 100).min(text.len());
                    let snippet = text[start..end].trim();
                    points.push(format!("Decision: {snippet}"));
                }
            }

            if text.to_lowercase().contains("implemented") {
                if let Some(idx) = text.find("implemented") {
                    let start = idx;
                    let end = (start + 100).min(text.len());
                    let snippet = text[start..end].trim();
                    points.push(format!("Implementation: {snippet}"));
                }
            }
        }

        // Limit to top 10 points
        points.truncate(10);
        points
    }

    /// Create a summary from text (for testing/manual use)
    pub fn from_text(text: impl Into<String>, session: &Session) -> Summary {
        let text = text.into();
        let original_tokens = session.estimate_tokens();
        let summary_tokens = text.len() / 4;

        Summary {
            text,
            message_count: session.message_count(),
            original_tokens,
            summary_tokens,
            key_points: Vec::new(),
            files_mentioned: session.context.files_touched.clone(),
            decisions_made: session.context.decisions.clone(),
            generated_at: chrono::Utc::now(),
        }
    }
}

/// Generate a default summary prompt
pub fn default_summary_prompt() -> String {
    r"You have been working on the task described above but have not yet completed it. Write a continuation summary that will allow you (or another instance of yourself) to resume work efficiently in a future context window where the conversation history will be replaced with this summary. Your summary should be structured, concise, and actionable. Include:

1. **Task Overview**
   - The user's core request and success criteria
   - Any clarifications or constraints they specified

2. **Current State**
   - What has been completed so far
   - Files created, modified, or analyzed (with paths if relevant)
   - Key outputs or artifacts produced

3. **Important Discoveries**
   - Technical constraints or requirements uncovered
   - Decisions made and their rationale
   - Errors encountered and how they were resolved
   - What approaches were tried that didn't work (and why)

4. **Next Steps**
   - Specific actions needed to complete the task
   - Any blockers or open questions to resolve
   - Priority order if multiple steps remain

5. **Context to Preserve**
   - User preferences or style requirements
   - Domain-specific details that aren't obvious
   - Any promises made to the user

Be concise but complete—err on the side of including information that would prevent duplicate work or repeated mistakes. Write in a way that enables immediate resumption of the task.

Wrap your summary in <summary></summary> tags.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;

    fn create_test_session() -> Session {
        let mut session = Session::new("Test Session");
        session.set_task("Implement feature X");
        session.set_phase("Development");

        session.add_message(Message::user("I need feature X implemented"));
        session.add_message(Message::assistant("I'll implement feature X for you"));

        session.touch_file("src/main.rs");
        session.record_decision("Use async/await pattern");

        session
    }

    #[test]
    fn test_summary_generation() {
        let session = create_test_session();
        let generator = SummaryGenerator::with_defaults();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let summary = generator.generate(&session).await.unwrap();

            assert!(!summary.text.is_empty());
            assert_eq!(summary.message_count, 2);
            assert!(summary.original_tokens > 0);
            assert!(summary.files_mentioned.contains(&"src/main.rs".to_string()));
            assert!(summary
                .decisions_made
                .contains(&"Use async/await pattern".to_string()));
        });
    }

    #[test]
    fn test_summary_from_text() {
        let session = create_test_session();
        let summary = SummaryGenerator::from_text("This is a test summary", &session);

        assert_eq!(summary.text, "This is a test summary");
        assert_eq!(summary.message_count, 2);
    }

    #[test]
    fn test_empty_session_summary() {
        let session = Session::new("Empty");
        let generator = SummaryGenerator::with_defaults();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let result = generator.generate(&session).await;
            assert!(matches!(result, Err(SummaryError::EmptySession)));
        });
    }

    #[test]
    fn test_summary_to_message_content() {
        let summary = Summary {
            text: "Task completed".to_string(),
            message_count: 10,
            original_tokens: 1000,
            summary_tokens: 100,
            key_points: vec!["Point 1".to_string(), "Point 2".to_string()],
            files_mentioned: vec!["file.rs".to_string()],
            decisions_made: vec!["Decision 1".to_string()],
            generated_at: chrono::Utc::now(),
        };

        let content = summary.to_message_content();

        assert!(content.contains("<summary>"));
        assert!(content.contains("Task completed"));
        assert!(content.contains("Point 1"));
        assert!(content.contains("file.rs"));
        assert!(content.contains("Decision 1"));
    }

    #[test]
    fn test_summary_reduction_percentage() {
        let summary = Summary {
            text: "Test".to_string(),
            message_count: 1,
            original_tokens: 1000,
            summary_tokens: 100,
            key_points: Vec::new(),
            files_mentioned: Vec::new(),
            decisions_made: Vec::new(),
            generated_at: chrono::Utc::now(),
        };

        assert_eq!(summary.reduction_percentage(), 90.0);
    }

    #[test]
    fn test_extract_key_points() {
        let session = create_test_session();
        let generator = SummaryGenerator::with_defaults();

        let points = generator.extract_key_points(&session);

        assert!(!points.is_empty());
        assert!(points.iter().any(|p| p.contains("Implement feature X")));
    }

    #[test]
    fn test_default_summary_prompt() {
        let prompt = default_summary_prompt();
        assert!(prompt.contains("Task Overview"));
        assert!(prompt.contains("Current State"));
        assert!(prompt.contains("Next Steps"));
        assert!(prompt.contains("<summary>"));
    }

    // --- SummaryError display tests ---

    #[test]
    fn test_summary_error_display() {
        assert_eq!(
            SummaryError::EmptySession.to_string(),
            "No messages to summarize"
        );
        assert_eq!(
            SummaryError::LlmError("timeout".to_string()).to_string(),
            "LLM provider error: timeout"
        );
        assert_eq!(
            SummaryError::GenerationError("bad input".to_string()).to_string(),
            "Summary generation failed: bad input"
        );
    }

    // --- Summary serde roundtrip ---

    #[test]
    fn test_summary_serde_roundtrip() {
        let summary = Summary {
            text: "Test summary".to_string(),
            message_count: 5,
            original_tokens: 500,
            summary_tokens: 50,
            key_points: vec!["Point 1".to_string()],
            files_mentioned: vec!["a.rs".to_string()],
            decisions_made: vec!["Use X".to_string()],
            generated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let de: Summary = serde_json::from_str(&json).unwrap();
        assert_eq!(de.text, "Test summary");
        assert_eq!(de.message_count, 5);
        assert_eq!(de.original_tokens, 500);
        assert_eq!(de.summary_tokens, 50);
        assert_eq!(de.key_points, vec!["Point 1".to_string()]);
        assert_eq!(de.files_mentioned, vec!["a.rs".to_string()]);
        assert_eq!(de.decisions_made, vec!["Use X".to_string()]);
    }

    // --- Summary edge cases ---

    #[test]
    fn test_summary_reduction_percentage_zero_original_tokens() {
        let summary = Summary {
            text: "Test".to_string(),
            message_count: 1,
            original_tokens: 0,
            summary_tokens: 0,
            key_points: Vec::new(),
            files_mentioned: Vec::new(),
            decisions_made: Vec::new(),
            generated_at: chrono::Utc::now(),
        };
        assert_eq!(summary.reduction_percentage(), 0.0);
    }

    #[test]
    fn test_summary_to_message_content_empty_collections() {
        let summary = Summary {
            text: "Done".to_string(),
            message_count: 0,
            original_tokens: 0,
            summary_tokens: 0,
            key_points: Vec::new(),
            files_mentioned: Vec::new(),
            decisions_made: Vec::new(),
            generated_at: chrono::Utc::now(),
        };
        let content = summary.to_message_content();
        assert!(content.contains("<summary>"));
        assert!(content.contains("</summary>"));
        assert!(content.contains("Done"));
    }

    #[test]
    fn test_summary_to_message_content_multiple_key_points() {
        let summary = Summary {
            text: "Test".to_string(),
            message_count: 2,
            original_tokens: 100,
            summary_tokens: 10,
            key_points: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            files_mentioned: vec!["f1.rs".to_string(), "f2.rs".to_string()],
            decisions_made: vec!["D1".to_string()],
            generated_at: chrono::Utc::now(),
        };
        let content = summary.to_message_content();
        assert!(content.contains("A"));
        assert!(content.contains("B"));
        assert!(content.contains("C"));
        assert!(content.contains("f1.rs"));
        assert!(content.contains("f2.rs"));
        assert!(content.contains("D1"));
    }

    // --- SummaryConfig tests ---

    #[test]
    fn test_summary_config_default() {
        let config = SummaryConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert!(config.extract_key_points);
        assert!(config.extract_files);
        assert!(config.extract_decisions);
        assert!(config.custom_prompt.is_none());
    }

    // --- SummaryGenerator tests ---

    #[test]
    fn test_summary_generator_with_defaults() {
        let generator = SummaryGenerator::with_defaults();
        // Should not panic; exercise the constructor
        drop(generator);
    }

    #[test]
    fn test_summary_generator_with_custom_config() {
        let config = SummaryConfig {
            max_tokens: 2048,
            extract_key_points: false,
            extract_files: false,
            extract_decisions: false,
            custom_prompt: Some("Summarize this".to_string()),
        };
        let generator = SummaryGenerator::new(config);
        drop(generator);
    }

    #[test]
    fn test_summary_generation_extracts_nothing_when_disabled() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hello"));
        session.touch_file("src/a.rs");
        session.record_decision("Use X");

        let config = SummaryConfig {
            max_tokens: 4096,
            extract_key_points: false,
            extract_files: false,
            extract_decisions: false,
            custom_prompt: None,
        };
        let generator = SummaryGenerator::new(config);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let summary = generator.generate(&session).await.unwrap();
            assert!(summary.key_points.is_empty());
            assert!(summary.files_mentioned.is_empty());
            assert!(summary.decisions_made.is_empty());
        });
    }

    #[test]
    fn test_summary_from_text_with_context() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Do stuff"));
        session.touch_file("src/main.rs");
        session.record_decision("Go async");

        let summary = SummaryGenerator::from_text("Custom summary text", &session);
        assert_eq!(summary.text, "Custom summary text");
        assert!(summary.files_mentioned.contains(&"src/main.rs".to_string()));
        assert!(summary.decisions_made.contains(&"Go async".to_string()));
        assert!(summary.key_points.is_empty());
    }

    #[test]
    fn test_summary_generation_includes_task_and_phase() {
        let mut session = Session::new("Test");
        session.set_task("Build feature Y");
        session.set_phase("Testing");
        session.add_message(Message::user("Start"));

        let generator = SummaryGenerator::with_defaults();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let summary = generator.generate(&session).await.unwrap();
            assert!(summary.text.contains("Build feature Y"));
            assert!(summary.text.contains("Testing"));
        });
    }

    #[test]
    fn test_summary_generation_includes_errors_resolved() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Fix bug"));
        session.record_error_resolution("compile error", "added import");

        let generator = SummaryGenerator::with_defaults();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let summary = generator.generate(&session).await.unwrap();
            assert!(summary.text.contains("compile error"));
        });
    }

    #[test]
    fn test_extract_key_points_with_decided_to() {
        let mut session = Session::new("Test");
        session.add_message(Message::assistant(
            "After analysis, we decided to use PostgreSQL for the database",
        ));

        let generator = SummaryGenerator::with_defaults();
        let points = generator.extract_key_points(&session);
        assert!(points.iter().any(|p| p.contains("decided to")));
    }

    #[test]
    fn test_extract_key_points_with_implemented() {
        let mut session = Session::new("Test");
        session.add_message(Message::assistant("I implemented the new caching layer"));

        let generator = SummaryGenerator::with_defaults();
        let points = generator.extract_key_points(&session);
        assert!(points.iter().any(|p| p.contains("implemented")));
    }

    #[test]
    fn test_extract_key_points_truncated_to_ten() {
        let mut session = Session::new("Test");
        // Add many messages with keywords
        for i in 0..20 {
            session.add_message(Message::assistant(format!(
                "I decided to implement feature number {}",
                i
            )));
        }

        let generator = SummaryGenerator::with_defaults();
        let points = generator.extract_key_points(&session);
        assert!(points.len() <= 10);
    }
}
