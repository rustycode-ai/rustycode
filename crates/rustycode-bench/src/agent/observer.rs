//! Shared bench observer and provider creation utilities.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_agent_runtime::{AgentEvents, AgentResult};
use rustycode_llm::provider::{ChatMessage, MessageContent, MessageRole};
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use serde_json::Value;

/// Observer that collects metrics from the agent loop.
pub struct BenchObserver {
    pub turns: usize,
    pub tool_calls: usize,
    pub errors: usize,
    pub final_text: String,
    first_text_time: Option<Instant>,
    start_time: Instant,
}

impl BenchObserver {
    pub fn new() -> Self {
        Self {
            turns: 0,
            tool_calls: 0,
            errors: 0,
            final_text: String::new(),
            first_text_time: None,
            start_time: Instant::now(),
        }
    }

    pub fn time_to_first_text_ms(&self) -> Option<u64> {
        self.first_text_time
            .map(|t| t.duration_since(self.start_time).as_millis() as u64)
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }
}

#[async_trait]
impl AgentEvents for BenchObserver {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ToolCallStarted { .. } => {
                self.tool_calls += 1;
            }
            StreamEvent::ToolExecCompleted { is_error, .. } => {
                if is_error {
                    self.errors += 1;
                }
            }
            StreamEvent::Done => {
                self.turns += 1;
            }
            StreamEvent::TextDelta { .. } => {
                if self.first_text_time.is_none() {
                    self.first_text_time = Some(Instant::now());
                }
            }
            _ => {}
        }
    }

    async fn on_approval_needed(&mut self, _tool_name: &str, _input: &Value) -> ApprovalDecision {
        ApprovalDecision::AutoApproved
    }

    async fn on_done(&mut self, result: &AgentResult) {
        self.final_text.clone_from(&result.final_text);
    }
}

/// Create an LLM provider from a provider name and model string.
pub fn create_bench_provider(
    provider: &str,
    model: &str,
) -> Result<Arc<dyn rustycode_llm::LLMProvider>> {
    match provider {
        "anthropic" | "claude" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
                .context("ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN not set")?;
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::AnthropicProvider::new(config, model.to_string())?;
            Ok(Arc::new(p))
        }
        "openai" | "gpt" => {
            let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?;
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url: std::env::var("OPENAI_BASE_URL").ok(),
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::OpenAiProvider::new(config, model.to_string())?;
            Ok(Arc::new(p))
        }
        "gemini" => {
            let api_key = std::env::var("GOOGLE_API_KEY").context("GOOGLE_API_KEY not set")?;
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url: None,
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::GeminiProvider::new(config)?;
            Ok(Arc::new(p))
        }
        "zhipu" | "glm" => {
            let api_key = std::env::var("ZHIPU_API_KEY")
                .or_else(|_| std::env::var("OPENAI_API_KEY"))
                .context("ZHIPU_API_KEY or OPENAI_API_KEY not set")?;
            let base_url = std::env::var("ZHIPU_BASE_URL")
                .ok()
                .or_else(|| std::env::var("OPENAI_BASE_URL").ok());
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url,
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::ZhipuProvider::new(config)?;
            Ok(Arc::new(p))
        }
        other => {
            anyhow::bail!(
                "Unsupported provider: '{other}'. Supported: anthropic, openai, gemini, zhipu"
            )
        }
    }
}

/// Build a single user message.
pub fn user_message(content: &str) -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Simple(content.to_string()),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observer_counts_events() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut obs = BenchObserver::new();
            obs.on_event(StreamEvent::ToolCallStarted {
                id: "c1".into(),
                name: "Bash".into(),
            })
            .await;
            obs.on_event(StreamEvent::ToolExecCompleted {
                id: "c1".into(),
                name: "Bash".into(),
                output: "file.txt".into(),
                is_error: false,
            })
            .await;
            obs.on_event(StreamEvent::ToolCallStarted {
                id: "c2".into(),
                name: "Bash".into(),
            })
            .await;
            obs.on_event(StreamEvent::ToolExecCompleted {
                id: "c2".into(),
                name: "Bash".into(),
                output: "not found".into(),
                is_error: true,
            })
            .await;
            obs.on_event(StreamEvent::Done).await;
            assert_eq!(obs.tool_calls, 2);
            assert_eq!(obs.errors, 1);
            assert_eq!(obs.turns, 1);
        });
    }

    #[test]
    fn observer_tracks_ttft() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut obs = BenchObserver::new();
            assert!(obs.time_to_first_text_ms().is_none());

            obs.on_event(StreamEvent::TextDelta {
                content: "hello".into(),
            })
            .await;
            assert!(obs.time_to_first_text_ms().is_some());
        });
    }
}
