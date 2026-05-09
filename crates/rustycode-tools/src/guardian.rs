//! Guardian LLM-based auto-approval for borderline Write operations.
//!
//! Complements the heuristic `SmartApprove` classifier. A secondary lightweight
//! LLM call assesses whether a Write operation can be auto-approved. Only
//! consulted for operations classified as `Write`; read-only and destructive
//! operations skip it entirely. Fail-closed: all error conditions defer to user.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Trait for the LLM backend used by the guardian.
///
/// Implementations wrap any provider (`Anthropic`, `OpenAI`, local model).
#[async_trait]
pub trait GuardianLlm: Send + Sync {
    /// Complete a single prompt and return the raw text response.
    async fn complete(&self, prompt: &str) -> anyhow::Result<String>;
}

/// Decision returned by the guardian assessment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardianDecision {
    Allow { reason: String },
    Deny { reason: String },
    DeferToUser { reason: String },
}

/// Flat JSON representation matching the LLM prompt format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlatDecision {
    decision: String,
    reason: String,
}

impl From<FlatDecision> for GuardianDecision {
    fn from(flat: FlatDecision) -> Self {
        match flat.decision.as_str() {
            "allow" => Self::Allow {
                reason: flat.reason,
            },
            "deny" => Self::Deny {
                reason: flat.reason,
            },
            _ => Self::DeferToUser {
                reason: flat.reason,
            },
        }
    }
}
/// Configuration for the guardian assessor.
#[derive(Debug, Clone)]
pub struct GuardianConfig {
    /// Whether the guardian is active. Default: `false` (opt-in).
    pub enabled: bool,
    /// Maximum time to wait for the LLM response. Default: 3 seconds.
    pub timeout: Duration,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout: Duration::from_secs(3),
        }
    }
}

/// LLM-based assessor for borderline Write tool operations.
pub struct GuardianAssessor {
    llm: Box<dyn GuardianLlm>,
    config: GuardianConfig,
}

impl GuardianAssessor {
    pub fn new(llm: Box<dyn GuardianLlm>, config: GuardianConfig) -> Self {
        Self { llm, config }
    }

    /// Assess whether a Write tool operation should be auto-approved.
    ///
    /// Returns `DeferToUser` on timeout, malformed JSON, or LLM failure.
    pub async fn assess(
        &self,
        tool_name: &str,
        args: &str,
        transcript_summary: &str,
    ) -> GuardianDecision {
        let prompt = build_prompt(tool_name, args, transcript_summary);
        let result = tokio::time::timeout(self.config.timeout, self.llm.complete(&prompt)).await;
        match result {
            Ok(Ok(response)) => parse_decision(&response),
            Ok(Err(_)) => defer("LLM call failed"),
            Err(_) => defer("guardian timed out"),
        }
    }

    /// Whether the guardian is enabled in its configuration.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Build the structured prompt asking for strict JSON output.
fn build_prompt(tool_name: &str, args: &str, transcript_summary: &str) -> String {
    format!(
        "You are a security guardian for a code assistant. \
         Decide whether this tool operation is safe to auto-approve.\n\n\
         Tool: {tool_name}\nArguments: {args}\n\
         Transcript summary: {transcript_summary}\n\n\
         Respond with ONLY a JSON object (no markdown):\n\
         {{\"decision\": \"allow\"|\"deny\"|\"defer\", \"reason\": \"brief explanation\"}}\n\n\
         \"allow\": clearly safe (new non-sensitive file, test file).\n\
         \"deny\": clearly dangerous (credentials, .env, outside project).\n\
         \"defer\": unsure — let the user decide."
    )
}

/// Parse the LLM response into a `GuardianDecision`. Returns `DeferToUser` on any failure.
fn parse_decision(response: &str) -> GuardianDecision {
    let json_text = extract_json_object(response.trim());
    match serde_json::from_str::<FlatDecision>(json_text) {
        Ok(flat) => flat.into(),
        Err(e) => defer(&format!("failed to parse guardian response: {e}")),
    }
}

/// Extract the first balanced `{...}` JSON object, stripping markdown fences.
fn extract_json_object(input: &str) -> &str {
    let text = input
        .strip_prefix("```json")
        .or_else(|| input.strip_prefix("```"))
        .unwrap_or(input)
        .trim_end_matches("```")
        .trim();
    let start = match text.find('{') {
        Some(i) => i,
        None => return text,
    };
    let mut depth = 0i32;
    for (i, ch) in text.bytes().enumerate().skip(start) {
        match ch {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[start..=i];
                }
            }
            _ => {}
        }
    }
    text
}

fn defer(reason: &str) -> GuardianDecision {
    GuardianDecision::DeferToUser {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn decision_serde_roundtrip() {
        let pairs: Vec<(&str, GuardianDecision)> = vec![
            (
                "allow",
                GuardianDecision::Allow {
                    reason: "safe".into(),
                },
            ),
            (
                "deny",
                GuardianDecision::Deny {
                    reason: "danger".into(),
                },
            ),
            (
                "defer",
                GuardianDecision::DeferToUser {
                    reason: "unsure".into(),
                },
            ),
        ];
        for (tag, original) in pairs {
            let json = format!(
                r#"{{"decision":"{tag}","reason":"{}"}}"#,
                match &original {
                    GuardianDecision::Allow { reason }
                    | GuardianDecision::Deny { reason }
                    | GuardianDecision::DeferToUser { reason } => reason,
                }
            );
            let parsed: GuardianDecision = serde_json::from_str::<FlatDecision>(&json)
                .expect("parse")
                .into();
            assert_eq!(original, parsed, "roundtrip failed for {tag}");
        }
    }

    #[test]
    fn parse_allow_deny_defer() {
        assert_eq!(
            parse_decision(r#"{"decision": "allow", "reason": "safe"}"#),
            GuardianDecision::Allow {
                reason: "safe".into()
            },
        );
        assert_eq!(
            parse_decision(r#"{"decision": "deny", "reason": "secrets"}"#),
            GuardianDecision::Deny {
                reason: "secrets".into()
            },
        );
        assert_eq!(
            parse_decision(r#"{"decision": "defer", "reason": "unclear"}"#),
            GuardianDecision::DeferToUser {
                reason: "unclear".into()
            },
        );
    }

    #[test]
    fn parse_json_wrapped_in_markdown() {
        let d = parse_decision("```json\n{\"decision\": \"allow\", \"reason\": \"ok\"}\n```");
        assert_eq!(
            d,
            GuardianDecision::Allow {
                reason: "ok".into()
            }
        );
    }

    #[test]
    fn malformed_json_produces_defer() {
        for input in [
            "not JSON",
            r#"{"decision": "allow"#,
            r#"{"decision":"maybe","reason":"x"}"#,
        ] {
            let d = parse_decision(input);
            assert!(
                matches!(d, GuardianDecision::DeferToUser { .. }),
                "{input} -> {d:?}"
            );
        }
    }

    // -- Mock LLMs --

    struct HangingLlm;
    #[async_trait]
    impl GuardianLlm for HangingLlm {
        async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
            tokio::time::sleep(Duration::from_secs(300)).await;
            Ok("unreachable".into())
        }
    }

    struct FailingLlm;
    #[async_trait]
    impl GuardianLlm for FailingLlm {
        async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
            anyhow::bail!("provider unavailable")
        }
    }

    struct MockLlm {
        response: Mutex<String>,
    }
    impl MockLlm {
        fn new(response: &str) -> Self {
            Self {
                response: Mutex::new(response.into()),
            }
        }
    }
    #[async_trait]
    impl GuardianLlm for MockLlm {
        async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
            Ok(self.response.lock().expect("lock").clone())
        }
    }

    fn test_config() -> GuardianConfig {
        GuardianConfig {
            enabled: true,
            timeout: Duration::from_secs(3),
        }
    }

    #[tokio::test]
    async fn timeout_produces_defer_to_user() {
        let a = GuardianAssessor::new(
            Box::new(HangingLlm),
            GuardianConfig {
                enabled: true,
                timeout: Duration::from_millis(50),
            },
        );
        let d = a.assess("Write", "{}", "").await;
        assert!(
            matches!(d, GuardianDecision::DeferToUser { ref reason } if reason.contains("timed out")),
            "got {d:?}"
        );
    }

    #[tokio::test]
    async fn llm_error_produces_defer_to_user() {
        let a = GuardianAssessor::new(Box::new(FailingLlm), test_config());
        let d = a.assess("Write", "{}", "").await;
        assert!(
            matches!(d, GuardianDecision::DeferToUser { ref reason } if reason.contains("LLM call failed")),
            "got {d:?}"
        );
    }

    #[tokio::test]
    async fn allow_decision_from_llm() {
        let a = GuardianAssessor::new(
            Box::new(MockLlm::new(
                r#"{"decision": "allow", "reason": "new test file"}"#,
            )),
            test_config(),
        );
        assert_eq!(
            a.assess("Write", r#"{"path":"tests/foo.rs"}"#, "").await,
            GuardianDecision::Allow {
                reason: "new test file".into()
            },
        );
    }

    #[tokio::test]
    async fn deny_decision_from_llm() {
        let a = GuardianAssessor::new(
            Box::new(MockLlm::new(
                r#"{"decision": "deny", "reason": "writes to .env"}"#,
            )),
            test_config(),
        );
        assert_eq!(
            a.assess("Write", r#"{"path":".env"}"#, "").await,
            GuardianDecision::Deny {
                reason: "writes to .env".into()
            },
        );
    }

    #[test]
    fn config_default_and_enabled() {
        let c = GuardianConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.timeout, Duration::from_secs(3));
        let a = GuardianAssessor::new(
            Box::new(MockLlm::new("{}")),
            GuardianConfig {
                enabled: true,
                timeout: Duration::from_secs(1),
            },
        );
        assert!(a.is_enabled());
    }

    #[test]
    fn build_prompt_contains_tool_name() {
        let p = build_prompt("Write", r#"{"path":"a.rs"}"#, "create");
        assert!(p.contains("Write") && p.contains("JSON"));
    }
}
