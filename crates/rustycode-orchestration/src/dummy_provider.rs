use async_trait::async_trait;
use futures::Stream;
use rustycode_llm::provider::{CompletionRequest, CompletionResponse, LLMProvider, ProviderError};
use std::pin::Pin;

#[derive(Clone)]
pub struct DummyLlmProvider;

#[async_trait]
impl LLMProvider for DummyLlmProvider {
    fn name(&self) -> &'static str {
        "dummy"
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn list_models(&self) -> std::result::Result<Vec<String>, ProviderError> {
        Ok(vec!["dummy".to_string()])
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        Ok(CompletionResponse {
            content: "dummy".into(),
            model: "dummy".into(),
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
    ) -> std::result::Result<
        Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
        ProviderError,
    > {
        Err(ProviderError::Unknown("streaming not supported".into()))
    }

    fn config(&self) -> Option<&rustycode_llm::ProviderConfig> {
        None
    }
}
