//! Parse LLM responses into Thought objects

use crate::thinking::core::error::{Error, Result};
use crate::thinking::core::types::{Thought, ThoughtKind};
use serde::{Deserialize, Serialize};

/// Expected structure of LLM response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThoughtResponse {
    pub thoughts: Vec<ThoughtData>,
    #[serde(default)]
    pub relationships: Vec<RelationshipData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThoughtData {
    pub kind: String,
    pub content: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub reasoning: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelationshipData {
    pub from_idx: usize,
    pub to_idx: usize,
    #[serde(default)]
    pub edge_kind: String,
}

const fn default_confidence() -> f64 {
    0.7
}

/// Parser for LLM responses
pub struct ResponseParser;

impl ResponseParser {
    /// Parse JSON response into Thought objects.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid JSON or does not match
    /// the expected `ThoughtResponse` structure.
    pub fn parse_json(text: &str) -> Result<ThoughtResponse> {
        serde_json::from_str(text)
            .map_err(|e| Error::SerializationError(format!("Failed to parse response JSON: {e}")))
    }

    /// Convert response data to Thought objects.
    ///
    /// # Errors
    ///
    /// Returns an error if any thought conversion fails (currently infallible,
    /// but preserved for forward compatibility).
    pub fn to_thoughts(response: &ThoughtResponse) -> Result<Vec<Thought>> {
        Ok(response
            .thoughts
            .iter()
            .map(Self::data_to_thought)
            .collect())
    }

    /// Parse thought kind from string
    fn parse_kind(kind_str: &str) -> ThoughtKind {
        match kind_str.to_lowercase().as_str() {
            "initial" => ThoughtKind::Initial,
            "refinement" => ThoughtKind::Refinement,
            "synthesis" => ThoughtKind::Synthesis,
            "critique" => ThoughtKind::Critique,
            "resolution" => ThoughtKind::Resolution,
            // Default fallback for unknown or "analysis"
            _ => ThoughtKind::Analysis,
        }
    }

    /// Convert `ThoughtData` to Thought object
    fn data_to_thought(data: &ThoughtData) -> Thought {
        let kind = Self::parse_kind(&data.kind);
        let mut thought = Thought::new(kind, data.content.clone())
            .with_confidence(data.confidence.clamp(0.0, 1.0));

        if !data.reasoning.is_empty() {
            thought.metadata.evidence.push(data.reasoning.clone());
        }

        thought
    }

    /// Attempt to parse response as JSON, fallback to regex.
    ///
    /// # Errors
    ///
    /// Returns an error if neither JSON nor fallback parsing can extract
    /// any thoughts from the response text.
    pub fn parse_response(text: &str) -> Result<ThoughtResponse> {
        // Try JSON first
        match Self::parse_json(text) {
            Ok(response) => {
                // Validate response structure
                if !response.thoughts.is_empty() {
                    return Ok(response);
                }
            }
            Err(e) => {
                tracing::debug!("JSON parsing failed: {}, attempting fallback", e);
            }
        }

        // Fallback: try to extract thought-like patterns
        Self::parse_fallback(text)
    }

    /// Regex-based fallback parser for malformed responses
    fn parse_fallback(text: &str) -> Result<ThoughtResponse> {
        // Look for patterns like "...[Analysis] ... confidence: 0.8..."
        let mut thoughts = Vec::new();

        // Simple heuristic: split on paragraphs, look for confidence scores
        for paragraph in text.split("\n\n") {
            if paragraph.trim().is_empty() {
                continue;
            }

            // Try to extract confidence score
            let confidence = Self::extract_confidence(paragraph).unwrap_or(0.7);

            // Determine kind based on keywords
            let kind = if paragraph.to_lowercase().contains("synthesis")
                || paragraph.to_lowercase().contains("combined")
            {
                "Synthesis"
            } else if paragraph.to_lowercase().contains("critique")
                || paragraph.to_lowercase().contains("problem")
            {
                "Critique"
            } else if paragraph.to_lowercase().contains("resolution")
                || paragraph.to_lowercase().contains("conclusion")
            {
                "Resolution"
            } else {
                "Analysis"
            };

            thoughts.push(ThoughtData {
                kind: kind.to_string(),
                content: paragraph.trim().to_string(),
                confidence,
                reasoning: String::new(),
            });
        }

        if thoughts.is_empty() {
            return Err(Error::SerializationError(
                "Could not parse any thoughts from response".to_string(),
            ));
        }

        Ok(ThoughtResponse {
            thoughts,
            relationships: Vec::new(),
        })
    }

    /// Extract confidence score from text using simple regex
    fn extract_confidence(text: &str) -> Option<f64> {
        // Look for patterns: "confidence: 0.8", "confidence=0.8", etc.
        let re = regex::Regex::new(r"confidence[:\s=]+([0-9]\.[0-9]+)").ok()?;
        re.captures(text)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse::<f64>().ok())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_json_response() -> Result<()> {
        let json = r#"
        {
            "thoughts": [
                {
                    "kind": "Analysis",
                    "content": "First analysis",
                    "confidence": 0.8,
                    "reasoning": "Because..."
                }
            ]
        }
        "#;

        let response = ResponseParser::parse_json(json)?;
        assert_eq!(response.thoughts.len(), 1);
        assert_eq!(response.thoughts[0].content, "First analysis");

        Ok(())
    }

    #[test]
    fn test_data_to_thought() {
        let data = ThoughtData {
            kind: "Analysis".to_string(),
            content: "Test thought".to_string(),
            confidence: 0.8,
            reasoning: "Because...".to_string(),
        };

        let thought = ResponseParser::data_to_thought(&data);
        assert_eq!(thought.content, "Test thought");
        assert_eq!(thought.metadata.confidence, 0.8);
    }

    #[test]
    fn test_fallback_parsing() -> Result<()> {
        let text = "This is a test response.\n\nIt has multiple paragraphs.\nConfidence: 0.75";

        let response = ResponseParser::parse_fallback(text)?;
        assert!(!response.thoughts.is_empty());

        Ok(())
    }

    #[test]
    fn test_extract_confidence() {
        let text = "This is good. confidence: 0.85 right?";
        let conf = ResponseParser::extract_confidence(text);
        assert_eq!(conf, Some(0.85));
    }

    #[test]
    fn test_fallback_confidence_extraction() {
        let text = "confidence=0.72 in this solution";
        let conf = ResponseParser::extract_confidence(text);
        assert_eq!(conf, Some(0.72));
    }

    #[test]
    fn test_parse_response_prefers_json() -> Result<()> {
        let json = r#"
        {
            "thoughts": [
                {
                    "kind": "Analysis",
                    "content": "Valid JSON",
                    "confidence": 0.9
                }
            ]
        }
        "#;

        let response = ResponseParser::parse_response(json)?;
        assert_eq!(response.thoughts[0].content, "Valid JSON");

        Ok(())
    }

    #[test]
    fn test_parse_response_fallback_on_invalid_json() -> Result<()> {
        let invalid_json =
            "This is not JSON but has multiple paragraphs.\n\nSecond paragraph here.";

        let response = ResponseParser::parse_response(invalid_json)?;
        assert!(!response.thoughts.is_empty());

        Ok(())
    }

    #[test]
    fn test_confidence_clamping() {
        let data = ThoughtData {
            kind: "Analysis".to_string(),
            content: "Test".to_string(),
            confidence: 1.5, // Out of bounds
            reasoning: String::new(),
        };

        let thought = ResponseParser::data_to_thought(&data);
        assert_eq!(thought.metadata.confidence, 1.0); // Clamped
    }

    #[test]
    fn test_parse_kind_variants() {
        let data = |kind: &str| ThoughtData {
            kind: kind.to_string(),
            content: "test".to_string(),
            confidence: 0.5,
            reasoning: String::new(),
        };

        let initial = ResponseParser::data_to_thought(&data("initial"));
        assert!(matches!(initial.kind, ThoughtKind::Initial));

        let refinement = ResponseParser::data_to_thought(&data("refinement"));
        assert!(matches!(refinement.kind, ThoughtKind::Refinement));

        let synthesis = ResponseParser::data_to_thought(&data("synthesis"));
        assert!(matches!(synthesis.kind, ThoughtKind::Synthesis));

        let critique = ResponseParser::data_to_thought(&data("critique"));
        assert!(matches!(critique.kind, ThoughtKind::Critique));

        let resolution = ResponseParser::data_to_thought(&data("resolution"));
        assert!(matches!(resolution.kind, ThoughtKind::Resolution));
    }

    #[test]
    fn test_parse_kind_case_insensitive() {
        let data = ThoughtData {
            kind: "ANALYSIS".to_string(),
            content: "test".to_string(),
            confidence: 0.5,
            reasoning: String::new(),
        };
        let thought = ResponseParser::data_to_thought(&data);
        assert!(matches!(thought.kind, ThoughtKind::Analysis));
    }

    #[test]
    fn test_parse_kind_unknown_defaults_to_analysis() {
        let data = ThoughtData {
            kind: "unknown_kind".to_string(),
            content: "test".to_string(),
            confidence: 0.5,
            reasoning: String::new(),
        };
        let thought = ResponseParser::data_to_thought(&data);
        assert!(matches!(thought.kind, ThoughtKind::Analysis));
    }

    #[test]
    fn test_data_to_thought_with_reasoning_adds_evidence() {
        let data = ThoughtData {
            kind: "Analysis".to_string(),
            content: "test".to_string(),
            confidence: 0.7,
            reasoning: "because reasons".to_string(),
        };
        let thought = ResponseParser::data_to_thought(&data);
        assert_eq!(thought.metadata.evidence.len(), 1);
        assert_eq!(thought.metadata.evidence[0], "because reasons");
    }

    #[test]
    fn test_data_to_thought_empty_reasoning_no_evidence() {
        let data = ThoughtData {
            kind: "Analysis".to_string(),
            content: "test".to_string(),
            confidence: 0.7,
            reasoning: String::new(),
        };
        let thought = ResponseParser::data_to_thought(&data);
        assert!(thought.metadata.evidence.is_empty());
    }

    #[test]
    fn test_parse_json_invalid() {
        let result = ResponseParser::parse_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_empty_string() {
        let result = ResponseParser::parse_response("");
        assert!(result.is_err(), "Empty string should fail to parse");
    }

    #[test]
    fn test_parse_fallback_detects_synthesis() -> Result<()> {
        let text = "This is a synthesis of our findings.\n\nAnother paragraph.";
        let response = ResponseParser::parse_fallback(text)?;
        assert_eq!(response.thoughts[0].kind, "Synthesis");
        Ok(())
    }

    #[test]
    fn test_parse_fallback_detects_critique() -> Result<()> {
        let text = "This is a critique of the approach.\n\nThe problem is clear.";
        let response = ResponseParser::parse_fallback(text)?;
        assert_eq!(response.thoughts[0].kind, "Critique");
        Ok(())
    }

    #[test]
    fn test_parse_fallback_detects_resolution() -> Result<()> {
        let text = "The conclusion is clear.\n\nWe have a resolution.";
        let response = ResponseParser::parse_fallback(text)?;
        assert!(response.thoughts.iter().any(|t| t.kind == "Resolution"));
        Ok(())
    }

    #[test]
    fn test_to_thoughts_empty_response() -> Result<()> {
        let response = ThoughtResponse {
            thoughts: vec![],
            relationships: vec![],
        };
        let thoughts = ResponseParser::to_thoughts(&response)?;
        assert!(thoughts.is_empty());
        Ok(())
    }

    #[test]
    fn test_to_thoughts_multiple() -> Result<()> {
        let response = ThoughtResponse {
            thoughts: vec![
                ThoughtData {
                    kind: "Analysis".to_string(),
                    content: "First".to_string(),
                    confidence: 0.8,
                    reasoning: String::new(),
                },
                ThoughtData {
                    kind: "Synthesis".to_string(),
                    content: "Second".to_string(),
                    confidence: 0.6,
                    reasoning: String::new(),
                },
            ],
            relationships: vec![],
        };
        let thoughts = ResponseParser::to_thoughts(&response)?;
        assert_eq!(thoughts.len(), 2);
        Ok(())
    }

    #[test]
    fn test_thought_data_serialization() {
        let data = ThoughtData {
            kind: "Analysis".to_string(),
            content: "Test".to_string(),
            confidence: 0.7,
            reasoning: "reason".to_string(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: ThoughtData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, "Analysis");
        assert_eq!(back.confidence, 0.7);
    }

    #[test]
    fn test_relationship_data_serialization() {
        let rel = RelationshipData {
            from_idx: 0,
            to_idx: 1,
            edge_kind: "supports".to_string(),
        };
        let json = serde_json::to_string(&rel).unwrap();
        let back: RelationshipData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from_idx, 0);
        assert_eq!(back.to_idx, 1);
    }

    #[test]
    fn test_thought_response_default_confidence() {
        let json = r#"{"thoughts": [{"kind": "Analysis", "content": "test"}]}"#;
        let response: ThoughtResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.thoughts[0].confidence, 0.7); // default
    }
}
