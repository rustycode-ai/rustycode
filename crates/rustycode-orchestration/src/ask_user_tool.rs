//! `ask_user` tool — lets the LLM request clarification or signal it's stuck.
//!
//! During structured thinking, the LLM may hit a dead end: ambiguous
//! requirements, conflicting constraints, or circular reasoning. This tool
//! gives it an explicit escape hatch. The response is returned synchronously
//! as a tool result so the LLM can continue its reasoning.

use serde_json::{json, Value};

/// Schema and guidance for the `ask_user` tool.
pub struct AskUserToolSchema;

impl AskUserToolSchema {
    /// OpenAI-compatible tool schema for `ask_user`.
    pub fn schema() -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "ask_user",
                "description": "Ask the user for clarification or help when you are stuck, unsure, or going in circles. Use this when structured_thinking alone cannot resolve ambiguity.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The specific question you need answered"
                        },
                        "context": {
                            "type": "string",
                            "description": "What you have tried or considered so far, so the user can give targeted help"
                        },
                        "options": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Suggested answers or approaches the user can pick from (optional)"
                        },
                        "urgency": {
                            "type": "string",
                            "enum": ["low", "medium", "high"],
                            "description": "How blocking this is: low=can proceed with guess, medium=prefer answer, high=cannot proceed without answer"
                        }
                    },
                    "required": ["question", "urgency"]
                }
            }
        })
    }

    /// System prompt guidance for using the `ask_user` tool alongside `structured_thinking`.
    pub const fn system_prompt_guidance() -> &'static str {
        r"If you find yourself going in circles during structured thinking, or you lack critical information to proceed, use the ask_user tool to request clarification. Good reasons to ask:
1. Requirements are ambiguous and affect the approach choice
2. You've considered 3+ approaches and can't decide without domain knowledge
3. You've been thinking for many phases without increasing confidence
4. A constraint is contradictory or impossible to satisfy

When asking: be specific about what you need, share what you've already considered, and suggest options if possible. This keeps the user's effort minimal."
    }
}

/// Reasons the system may suggest the LLM ask for help.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum StuckSignal {
    /// Confidence has been flat or declining for N consecutive thoughts.
    ConfidenceStagnation { phases: u32 },
    /// Similar thoughts detected (semantic repetition).
    RepetitiveReasoning { repeated_count: u32 },
    /// Too many thoughts without approaching a conclusion.
    ExcessivePhases { phase_count: u32 },
}

/// Result of checking whether the LLM appears stuck.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StuckCheckResult {
    /// Whether the LLM appears to be stuck.
    pub is_stuck: bool,
    /// Signals that triggered the stuck detection.
    pub signals: Vec<StuckSignal>,
    /// A suggested message to include in the tool response.
    pub suggestion: String,
}

/// Configuration for stuck-detection heuristics.
#[derive(Debug, Clone)]
pub struct StuckDetectionConfig {
    /// Number of consecutive thoughts with flat/declining confidence before flagging.
    pub stagnation_threshold: u32,
    /// Number of phases beyond which we warn about excessive thinking.
    pub max_phases: u32,
    /// Minimum similarity (0.0–1.0) between thoughts to count as repetition.
    pub repetition_similarity: f64,
    /// Number of repeated thoughts before flagging.
    pub repetition_count: u32,
}

impl Default for StuckDetectionConfig {
    fn default() -> Self {
        Self {
            stagnation_threshold: 3,
            max_phases: 8,
            repetition_similarity: 0.7,
            repetition_count: 3,
        }
    }
}

/// Tracks recent thinking history to detect loops.
pub struct StuckDetector {
    config: StuckDetectionConfig,
    /// Confidence values from recent thoughts (most recent last).
    confidence_history: Vec<u32>,
    /// Truncated thought text hashes for repetition detection.
    thought_fingerprints: Vec<u64>,
    /// Total phase count.
    phase_count: u32,
}

impl StuckDetector {
    pub const fn new(config: StuckDetectionConfig) -> Self {
        Self {
            config,
            confidence_history: Vec::new(),
            thought_fingerprints: Vec::new(),
            phase_count: 0,
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(StuckDetectionConfig::default())
    }

    /// Record a new thought and check for stuck signals.
    pub fn record_thought(
        &mut self,
        thought: &str,
        confidence: u32,
        phase: u32,
    ) -> StuckCheckResult {
        self.confidence_history.push(confidence);
        self.phase_count = phase;

        // Fingerprint: lowercase, stripped, truncated to first 80 chars
        let normalized = thought.to_lowercase();
        let normalized = normalized.trim();
        let truncated = if normalized.len() > 80 {
            let end = normalized.floor_char_boundary(80);
            &normalized[..end]
        } else {
            normalized
        };
        let fingerprint = simple_hash(truncated);
        self.thought_fingerprints.push(fingerprint);

        // Keep only recent history to bound memory
        if self.confidence_history.len() > 20 {
            self.confidence_history.remove(0);
        }
        if self.thought_fingerprints.len() > 20 {
            self.thought_fingerprints.remove(0);
        }

        let mut signals = Vec::new();

        // Check confidence stagnation
        if self.detect_stagnation() {
            signals.push(StuckSignal::ConfidenceStagnation {
                phases: self.config.stagnation_threshold,
            });
        }

        // Check repetitive reasoning
        if let Some(count) = self.detect_repetition() {
            signals.push(StuckSignal::RepetitiveReasoning {
                repeated_count: count,
            });
        }

        // Check excessive phases
        if self.phase_count > self.config.max_phases {
            signals.push(StuckSignal::ExcessivePhases {
                phase_count: self.phase_count,
            });
        }

        let is_stuck = !signals.is_empty();
        let suggestion = if is_stuck {
            format!(
                "Thinking loop detected (phase {phase}). Consider using ask_user to get clarification. \
                 Signals: {}",
                signals
                    .iter()
                    .map(|s| match s {
                        StuckSignal::ConfidenceStagnation { phases } =>
                            format!("confidence flat for {phases} phases"),
                        StuckSignal::RepetitiveReasoning { repeated_count } =>
                            format!("{repeated_count} similar thoughts"),
                        StuckSignal::ExcessivePhases { phase_count } =>
                            format!("{phase_count} phases without conclusion"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            String::new()
        };

        StuckCheckResult {
            is_stuck,
            signals,
            suggestion,
        }
    }

    /// Reset state for a new task.
    pub fn reset(&mut self) {
        self.confidence_history.clear();
        self.thought_fingerprints.clear();
        self.phase_count = 0;
    }

    fn detect_stagnation(&self) -> bool {
        let threshold = self.config.stagnation_threshold as usize;
        if self.confidence_history.len() < threshold {
            return false;
        }
        let recent = &self.confidence_history[self.confidence_history.len() - threshold..];
        // Stagnation: all recent confidences are the same, or declining
        let all_same = recent.windows(2).all(|w| w[0] == w[1]);
        let declining = recent.windows(2).all(|w| w[0] >= w[1]);
        all_same || declining
    }

    fn detect_repetition(&self) -> Option<u32> {
        if self.thought_fingerprints.is_empty() {
            return None;
        }
        let last = *self.thought_fingerprints.last()?;
        let count = self
            .thought_fingerprints
            .iter()
            .filter(|&&fp| fp == last)
            .count();
        if count >= self.config.repetition_count as usize {
            Some(count as u32)
        } else {
            None
        }
    }
}

/// Parses the `ask_user` tool call arguments.
pub fn parse_ask_user_args(args: &Value) -> AskUserRequest {
    AskUserRequest {
        question: args["question"].as_str().unwrap_or("").to_string(),
        context: args["context"].as_str().map(String::from),
        options: args["options"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        urgency: match args["urgency"].as_str().unwrap_or("medium") {
            "low" => Urgency::Low,
            "high" => Urgency::High,
            _ => Urgency::Medium,
        },
    }
}

/// Parsed `ask_user` request from the LLM.
#[derive(Debug, Clone)]
pub struct AskUserRequest {
    pub question: String,
    pub context: Option<String>,
    pub options: Vec<String>,
    pub urgency: Urgency,
}

/// Urgency level of the clarification request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Urgency {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for Urgency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

/// Simple FNV-1a hash for thought fingerprinting. No external dependency needed.
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// [`Tool`] trait implementation for `ask_user`.
///
/// When registered in a [`ToolRegistry`](rustycode_tools_api::ToolRegistry),
/// the LLM can call this to request clarification from the user.
/// The tool returns a placeholder response since the actual user interaction
/// is handled by the TUI/headless layer via tool result inspection.
pub struct AskUserTool;

impl rustycode_tools_api::Tool for AskUserTool {
    fn name(&self) -> &'static str {
        "ask_user"
    }

    fn description(&self) -> &'static str {
        "Ask the user for clarification or help when stuck during structured reasoning"
    }

    fn parameters_schema(&self) -> Value {
        AskUserToolSchema::schema()
            .get("function")
            .and_then(|f| f.get("parameters"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}))
    }

    fn execute(
        &self,
        params: Value,
        _ctx: &rustycode_tools_api::ToolContext,
    ) -> anyhow::Result<rustycode_tools_api::ToolOutput> {
        let request = parse_ask_user_args(&params);

        tracing::info!(
            question = %request.question,
            urgency = %request.urgency,
            has_options = !request.options.is_empty(),
            "LLM requested user clarification"
        );

        // The actual user interaction is handled by the TUI/headless layer
        // which inspects tool results. Return a structured response so the
        // LLM knows the question was received.
        let mut response = serde_json::json!({
            "status": "clarification_requested",
            "question": request.question,
            "urgency": request.urgency.to_string(),
        });

        if let Some(ctx) = request.context {
            response["context_provided"] = serde_json::Value::String(ctx);
        }

        if !request.options.is_empty() {
            response["options"] = serde_json::Value::Array(
                request
                    .options
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            );
        }

        response["guidance"] = serde_json::Value::String(
            "Question forwarded to user. Consider proceeding with your best judgment \
             while waiting, or pivot to a different aspect of the problem."
                .into(),
        );

        Ok(rustycode_tools_api::ToolOutput::text(response.to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_valid() {
        let schema = AskUserToolSchema::schema();
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], "ask_user");
        let required = schema["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert!(required.iter().any(|r| r.as_str() == Some("question")));
        assert!(required.iter().any(|r| r.as_str() == Some("urgency")));
    }

    #[test]
    fn guidance_mentions_ask_user() {
        let g = AskUserToolSchema::system_prompt_guidance();
        assert!(g.contains("ask_user"));
        assert!(g.contains("circles"));
    }

    #[test]
    fn stuck_detector_no_signals_initially() {
        let mut det = StuckDetector::with_default_config();
        let result = det.record_thought("first thought", 80, 1);
        assert!(!result.is_stuck);
    }

    #[test]
    fn stuck_detector_confidence_stagnation() {
        let mut det = StuckDetector::with_default_config();
        // 4 thoughts with same confidence
        det.record_thought("analyzing", 60, 1);
        det.record_thought("still analyzing", 60, 2);
        det.record_thought("more analysis", 60, 3);
        let result = det.record_thought("yet more", 60, 4);
        assert!(result.is_stuck);
        assert!(result
            .signals
            .iter()
            .any(|s| matches!(s, StuckSignal::ConfidenceStagnation { .. })));
    }

    #[test]
    fn stuck_detector_declining_confidence() {
        let mut det = StuckDetector::with_default_config();
        det.record_thought("a", 80, 1);
        det.record_thought("b", 70, 2);
        det.record_thought("c", 60, 3);
        let result = det.record_thought("d", 50, 4);
        assert!(result.is_stuck);
    }

    #[test]
    fn stuck_detector_repetition() {
        let mut det = StuckDetector::with_default_config();
        let same = "the same thought repeated";
        det.record_thought(same, 70, 1);
        det.record_thought("different", 70, 2);
        det.record_thought(same, 70, 3);
        let result = det.record_thought(same, 70, 4);
        assert!(result
            .signals
            .iter()
            .any(|s| matches!(s, StuckSignal::RepetitiveReasoning { .. })));
    }

    #[test]
    fn stuck_detector_excessive_phases() {
        let mut det = StuckDetector::with_default_config();
        // Default max_phases is 8
        let result = det.record_thought("deep thought", 90, 9);
        assert!(result
            .signals
            .iter()
            .any(|s| matches!(s, StuckSignal::ExcessivePhases { .. })));
    }

    #[test]
    fn stuck_detector_resets() {
        let mut det = StuckDetector::with_default_config();
        det.record_thought("a", 60, 1);
        det.record_thought("a", 60, 2);
        det.record_thought("a", 60, 3);
        det.reset();
        let result = det.record_thought("fresh", 90, 1);
        assert!(!result.is_stuck);
    }

    #[test]
    fn stuck_detector_no_false_positive_on_improving_confidence() {
        let mut det = StuckDetector::with_default_config();
        det.record_thought("a", 50, 1);
        det.record_thought("b", 60, 2);
        det.record_thought("c", 70, 3);
        let result = det.record_thought("d", 80, 4);
        assert!(!result
            .signals
            .iter()
            .any(|s| matches!(s, StuckSignal::ConfidenceStagnation { .. })));
    }

    #[test]
    fn parse_args_full() {
        let args = serde_json::json!({
            "question": "Which algorithm?",
            "context": "I've tried BFS and DFS",
            "options": ["BFS", "DFS", "Dijkstra"],
            "urgency": "high"
        });
        let req = parse_ask_user_args(&args);
        assert_eq!(req.question, "Which algorithm?");
        assert_eq!(req.context.unwrap(), "I've tried BFS and DFS");
        assert_eq!(req.options.len(), 3);
        assert_eq!(req.urgency, Urgency::High);
    }

    #[test]
    fn parse_args_minimal() {
        let args = serde_json::json!({
            "question": "What should I do?",
            "urgency": "low"
        });
        let req = parse_ask_user_args(&args);
        assert!(req.context.is_none());
        assert!(req.options.is_empty());
        assert_eq!(req.urgency, Urgency::Low);
    }

    #[test]
    fn parse_args_defaults() {
        let args = serde_json::json!({
            "question": "Help"
        });
        let req = parse_ask_user_args(&args);
        assert_eq!(req.urgency, Urgency::Medium);
    }

    #[test]
    fn stuck_check_result_serialization() {
        let result = StuckCheckResult {
            is_stuck: true,
            signals: vec![StuckSignal::ConfidenceStagnation { phases: 3 }],
            suggestion: "stuck".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("ConfidenceStagnation"));
    }

    // AskUserTool Tool trait tests

    use rustycode_tools_api::Tool as _;

    #[test]
    fn ask_user_tool_name() {
        let tool = AskUserTool;
        assert_eq!(tool.name(), "ask_user");
    }

    #[test]
    fn ask_user_tool_schema_matches() {
        let schema = AskUserToolSchema::schema();
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], "ask_user");
    }

    #[test]
    fn ask_user_tool_execute_returns_clarification() {
        let tool = AskUserTool;
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "question": "Should I use BFS or Dijkstra?",
            "context": "Graph has weighted edges",
            "options": ["BFS", "Dijkstra", "A*"],
            "urgency": "high"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("clarification_requested"));
        assert!(result.text.contains("high"));
        assert!(result.text.contains("BFS"));
        assert!(result.text.contains("guidance"));
    }

    #[test]
    fn ask_user_tool_execute_minimal() {
        let tool = AskUserTool;
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "question": "What?",
            "urgency": "low"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("clarification_requested"));
        assert!(result.text.contains("low"));
    }
}
