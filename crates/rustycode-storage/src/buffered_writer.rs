//! Buffered writer with periodic flushing for high-frequency writes.
//!
//! Batched I/O writer with configurable flushing. Batches multiple small
//! writes into a single I/O operation, flushing on a timer, when the buffer
//! exceeds a size threshold, or when explicitly requested.
//!
//! Useful for log sinks, session persistence, and any scenario with frequent
//! small writes where immediate disk I/O would be wasteful.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::warn;

/// Default flush interval in milliseconds.
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 1000;

/// Default maximum number of items buffered before auto-flush.
const DEFAULT_MAX_BUFFER_SIZE: usize = 100;

/// Default maximum bytes buffered before auto-flush.
const DEFAULT_MAX_BUFFER_BYTES: usize = 64 * 1024; // 64 KB

/// Messages sent to the writer thread.
enum WriterMessage {
    /// Append content to the buffer.
    Write(String),
    /// Flush the buffer to the underlying writer.
    Flush,
    /// Shut down the writer thread, flushing any remaining content.
    Shutdown,
}

/// Configuration for the buffered writer.
#[derive(Debug, Clone)]
pub struct BufferedWriterConfig {
    /// Interval between automatic flushes.
    pub flush_interval: Duration,
    /// Maximum number of items in the buffer before auto-flush.
    pub max_buffer_size: usize,
    /// Maximum bytes in the buffer before auto-flush.
    pub max_buffer_bytes: usize,
    /// If true, each write is immediately flushed (passthrough mode).
    pub immediate_mode: bool,
}

impl Default for BufferedWriterConfig {
    fn default() -> Self {
        Self {
            flush_interval: Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
            max_buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            max_buffer_bytes: DEFAULT_MAX_BUFFER_BYTES,
            immediate_mode: false,
        }
    }
}

impl BufferedWriterConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn with_flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    pub const fn with_max_buffer_size(mut self, size: usize) -> Self {
        self.max_buffer_size = size;
        self
    }

    pub const fn with_max_buffer_bytes(mut self, bytes: usize) -> Self {
        self.max_buffer_bytes = bytes;
        self
    }

    pub const fn with_immediate_mode(mut self, enabled: bool) -> Self {
        self.immediate_mode = enabled;
        self
    }
}

/// A buffered writer that batches writes and flushes periodically.
///
/// Uses a background thread to accumulate writes and flush them to a
/// caller-provided write function. Thread-safe: the handle can be shared
/// across threads.
///
/// # Example
///
/// ```rust,no_run
/// use rustycode_storage::buffered_writer::{BufferedWriter, BufferedWriterConfig};
/// use std::time::Duration;
///
/// let config = BufferedWriterConfig::new()
///     .with_flush_interval(Duration::from_millis(500))
///     .with_max_buffer_size(50);
///
/// let writer = BufferedWriter::new(
///     |content: &str| {
///         println!("{}", content);
///     },
///     config,
/// );
///
/// writer.write("Hello ");
/// writer.write("world");
/// writer.flush(); // Forces immediate write
/// writer.dispose(); // Flushes remaining content and shuts down (blocks until done)
/// ```
pub struct BufferedWriter {
    sender: mpsc::Sender<WriterMessage>,
    handle: Option<JoinHandle<()>>,
}

impl BufferedWriter {
    /// Create a new buffered writer.
    ///
    /// The `write_fn` closure is called with the accumulated content
    /// whenever the buffer is flushed.
    pub fn new(write_fn: impl FnMut(&str) + Send + 'static, config: BufferedWriterConfig) -> Self {
        let (sender, receiver) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            run_writer_loop(receiver, write_fn, config);
        });

        Self {
            sender,
            handle: Some(handle),
        }
    }

    /// Write content to the buffer.
    pub fn write(&self, content: impl Into<String>) {
        if let Err(e) = self.sender.send(WriterMessage::Write(content.into())) {
            warn!(
                "BufferedWriter: failed to send write (writer thread shut down?): {}",
                e
            );
        }
    }

    /// Flush the buffer, writing all buffered content immediately.
    pub fn flush(&self) {
        if let Err(e) = self.sender.send(WriterMessage::Flush) {
            warn!(
                "BufferedWriter: failed to send flush (writer thread shut down?): {}",
                e
            );
        }
    }

    /// Dispose of the writer, flushing any remaining content and
    /// shutting down the background thread. Blocks until the thread
    /// has flushed and exited.
    pub fn dispose(mut self) {
        if let Err(e) = self.sender.send(WriterMessage::Shutdown) {
            warn!("BufferedWriter: failed to send shutdown: {}", e);
        }
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                warn!("BufferedWriter: writer thread panicked: {:?}", e);
            }
        }
    }
}

/// Run the writer loop on the background thread.
fn run_writer_loop(
    receiver: mpsc::Receiver<WriterMessage>,
    mut write_fn: impl FnMut(&str),
    config: BufferedWriterConfig,
) {
    let mut buffer: Vec<String> = Vec::new();
    let mut buffer_bytes: usize = 0;

    let flush = |buffer: &mut Vec<String>, bytes: &mut usize, write_fn: &mut dyn FnMut(&str)| {
        if buffer.is_empty() {
            return;
        }
        // Pre-allocate capacity to avoid repeated reallocations
        let total_len: usize = buffer.iter().map(String::len).sum();
        let mut combined = String::with_capacity(total_len);
        for s in buffer.drain(..) {
            combined.push_str(&s);
        }
        *bytes = 0;
        write_fn(&combined);
    };

    loop {
        match receiver.recv_timeout(config.flush_interval) {
            Ok(WriterMessage::Write(content)) => {
                if config.immediate_mode {
                    write_fn(&content);
                    continue;
                }

                buffer_bytes = buffer_bytes.saturating_add(content.len());
                buffer.push(content);

                if buffer.len() >= config.max_buffer_size || buffer_bytes >= config.max_buffer_bytes
                {
                    flush(&mut buffer, &mut buffer_bytes, &mut write_fn);
                }
            }
            Ok(WriterMessage::Flush) => {
                flush(&mut buffer, &mut buffer_bytes, &mut write_fn);
            }
            Ok(WriterMessage::Shutdown) => {
                flush(&mut buffer, &mut buffer_bytes, &mut write_fn);
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush(&mut buffer, &mut buffer_bytes, &mut write_fn);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush(&mut buffer, &mut buffer_bytes, &mut write_fn);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn immediate_mode_writes_directly() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new().with_immediate_mode(true);
        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        writer.write("hello");
        writer.write(" ");
        writer.write("world");
        writer.dispose();

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn buffered_mode_flushes_on_dispose() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new().with_immediate_mode(false);
        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        writer.write("hello");
        writer.write(" ");
        writer.write("world");
        writer.dispose(); // blocks until flushed

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn explicit_flush() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new()
            .with_immediate_mode(false)
            .with_max_buffer_size(1000); // High threshold so it won't auto-flush

        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        writer.write("chunk1");
        writer.flush();
        // Give the thread time to process the flush
        std::thread::sleep(Duration::from_millis(50));
        writer.write("chunk2");
        writer.dispose();

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(result, "chunk1chunk2");
    }

    #[test]
    fn auto_flush_on_buffer_size() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new()
            .with_immediate_mode(false)
            .with_max_buffer_size(3)
            .with_max_buffer_bytes(usize::MAX)
            .with_flush_interval(Duration::from_mins(1)); // Long interval

        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        writer.write("a");
        writer.write("b");
        writer.write("c");
        writer.write("d"); // triggers auto-flush (4 items >= max_buffer_size 3)
        writer.write("e"); // remaining in buffer
        writer.dispose(); // flushes "e"

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // "abcd" from auto-flush + "e" from dispose flush
        assert_eq!(result, "abcde");
    }

    #[test]
    fn default_config_values() {
        let config = BufferedWriterConfig::default();
        assert_eq!(config.flush_interval, Duration::from_secs(1));
        assert_eq!(config.max_buffer_size, 100);
        assert_eq!(config.max_buffer_bytes, 64 * 1024);
        assert!(!config.immediate_mode);
    }

    #[test]
    fn config_builder() {
        let config = BufferedWriterConfig::new()
            .with_flush_interval(Duration::from_millis(200))
            .with_max_buffer_size(50)
            .with_max_buffer_bytes(1024)
            .with_immediate_mode(true);

        assert_eq!(config.flush_interval, Duration::from_millis(200));
        assert_eq!(config.max_buffer_size, 50);
        assert_eq!(config.max_buffer_bytes, 1024);
        assert!(config.immediate_mode);
    }

    #[test]
    fn auto_flush_on_byte_threshold() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new()
            .with_immediate_mode(false)
            .with_max_buffer_size(1000) // High item count
            .with_max_buffer_bytes(10) // Low byte threshold
            .with_flush_interval(Duration::from_mins(1)); // Long interval

        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        writer.write("12345"); // 5 bytes
        writer.write("67890"); // 10 bytes total >= 10 threshold → auto-flush
        writer.write("abc"); // remaining
        writer.dispose(); // flushes remaining "abc"

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(result, "1234567890abc");
    }

    #[test]
    fn write_empty_string() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new().with_immediate_mode(true);
        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        writer.write("");
        writer.write("hello");
        writer.dispose();

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(result, "hello");
    }

    #[test]
    fn config_debug_format() {
        let config = BufferedWriterConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("BufferedWriterConfig"));
        assert!(debug.contains("flush_interval"));
        assert!(debug.contains("immediate_mode"));
    }

    #[test]
    fn config_new_equals_default() {
        let new = BufferedWriterConfig::new();
        let default = BufferedWriterConfig::default();
        assert_eq!(new.flush_interval, default.flush_interval);
        assert_eq!(new.max_buffer_size, default.max_buffer_size);
        assert_eq!(new.max_buffer_bytes, default.max_buffer_bytes);
        assert_eq!(new.immediate_mode, default.immediate_mode);
    }

    #[test]
    fn flush_on_empty_buffer_is_noop() {
        let written = Arc::new(Mutex::new(String::new()));
        let written_clone = written.clone();

        let config = BufferedWriterConfig::new().with_immediate_mode(false);
        let writer = BufferedWriter::new(
            move |content: &str| {
                written_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_str(content);
            },
            config,
        );

        // Flush empty buffer — should not call write_fn
        writer.flush();
        std::thread::sleep(Duration::from_millis(50));
        writer.dispose();

        let result = written.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert!(result.is_empty());
    }

    #[test]
    fn default_constants() {
        assert_eq!(DEFAULT_FLUSH_INTERVAL_MS, 1000);
        assert_eq!(DEFAULT_MAX_BUFFER_SIZE, 100);
        assert_eq!(DEFAULT_MAX_BUFFER_BYTES, 64 * 1024);
    }
}
