//! Replay Provider for Deterministic Testing
//!
//! A provider wrapper that records LLM interactions to JSON files and replays
//! them later. This enables fast, deterministic, offline tests without API calls.
//!
//! Inspired by goose's `TestProvider` in `providers/testprovider.rs`.
//!
//! # Modes
//!
//! - **Recording**: Wraps a real provider, forwards requests, and saves
//!   request/response pairs keyed by SHA-256 hash of the input messages.
//! - **Replaying**: Loads a recording file and returns saved responses for
//!   matching inputs. Returns an error if no recorded response matches.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_llm::replay_provider::ReplayProvider;
//!
//! // Record mode: wraps a real provider
//! let real_provider = create_provider("anthropic", "claude-sonnet-4-20250514")?;
//! let recorder = ReplayProvider::new_recording(real_provider, "recordings/test.json");
//! let response = LLMProvider::complete(&recorder, request).await?;
//! recorder.finish_recording()?;
//!
//! // Replay mode: plays back recorded interactions
//! let replayer = ReplayProvider::new_replaying("recordings/test.json")?;
//! let response = LLMProvider::complete(&replayer, request).await?; // No API call!
//! ```

use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

// ── Serialization Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecordedInput {
    model: String,
    /// Hash of messages for matching (also the HashMap key)
    #[allow(dead_code)] // Kept for future use
    messages_hash: String,
    system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecordedOutput {
    content: String,
    model: String,
    usage: Option<Usage>,
    stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    input: RecordedInput,
    output: RecordedOutput,
}

// ── ReplayProvider ──────────────────────────────────────────────────────────

/// A provider that can record and replay LLM interactions.
///
/// In **recording** mode, it wraps a real provider and saves all interactions
/// to a JSON file. In **replaying** mode, it loads that file and returns saved
/// responses for matching inputs, making tests deterministic and fast.
pub struct ReplayProvider {
    /// The real provider (only in recording mode)
    inner: Option<Arc<dyn LLMProvider>>,
    /// Recorded interactions keyed by input hash
    records: Arc<Mutex<HashMap<String, Record>>>,
    /// File path for saving/loading recordings
    file_path: PathBuf,
}

impl ReplayProvider {
    /// Create a new recording provider that wraps a real provider.
    ///
    /// All interactions are recorded in memory. Call `finish_recording()` to
    /// persist them to the file.
    pub fn new_recording(inner: Arc<dyn LLMProvider>, file_path: impl Into<PathBuf>) -> Self {
        Self {
            inner: Some(inner),
            records: Arc::new(Mutex::new(HashMap::new())),
            file_path: file_path.into(),
        }
    }

    /// Create a new replaying provider that loads from a recording file.
    ///
    /// Returns an error if the file doesn't exist or is invalid JSON.
    pub fn new_replaying(file_path: impl Into<PathBuf>) -> Result<Self, ProviderError> {
        let file_path = file_path.into();
        let records = Self::load_records(&file_path)?;
        Ok(Self {
            inner: None,
            records: Arc::new(Mutex::new(records)),
            file_path,
        })
    }

    /// Save all recorded interactions to the recording file.
    ///
    /// Only meaningful in recording mode. Returns Ok(()) if not recording.
    pub fn finish_recording(&self) -> Result<(), ProviderError> {
        if self.inner.is_none() {
            return Ok(());
        }
        self.save_records()
    }

    /// Get the number of recorded interactions.
    pub fn record_count(&self) -> usize {
        self.records.lock().map(|r| r.len()).unwrap_or(0)
    }

    /// Check if this provider is in recording mode.
    pub fn is_recording(&self) -> bool {
        self.inner.is_some()
    }

    // ── Hashing ─────────────────────────────────────────────────────────

    /// Hash request messages for deterministic matching.
    ///
    /// Uses SHA-256 of the serialized messages for consistent matching
    /// regardless of field ordering.
    fn hash_request(request: &CompletionRequest) -> String {
        // Serialize only the messages for matching — model/temperature/etc
        // shouldn't affect replay matching
        let serialized = serde_json::to_string(&request.messages).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        hasher
            .finalize()
            .iter()
            .fold(String::with_capacity(64), |mut acc, byte| {
                use std::fmt::Write;
                let _ = write!(acc, "{byte:02x}");
                acc
            })
    }

    // ── Persistence ─────────────────────────────────────────────────────

    fn load_records(file_path: &Path) -> Result<HashMap<String, Record>, ProviderError> {
        if !file_path.exists() {
            return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            ProviderError::Configuration(format!(
                "Failed to read recording file {}: {}",
                file_path.display(),
                e
            ))
        })?;
        serde_json::from_str(&content).map_err(|e| {
            ProviderError::Configuration(format!(
                "Invalid recording file {}: {}",
                file_path.display(),
                e
            ))
        })
    }

    fn save_records(&self) -> Result<(), ProviderError> {
        let records = self.records.lock().map_err(|e| {
            ProviderError::Unknown(format!("Failed to acquire records lock: {}", e))
        })?;
        let content = serde_json::to_string_pretty(&*records)
            .map_err(|e| ProviderError::Unknown(format!("Failed to serialize records: {}", e)))?;
        // Create parent directories if needed
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ProviderError::Unknown(format!("Failed to create recording directory: {}", e))
            })?;
        }
        std::fs::write(&self.file_path, content).map_err(|e| {
            ProviderError::Unknown(format!(
                "Failed to write recording file {}: {}",
                self.file_path.display(),
                e
            ))
        })
    }
}

#[async_trait]
impl LLMProvider for ReplayProvider {
    fn name(&self) -> &'static str {
        "replay"
    }

    async fn is_available(&self) -> bool {
        if let Some(inner) = &self.inner {
            inner.is_available().await
        } else {
            // Replaying is always available
            true
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        if let Some(inner) = &self.inner {
            inner.list_models().await
        } else {
            Ok(vec!["replay-model".to_string()])
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let hash = Self::hash_request(&request);

        if let Some(inner) = &self.inner {
            // Recording mode: forward to real provider and save
            let response = inner.complete(request.clone()).await?;

            let record = Record {
                input: RecordedInput {
                    model: request.model.clone(),
                    messages_hash: hash.clone(),
                    system_prompt: request.system_prompt.clone(),
                },
                output: RecordedOutput {
                    content: response.content.clone(),
                    model: response.model.clone(),
                    usage: response.usage.clone(),
                    stop_reason: response.stop_reason.clone(),
                },
            };

            {
                let mut records = self.records.lock().map_err(|_| {
                    ProviderError::Unknown("Failed to acquire records lock".to_string())
                })?;
                records.insert(hash, record);
            }

            Ok(response)
        } else {
            // Replay mode: look up recorded response
            let records = self.records.lock().map_err(|_| {
                ProviderError::Unknown("Failed to acquire records lock".to_string())
            })?;
            match records.get(&hash) {
                Some(record) => Ok(CompletionResponse {
                    content: record.output.content.clone(),
                    model: record.output.model.clone(),
                    usage: record.output.usage.clone(),
                    stop_reason: record.output.stop_reason.clone(),
                    citations: None,
                    thinking_blocks: None,
                    structured_output: None,
                }),
                None => Err(ProviderError::Unknown(format!(
                    "No recorded response found for input hash: {}",
                    hash
                ))),
            }
        }
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let hash = Self::hash_request(&request);

        if let Some(inner) = &self.inner {
            // Recording mode: collect the stream, save the full response, then replay as stream
            let mut stream = inner.complete_stream(request.clone()).await?;
            let mut collected_text = String::new();
            let mut final_usage: Option<Usage> = None;
            let mut final_stop_reason: Option<String> = None;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta { content }) => {
                        collected_text.push_str(&content);
                    }
                    Ok(rustycode_protocol::stream_event::StreamEvent::TokenUsage {
                        input_tokens,
                        output_tokens,
                    }) => {
                        final_usage = Some(Usage {
                            input_tokens: input_tokens as u32,
                            output_tokens: output_tokens as u32,
                            total_tokens: (input_tokens + output_tokens) as u32,
                            cache_read_input_tokens: 0,
                            cache_creation_input_tokens: 0,
                            reasoning_tokens: Some(0),
                        });
                    }
                    Ok(rustycode_protocol::stream_event::StreamEvent::TurnCompleted {
                        stop_reason,
                    }) => {
                        final_stop_reason = Some(stop_reason);
                    }
                    Ok(_) => {} // Ignore other events for replay
                    Err(e) => return Err(e),
                }
            }

            let response = CompletionResponse {
                content: collected_text.clone(),
                model: request.model.clone(),
                usage: final_usage.clone(),
                stop_reason: final_stop_reason.clone(),
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            };

            // Save the recording
            let record = Record {
                input: RecordedInput {
                    model: request.model.clone(),
                    messages_hash: hash.clone(),
                    system_prompt: request.system_prompt.clone(),
                },
                output: RecordedOutput {
                    content: collected_text,
                    model: response.model.clone(),
                    usage: final_usage,
                    stop_reason: final_stop_reason,
                },
            };

            {
                let mut records = self.records.lock().map_err(|_| {
                    ProviderError::Unknown("Failed to acquire records lock".to_string())
                })?;
                records.insert(hash, record);
            }

            // Return the collected response as a single-chunk stream
            let stream_chunk: StreamChunk =
                Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta {
                    content: response.content,
                });
            Ok(Box::pin(futures::stream::once(async move { stream_chunk })))
        } else {
            // Replay mode: return saved response as a stream
            let records = self.records.lock().map_err(|_| {
                ProviderError::Unknown("Failed to acquire records lock".to_string())
            })?;
            match records.get(&hash) {
                Some(record) => {
                    let text = record.output.content.clone();
                    let stream_chunk: StreamChunk =
                        Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta {
                            content: text,
                        });
                    Ok(Box::pin(futures::stream::once(async move { stream_chunk })))
                }
                None => Err(ProviderError::Unknown(format!(
                    "No recorded response found for input hash: {}",
                    hash
                ))),
            }
        }
    }

    fn config(&self) -> Option<&ProviderConfig> {
        self.inner.as_ref().and_then(|p| p.config())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ChatMessage;
    use crate::MockProvider;

    fn test_request(messages: Vec<ChatMessage>) -> CompletionRequest {
        CompletionRequest::new("test-model", messages)
    }

    #[test]
    fn test_record_and_replay_complete() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_replay_{}.json", std::process::id()));

        let mock = Arc::new(MockProvider::from_text("Hello from mock!"));

        // Recording phase
        {
            let recorder = ReplayProvider::new_recording(mock, &temp_file);
            assert!(recorder.is_recording());
            assert_eq!(recorder.record_count(), 0);

            let response = futures::executor::block_on(LLMProvider::complete(
                &recorder,
                test_request(vec![ChatMessage::user("test".to_string())]),
            ))
            .unwrap();

            assert_eq!(response.content, "Hello from mock!");
            assert_eq!(recorder.record_count(), 1);
            recorder.finish_recording().unwrap();
        }

        // Replay phase
        {
            let replayer = ReplayProvider::new_replaying(&temp_file).unwrap();
            assert!(!replayer.is_recording());

            let response = futures::executor::block_on(LLMProvider::complete(
                &replayer,
                test_request(vec![ChatMessage::user("test".to_string())]),
            ))
            .unwrap();

            assert_eq!(response.content, "Hello from mock!");
        }

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_replay_missing_record_returns_error() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_replay_missing_{}.json", std::process::id()));

        // Write an empty recording file
        std::fs::write(&temp_file, "{}").unwrap();

        let replayer = ReplayProvider::new_replaying(&temp_file).unwrap();
        let result = futures::executor::block_on(LLMProvider::complete(
            &replayer,
            test_request(vec![ChatMessage::user("different input".to_string())]),
        ));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No recorded response found"));

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_record_and_replay_stream() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_replay_stream_{}.json", std::process::id()));

        let mock = Arc::new(MockProvider::from_text("Stream response!"));

        // Recording phase via stream
        {
            let recorder = ReplayProvider::new_recording(mock, &temp_file);
            let stream = futures::executor::block_on(LLMProvider::complete_stream(
                &recorder,
                test_request(vec![ChatMessage::user("stream test".to_string())]),
            ))
            .unwrap();

            let chunks: Vec<_> = futures::executor::block_on(stream.collect());
            assert!(!chunks.is_empty());
            assert_eq!(recorder.record_count(), 1);
            recorder.finish_recording().unwrap();
        }

        // Replay phase via stream
        {
            let replayer = ReplayProvider::new_replaying(&temp_file).unwrap();
            let stream = futures::executor::block_on(LLMProvider::complete_stream(
                &replayer,
                test_request(vec![ChatMessage::user("stream test".to_string())]),
            ))
            .unwrap();

            let chunks: Vec<_> = futures::executor::block_on(stream.collect());
            assert!(!chunks.is_empty());

            // Extract text from the first chunk
            match chunks[0].as_ref().unwrap() {
                rustycode_protocol::stream_event::StreamEvent::TextDelta { content } => {
                    assert_eq!(content, "Stream response!");
                }
                _ => panic!("Expected TextDelta variant"),
            }
        }

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_different_inputs_get_different_hashes() {
        let req1 = test_request(vec![ChatMessage::user("hello".to_string())]);
        let req2 = test_request(vec![ChatMessage::user("world".to_string())]);
        let hash1 = ReplayProvider::hash_request(&req1);
        let hash2 = ReplayProvider::hash_request(&req2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_same_input_gets_same_hash() {
        let req1 = test_request(vec![ChatMessage::user("hello".to_string())]);
        let req2 = test_request(vec![ChatMessage::user("hello".to_string())]);
        let hash1 = ReplayProvider::hash_request(&req1);
        let hash2 = ReplayProvider::hash_request(&req2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_replay_invalid_file_returns_error() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_replay_invalid_{}.json", std::process::id()));
        std::fs::write(&temp_file, "not json").unwrap();

        let result = ReplayProvider::new_replaying(&temp_file);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&temp_file);
    }
}
