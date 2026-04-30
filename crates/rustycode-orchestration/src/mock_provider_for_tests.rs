use async_trait::async_trait;
use futures::Stream;
#[cfg(test)]
use rustycode_llm::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderError, StreamChunk,
};
use rustycode_llm::ProviderConfig;
use rustycode_protocol::stream_event::StreamEvent;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A simple mock LLM provider for orchestration tests.
#[derive(Clone, Debug)]
pub struct MockLlmProvider {
    call_count: Arc<AtomicUsize>,
    response_content: String,
    should_error: bool,
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            response_content: "test response".to_string(),
            should_error: false,
        }
    }

    pub fn with_response(mut self, content: &str) -> Self {
        self.response_content = content.to_string();
        self
    }

    pub fn with_failure(mut self, should_error: bool) -> Self {
        self.should_error = should_error;
        self
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LLMProvider for MockLlmProvider {
    fn name(&self) -> &'static str {
        "mock-test"
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["mock-test".to_string()])
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.should_error {
            return Err(ProviderError::Unknown("mock error".to_string()));
        }

        Ok(CompletionResponse {
            content: self.response_content.clone(),
            model: "mock-test".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.should_error {
            return Err(ProviderError::Unknown("mock stream error".to_string()));
        }

        let content = self.response_content.clone();
        let stream = futures::stream::iter(vec![Ok(StreamEvent::TextDelta { content })]);
        Ok(Box::pin(stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        None
    }
}
