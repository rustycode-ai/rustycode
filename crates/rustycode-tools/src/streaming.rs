use anyhow::Result;
use std::sync::mpsc;

/// Stream chunk for tool output streaming
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Chunk of output text
    pub text: String,
    /// Whether this is the final chunk
    pub is_done: bool,
    /// Optional error if streaming failed
    pub error: Option<String>,
}

impl StreamChunk {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_done: false,
            error: None,
        }
    }

    pub const fn done() -> Self {
        Self {
            text: String::new(),
            is_done: true,
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            is_done: true,
            error: Some(error.into()),
        }
    }
}

/// Receiver for streaming tool output (with timeout support)
pub type StreamReceiver = mpsc::Receiver<StreamChunk>;

/// Sender for streaming tool output
pub type StreamSender = mpsc::Sender<StreamChunk>;

/// Create a new streaming channel
pub fn create_stream_channel() -> (StreamSender, StreamReceiver) {
    mpsc::channel()
}

/// Streaming extension for tools
pub trait ToolStreaming {
    /// Execute tool and return a streaming receiver
    ///
    /// # Arguments
    ///
    /// * `params` - Tool parameters
    /// * `ctx` - Tool execution context
    ///
    /// # Returns
    ///
    /// A receiver that yields output chunks as they're produced
    fn execute_stream(
        &self,
        params: serde_json::Value,
        ctx: &crate::ToolContext,
    ) -> Result<StreamReceiver>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_chunk_new() {
        let chunk = StreamChunk::new("hello");
        assert_eq!(chunk.text, "hello");
        assert!(!chunk.is_done);
        assert!(chunk.error.is_none());
    }

    #[test]
    fn test_stream_chunk_new_empty() {
        let chunk = StreamChunk::new("");
        assert_eq!(chunk.text, "");
        assert!(!chunk.is_done);
    }

    #[test]
    fn test_stream_chunk_done() {
        let chunk = StreamChunk::done();
        assert_eq!(chunk.text, "");
        assert!(chunk.is_done);
        assert!(chunk.error.is_none());
    }

    #[test]
    fn test_stream_chunk_error() {
        let chunk = StreamChunk::error("something went wrong");
        assert_eq!(chunk.text, "");
        assert!(chunk.is_done);
        assert_eq!(chunk.error, Some("something went wrong".to_string()));
    }

    #[test]
    fn test_create_stream_channel_send_receive() {
        let (tx, rx) = create_stream_channel();
        tx.send(StreamChunk::new("chunk1")).unwrap();
        tx.send(StreamChunk::new("chunk2")).unwrap();
        tx.send(StreamChunk::done()).unwrap();

        let c1 = rx.recv().unwrap();
        assert_eq!(c1.text, "chunk1");
        assert!(!c1.is_done);

        let c2 = rx.recv().unwrap();
        assert_eq!(c2.text, "chunk2");

        let c3 = rx.recv().unwrap();
        assert!(c3.is_done);
    }

    #[test]
    fn test_stream_chunk_clone() {
        let original = StreamChunk::new("data");
        let cloned = original.clone();
        assert_eq!(cloned.text, "data");
        assert_eq!(cloned.is_done, original.is_done);
    }

    #[test]
    fn test_stream_chunk_error_is_done() {
        // Error chunks should always be marked as done
        let chunk = StreamChunk::error("timeout");
        assert!(chunk.is_done);
    }
}
