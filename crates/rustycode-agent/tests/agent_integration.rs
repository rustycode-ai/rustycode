#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::significant_drop_tightening
)]

use futures::stream::Stream;
use rustycode_agent::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_llm::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderError, Usage,
};
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolRegistry};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

enum MockResponse {
    Stream(Vec<StreamEvent>),
    Completion(CompletionResponse),
}

struct MockProvider {
    responses: Arc<Mutex<Vec<MockResponse>>>,
    supports_streaming: bool,
}

#[async_trait::async_trait]
impl LLMProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn supports_streaming(&self) -> bool {
        self.supports_streaming
    }
    async fn is_available(&self) -> bool {
        true
    }
    async fn list_models(
        &self,
    ) -> anyhow::Result<Vec<String>, rustycode_llm::provider::ProviderError> {
        Ok(vec!["test-model".into()])
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> anyhow::Result<
        rustycode_llm::provider::CompletionResponse,
        rustycode_llm::provider::ProviderError,
    > {
        let responses = self.responses.clone();
        let mut resps = responses.lock().await;
        match resps.first() {
            Some(MockResponse::Completion(_)) => match resps.remove(0) {
                MockResponse::Completion(response) => Ok(response),
                MockResponse::Stream(_) => unreachable!("peeked completion response"),
            },
            Some(MockResponse::Stream(_)) => Err(ProviderError::Configuration(
                "streaming-only response received on complete()".into(),
            )),
            None => Err(ProviderError::Configuration(
                "no mock responses left for complete()".into(),
            )),
        }
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> anyhow::Result<
        Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
        rustycode_llm::provider::ProviderError,
    > {
        let responses = self.responses.clone();
        let mut resps = responses.lock().await;
        match resps.first() {
            Some(MockResponse::Stream(_)) => match resps.remove(0) {
                MockResponse::Stream(events) => {
                    let stream = futures::stream::iter(events.into_iter().map(Ok));
                    Ok(Box::pin(stream))
                }
                MockResponse::Completion(_) => unreachable!("peeked stream response"),
            },
            Some(MockResponse::Completion(_)) => Err(ProviderError::Configuration(
                "completion-only response received on complete_stream()".into(),
            )),
            None => Err(ProviderError::Configuration(
                "no mock responses left for complete_stream()".into(),
            )),
        }
    }
}

struct TestEvents {
    pub tool_called: bool,
    pub turns: usize,
    pub done: Option<AgentResult>,
}

impl TestEvents {
    const fn new() -> Self {
        Self {
            tool_called: false,
            turns: 0,
            done: None,
        }
    }
}

#[async_trait::async_trait]
impl AgentEvents for TestEvents {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ToolCallStarted { .. } => {
                self.tool_called = true;
            }
            StreamEvent::Done => {
                self.turns += 1;
            }
            _ => {}
        }
    }
    async fn on_done(&mut self, result: &AgentResult) {
        self.done = Some(AgentResult {
            final_text: result.final_text.clone(),
            messages: result.messages.clone(),
            stopped_reason: result.stopped_reason.clone(),
            total_input_tokens: result.total_input_tokens,
            total_output_tokens: result.total_output_tokens,
            total_cache_read_tokens: result.total_cache_read_tokens,
            total_cache_creation_tokens: result.total_cache_creation_tokens,
        });
    }
}

struct EchoTool;
impl Tool for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn description(&self) -> &'static str {
        "echoes input"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput::text(input.to_string()))
    }
}

fn completion_response(
    content: impl Into<String>,
    stop_reason: Option<&str>,
) -> CompletionResponse {
    CompletionResponse {
        content: content.into(),
        model: "test-model".into(),
        usage: Some(Usage {
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: None,
        }),
        stop_reason: stop_reason.map(str::to_owned),
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    }
}

#[tokio::test]
async fn test_agent_loop_tool_execution_streaming() {
    let responses = vec![
        MockResponse::Stream(vec![
            StreamEvent::ToolCallStarted {
                id: "t1".into(),
                name: "echo".into(),
            },
            StreamEvent::TurnCompleted {
                stop_reason: "tool_use".into(),
            },
        ]),
        MockResponse::Stream(vec![
            StreamEvent::TextDelta {
                content: "done".into(),
            },
            StreamEvent::TurnCompleted {
                stop_reason: "end_turn".into(),
            },
        ]),
    ];
    let provider = MockProvider {
        responses: Arc::new(Mutex::new(responses)),
        supports_streaming: true,
    };
    let mut registry = ToolRegistry::new();
    registry.register(EchoTool);

    let mut session = AgentSession::new(AgentConfig::default(), std::env::current_dir().unwrap());
    let mut events = TestEvents::new();

    let result = session
        .run(
            &provider,
            "test-model",
            "system",
            vec![],
            &[],
            &registry,
            &mut events,
        )
        .await
        .expect("agent run failed");

    assert!(events.tool_called);
    assert_eq!(result.final_text, "done");
}

#[tokio::test]
async fn test_agent_loop_tool_execution_non_streaming_fallback() {
    let responses = vec![
        MockResponse::Completion(completion_response(
            "```tool\n[{\"id\":\"t1\",\"type\":\"function\",\"function\":{\"name\":\"echo\",\"arguments\":\"{\\\"msg\\\":\\\"hello\\\"}\"}}]\n```",
            Some("tool_use"),
        )),
        MockResponse::Completion(completion_response("done", Some("end_turn"))),
    ];
    let provider = MockProvider {
        responses: Arc::new(Mutex::new(responses)),
        supports_streaming: false,
    };
    let mut registry = ToolRegistry::new();
    registry.register(EchoTool);

    let mut session = AgentSession::new(AgentConfig::default(), std::env::current_dir().unwrap());
    let mut events = TestEvents::new();

    let result = session
        .run(
            &provider,
            "test-model",
            "system",
            vec![],
            &[],
            &registry,
            &mut events,
        )
        .await
        .expect("agent run failed");

    assert!(events.tool_called);
    assert_eq!(result.final_text, "done");
}
