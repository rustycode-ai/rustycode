use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rustycode_protocol::stream_event::StreamEvent;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A simple in-memory mock LLM provider for tests.
///
/// Construct with a sequence of responses the provider will return on each
/// `complete` call. Responses are consumed in FIFO order; when empty the last
/// response will be repeated.
#[derive(Clone, Debug)]
pub struct MockProvider {
    responses: Arc<Mutex<VecDeque<Result<CompletionResponse, ProviderError>>>>,
    config: ProviderConfig,
    scripted_stream: Option<Arc<ScriptedStream>>,
}

#[derive(Clone, Debug)]
struct ScriptedStream {
    items: Vec<std::result::Result<String, String>>,
    delay: Duration,
    error_before_stream: Option<String>,
}

impl MockProvider {
    /// Create a new mock provider with the given responses. If `config` is
    /// omitted `ProviderConfig::default()` is used.
    pub fn new(
        responses: Vec<Result<CompletionResponse, ProviderError>>,
        config: Option<ProviderConfig>,
    ) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            config: config.unwrap_or_default(),
            scripted_stream: None,
        }
    }

    /// Convenience constructor for a single successful text response.
    pub fn from_text(content: impl Into<String>) -> Self {
        let resp = CompletionResponse {
            content: content.into(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };
        Self::new(vec![Ok(resp)], Some(ProviderConfig::default()))
    }

    pub fn from_env(config: ProviderConfig) -> Self {
        let model = std::env::var("RUSTYCODE_MOCK_MODEL")
            .ok()
            .unwrap_or_else(|| "mock".to_string());

        let delay = std::env::var("RUSTYCODE_MOCK_STREAM_DELAY_MS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_default();

        let base_items = std::env::var("RUSTYCODE_MOCK_STREAM_CHUNKS")
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .map(|chunks| {
                chunks
                    .into_iter()
                    .map(Result::<String, String>::Ok)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![Ok(
                    std::env::var("RUSTYCODE_MOCK_RESPONSE").unwrap_or_default()
                )]
            });

        let error_before_stream = std::env::var("RUSTYCODE_MOCK_ERROR_MESSAGE").ok();
        let mut scripted_items = base_items.clone();
        if let Ok(message) = std::env::var("RUSTYCODE_MOCK_STREAM_ERROR") {
            let message = if message.is_empty() {
                "scripted mock stream error".to_string()
            } else {
                message
            };
            scripted_items.push(Err(message));
        }

        let joined = base_items
            .iter()
            .filter_map(|item| item.as_ref().ok().cloned())
            .collect::<String>();

        let responses = if let Some(message) = &error_before_stream {
            vec![Err(ProviderError::Unknown(message.clone()))]
        } else {
            vec![Ok(CompletionResponse {
                content: joined,
                model,
                usage: None,
                stop_reason: None,
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            })]
        };

        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            config,
            scripted_stream: Some(Arc::new(ScriptedStream {
                items: scripted_items,
                delay,
                error_before_stream,
            })),
        }
    }
}

#[async_trait]
impl LLMProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["mock".to_string()])
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        if let Some(script) = &self.scripted_stream {
            if let Some(message) = &script.error_before_stream {
                return Err(ProviderError::Unknown(message.clone()));
            }
        }

        let mut guard = self
            .responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.is_empty() {
            // No responses left; return a default empty response
            Ok(CompletionResponse {
                content: String::new(),
                model: "mock".to_string(),
                usage: None,
                stop_reason: None,
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            })
        } else if guard.len() == 1 {
            // Return the single response but keep it around for repeated calls
            match guard.front().unwrap() {
                Ok(resp) => Ok(resp.clone()),
                Err(err) => Err(err.clone()),
            }
        } else {
            guard.pop_front().unwrap()
        }
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        if let Some(script) = &self.scripted_stream {
            if let Some(message) = &script.error_before_stream {
                return Err(ProviderError::Unknown(message.clone()));
            }

            let items = script.items.clone();
            let delay = script.delay;
            if delay.is_zero() {
                let stream = futures::stream::iter(items.into_iter().map(|item| match item {
                    Ok(text) => Ok(StreamEvent::TextDelta { content: text }),
                    Err(message) => Err(ProviderError::Unknown(message)),
                }));
                return Ok(Box::pin(stream));
            }

            let interval = tokio::time::interval(delay);
            let stream = tokio_stream::wrappers::IntervalStream::new(interval)
                .zip(futures::stream::iter(items))
                .map(|(_, item)| match item {
                    Ok(text) => Ok(StreamEvent::TextDelta { content: text }),
                    Err(message) => Err(ProviderError::Unknown(message)),
                });
            return Ok(Box::pin(stream));
        }

        // Build a simple stream of strings from the queued responses. If the
        // queue contains errors, yield the error; otherwise yield the
        // completion content. We clone guarded contents to avoid holding the
        // mutex across the stream's lifetime.
        // Clone current responses to avoid holding the mutex during stream
        // iteration. We turn each CompletionResponse into a single chunk. If
        // there are multiple queued responses they will be yielded in order.
        // Drain the currently queued responses into an owned vector so the
        // returned stream does not hold the mutex while being consumed. This
        // makes streaming deterministic for test scenarios: the items that
        // existed at stream creation time are yielded and the provider's
        // internal queue is advanced.
        let items: Vec<StreamChunk> = {
            let mut guard = self
                .responses
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.is_empty() {
                vec![Ok(StreamEvent::TextDelta {
                    content: String::new(),
                })]
            } else {
                guard
                    .drain(..)
                    .map(|r| match r {
                        Ok(resp) => Ok(StreamEvent::TextDelta {
                            content: resp.content,
                        }),
                        Err(e) => Err(e),
                    })
                    .collect()
            }
        };

        let stream = futures::stream::iter(items);
        Ok(Box::pin(stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ChatMessage;
    use futures::stream::StreamExt;

    fn test_request() -> CompletionRequest {
        CompletionRequest::new(
            "mock".to_string(),
            vec![ChatMessage::user("test".to_string())],
        )
    }

    #[test]
    fn test_complete_stream_drains_queue() {
        let r1 = CompletionResponse {
            content: "first".into(),
            model: "m".into(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };
        let r2 = CompletionResponse {
            content: "second".into(),
            model: "m".into(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        let provider = MockProvider::new(vec![Ok(r1.clone()), Ok(r2.clone())], None);

        // Collect the stream contents synchronously
        let items: Vec<StreamChunk> = futures::executor::block_on(async {
            let s = LLMProvider::complete_stream(&provider, test_request())
                .await
                .unwrap();
            s.collect::<Vec<_>>().await
        });

        // Stream should yield the two contents in order
        assert_eq!(items.len(), 2);
        // Extract text from StreamEvent::TextDelta variants
        match items[0].as_ref().unwrap() {
            StreamEvent::TextDelta { content } => assert_eq!(content, "first"),
            _ => panic!("Expected TextDelta variant"),
        }
        match items[1].as_ref().unwrap() {
            StreamEvent::TextDelta { content } => assert_eq!(content, "second"),
            _ => panic!("Expected TextDelta variant"),
        }

        // Provider's internal queue should have been drained by complete_stream
        let guard = provider.responses.lock().unwrap();
        assert!(guard.is_empty());
        drop(guard);
    }

    #[test]
    fn test_complete_stream_handles_errors_and_empty() {
        // Error response yields an Err in the stream
        let provider_err =
            MockProvider::new(vec![Err(ProviderError::Unknown("boom".to_string()))], None);
        let items_err: Vec<StreamChunk> = futures::executor::block_on(async {
            let s = LLMProvider::complete_stream(&provider_err, test_request())
                .await
                .unwrap();
            s.collect::<Vec<_>>().await
        });
        assert_eq!(items_err.len(), 1);
        assert!(items_err[0].is_err());

        // Empty initial queue yields a single empty string item and leaves queue empty
        let provider_empty = MockProvider::new(vec![], None);
        let items_empty: Vec<StreamChunk> = futures::executor::block_on(async {
            let s = LLMProvider::complete_stream(&provider_empty, test_request())
                .await
                .unwrap();
            s.collect::<Vec<_>>().await
        });
        assert_eq!(items_empty.len(), 1);
        // Extract text from TextDelta variant
        match items_empty[0].as_ref().unwrap() {
            StreamEvent::TextDelta { content } => assert_eq!(content, ""),
            _ => panic!("Expected TextDelta variant"),
        }
        let guard = provider_empty.responses.lock().unwrap();
        assert!(guard.is_empty());
    }

    #[test]
    fn test_complete_repeats_single_response() {
        let r = CompletionResponse {
            content: "solo".into(),
            model: "m".into(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };
        let provider = MockProvider::new(vec![Ok(r.clone())], None);

        // Calling complete twice should return the same response without draining
        let first =
            futures::executor::block_on(LLMProvider::complete(&provider, test_request())).unwrap();
        let second =
            futures::executor::block_on(LLMProvider::complete(&provider, test_request())).unwrap();
        assert_eq!(first.content, "solo");
        assert_eq!(second.content, "solo");

        // Internal queue should still contain the single response
        let guard = provider.responses.lock().unwrap();
        assert_eq!(guard.len(), 1);
    }

    #[tokio::test]
    async fn test_stream_consumption_does_not_block_other_ops() {
        let r1 = CompletionResponse {
            content: "a".into(),
            model: "m".into(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };
        let r2 = CompletionResponse {
            content: "b".into(),
            model: "m".into(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        let provider = MockProvider::new(vec![Ok(r1.clone()), Ok(r2.clone())], None);

        // Create the stream which drains the current queue
        let mut stream = LLMProvider::complete_stream(&provider, test_request())
            .await
            .unwrap();

        // Spawn a producer that will push another response while the stream is consumed
        let provider_clone = provider.clone();
        let producer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let mut guard = provider_clone.responses.lock().unwrap();
            guard.push_back(Ok(CompletionResponse {
                content: "pushed".into(),
                model: "m".into(),
                usage: None,
                stop_reason: None,
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            }));
        });

        // Consume the stream slowly in this task
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            // simulate slow processing
            tokio::time::sleep(Duration::from_millis(10)).await;
            out.push(item);
        }

        // Ensure producer finished
        producer.await.unwrap();

        // Consumer should have received the original two items
        assert_eq!(out.len(), 2);
        // Extract text from TextDelta variants
        match out[0].as_ref().unwrap() {
            StreamEvent::TextDelta { content } => assert_eq!(content, "a"),
            _ => panic!("Expected TextDelta variant"),
        }
        match out[1].as_ref().unwrap() {
            StreamEvent::TextDelta { content } => assert_eq!(content, "b"),
            _ => panic!("Expected TextDelta variant"),
        }

        // The pushed item should remain in the provider queue
        let guard = provider.responses.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard.front().unwrap().as_ref().unwrap().content, "pushed");
    }
}
