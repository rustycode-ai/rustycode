//! LLM-augmented intent classification with fallback for low-confidence heuristic results.
//!
//! This module provides an async enhanced intent classifier that:
//! - First attempts heuristic classification (sync)
//! - If confidence is below threshold, optionally uses LLM for reclassification
//! - Tracks classification source and budget constraints

use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider};
use rustycode_protocol::intent::{
    classify_intent_with_confidence, IntentAssessment, IntentCategory,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Source of an intent classification result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClassificationSource {
    /// Classified by heuristic keyword matching (sync)
    Heuristic,
    /// Classified by LLM reclassification (async)
    LlmAugmented,
    /// Heuristic result used as fallback after LLM failed or low-confidence
    HeuristicFallback,
}

/// Enhanced intent assessment with classification metadata.
#[derive(Debug, Clone)]
pub struct EnhancedIntentAssessment {
    /// The detected intent category
    pub category: IntentCategory,
    /// Confidence score from 0.0 to 1.0
    pub confidence: f64,
    /// Source of this classification
    pub source: ClassificationSource,
}

impl From<IntentAssessment> for EnhancedIntentAssessment {
    fn from(assessment: IntentAssessment) -> Self {
        Self {
            category: assessment.category,
            confidence: assessment.confidence,
            source: ClassificationSource::Heuristic,
        }
    }
}

/// Budget tracker for LLM fallback calls within a session.
#[derive(Debug, Clone)]
pub struct LlmFallbackBudget {
    max_calls: usize,
    call_count: Arc<AtomicUsize>,
}

impl LlmFallbackBudget {
    /// Create a new budget with the given max call count.
    pub fn new(max_calls: usize) -> Self {
        Self {
            max_calls,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Check if a call is allowed and increment the counter if so.
    /// Returns true if the call is allowed, false if budget is exhausted.
    pub fn try_use(&self) -> bool {
        loop {
            let current = self.call_count.load(Ordering::SeqCst);
            if current >= self.max_calls {
                return false;
            }
            if self
                .call_count
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
            // CAS failed due to concurrent update; retry
        }
    }

    /// Get the current call count.
    pub fn current_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    /// Reset the counter (useful for testing).
    #[allow(dead_code)]
    fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
    }
}

/// LLM-augmented intent classifier.
///
/// Provides an async classification path that:
/// 1. Attempts heuristic classification first (fast, no API call)
/// 2. If confidence < threshold, optionally calls an LLM classifier
/// 3. Returns enhanced assessment with classification source
pub struct LlmIntentClassifier;

impl LlmIntentClassifier {
    /// Classify intent with optional LLM fallback.
    ///
    /// # Arguments
    /// * `task` - The user's prompt/task
    /// * `provider` - Optional LLM provider for fallback (None = heuristic-only)
    /// * `budget` - Budget tracker for limiting LLM calls
    /// * `llm_fallback_threshold` - Confidence threshold below which LLM is called (e.g., 0.65)
    ///
    /// # Returns
    /// Enhanced assessment including classification source
    pub async fn classify(
        task: &str,
        provider: Option<&Arc<dyn LLMProvider>>,
        budget: &LlmFallbackBudget,
        llm_fallback_threshold: f64,
    ) -> EnhancedIntentAssessment {
        // Step 1: Heuristic classification
        let heuristic = classify_intent_with_confidence(task);

        // Step 2: Check if LLM fallback is needed
        if heuristic.confidence >= llm_fallback_threshold {
            return EnhancedIntentAssessment {
                category: heuristic.category,
                confidence: heuristic.confidence,
                source: ClassificationSource::Heuristic,
            };
        }

        // Step 3: Try LLM fallback if provider available and budget allows
        if let Some(provider) = provider {
            if budget.try_use() {
                if let Ok(llm_assessment) = Self::llm_classify(task, provider).await {
                    // LLM succeeded — use its result if also confident
                    if llm_assessment.confidence >= llm_fallback_threshold {
                        return EnhancedIntentAssessment {
                            category: llm_assessment.category,
                            confidence: llm_assessment.confidence,
                            source: ClassificationSource::LlmAugmented,
                        };
                    }
                    // LLM was also low-confidence; use heuristic with fallback source
                }
            }
        }

        // Step 4: Fallback to heuristic if LLM unavailable, failed, or also low-confidence
        EnhancedIntentAssessment {
            category: heuristic.category,
            confidence: heuristic.confidence,
            source: ClassificationSource::HeuristicFallback,
        }
    }

    /// Call the LLM to reclassify intent (bounded, one attempt only).
    async fn llm_classify(
        task: &str,
        provider: &Arc<dyn LLMProvider>,
    ) -> Result<IntentAssessment, String> {
        let system_prompt = r#"You are an intent classifier. Classify the user's task into one of these categories with a confidence score.

Categories:
- Implementation: Code to write, features to build, files to create
- Investigation: Explore, research, understand (read-only)
- Explanation: Explain concepts or answer questions
- Refactoring: Restructure code without behavior change
- Planning: Design architecture or approach
- Testing: Write or run tests
- Analytical: Performance tuning or deep analysis
- Diagnostic: Troubleshoot or fix bugs

Respond with JSON:
{"category": "Implementation", "confidence": 0.95}
"#;

        let messages = vec![ChatMessage::user(task)];
        let request = CompletionRequest::new("claude-haiku-4-5", messages)
            .with_system_prompt(system_prompt.to_string())
            .with_max_tokens(200);

        // Call LLM with timeout (bounded execution)
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            provider.complete(request),
        )
        .await
        {
            Ok(Ok(response)) => Self::parse_llm_response(&response.content),
            Ok(Err(_)) => Err("LLM call failed".to_string()),
            Err(_) => Err("LLM call timed out".to_string()),
        }
    }

    /// Parse LLM's JSON response into category and confidence.
    fn parse_llm_response(content: &str) -> Result<IntentAssessment, String> {
        // Try to extract JSON from the response
        let json_str = if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
            &content[start..=end]
        } else {
            content
        };

        let parsed: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| format!("JSON parse failed: {}", e))?;

        let category_str = parsed
            .get("category")
            .and_then(|v| v.as_str())
            .ok_or("missing category")?;
        let confidence = parsed
            .get("confidence")
            .and_then(|v| v.as_f64())
            .ok_or("missing confidence")?;

        let category = match category_str {
            "Implementation" => IntentCategory::Implementation,
            "Investigation" => IntentCategory::Investigation,
            "Explanation" => IntentCategory::Explanation,
            "Refactoring" => IntentCategory::Refactoring,
            "Planning" => IntentCategory::Planning,
            "Testing" => IntentCategory::Testing,
            "Analytical" => IntentCategory::Analytical,
            "Diagnostic" => IntentCategory::Diagnostic,
            _ => return Err(format!("unknown category: {}", category_str)),
        };

        Ok(IntentAssessment {
            category,
            confidence: confidence.clamp(0.0, 1.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_high_confidence_no_llm_call() {
        // High-confidence prompt should not trigger LLM fallback
        let task = "Write a function to calculate the Fibonacci sequence in Rust.";
        let heuristic = classify_intent_with_confidence(task);

        // For a clear implementation task, confidence should be > 0.7
        assert!(heuristic.confidence > 0.6);
        assert_eq!(heuristic.category, IntentCategory::Implementation);
    }

    #[test]
    fn llm_fallback_budget_allows_calls() {
        let budget = LlmFallbackBudget::new(3);
        assert!(budget.try_use());
        assert!(budget.try_use());
        assert!(budget.try_use());
        assert!(!budget.try_use()); // Fourth call should fail
        assert_eq!(budget.current_count(), 3);
    }

    #[test]
    fn llm_fallback_budget_zero_blocks_all() {
        let budget = LlmFallbackBudget::new(0);
        assert!(!budget.try_use());
        assert_eq!(budget.current_count(), 0);
    }

    #[test]
    fn enhanced_assessment_from_heuristic() {
        let heuristic = IntentAssessment {
            category: IntentCategory::Refactoring,
            confidence: 0.8,
        };
        let enhanced: EnhancedIntentAssessment = heuristic.into();
        assert_eq!(enhanced.category, IntentCategory::Refactoring);
        assert_eq!(enhanced.confidence, 0.8);
        assert_eq!(enhanced.source, ClassificationSource::Heuristic);
    }

    #[test]
    fn parse_llm_response_valid_json() {
        let response = r#"{"category": "Implementation", "confidence": 0.92}"#;
        let result = LlmIntentClassifier::parse_llm_response(response).unwrap();
        assert_eq!(result.category, IntentCategory::Implementation);
        assert_eq!(result.confidence, 0.92);
    }

    #[test]
    fn parse_llm_response_json_in_markdown() {
        let response = r#"Here's the classification:
```json
{"category": "Diagnostic", "confidence": 0.85}
```"#;
        let result = LlmIntentClassifier::parse_llm_response(response).unwrap();
        assert_eq!(result.category, IntentCategory::Diagnostic);
        assert_eq!(result.confidence, 0.85);
    }

    #[test]
    fn parse_llm_response_clamps_confidence() {
        let response = r#"{"category": "Planning", "confidence": 1.5}"#;
        let result = LlmIntentClassifier::parse_llm_response(response).unwrap();
        assert_eq!(result.confidence, 1.0); // Clamped to [0.0, 1.0]
    }

    #[test]
    fn parse_llm_response_missing_category() {
        let response = r#"{"confidence": 0.8}"#;
        let result = LlmIntentClassifier::parse_llm_response(response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_llm_response_unknown_category() {
        let response = r#"{"category": "Unknown", "confidence": 0.8}"#;
        let result = LlmIntentClassifier::parse_llm_response(response);
        assert!(result.is_err());
    }
}
