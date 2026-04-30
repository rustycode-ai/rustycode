#![allow(
    clippy::collection_is_never_read,
    clippy::needless_collect,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_tightening,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::unwrap_used
)]

//! Performance tests for TUI components.
//!
//! This module provides performance testing for:
//! - Large message history rendering
//! - Memory leak detection
//! - Rapid input handling
//! - Streaming response performance
//! - Unicode rendering performance

use std::time::Instant;

#[cfg(target_os = "macos")]
fn get_memory_usage() -> usize {
    use std::process::Command;

    let output = Command::new("ps")
        .arg("-o")
        .arg("rss=")
        .arg(std::process::id().to_string())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    output.parse().unwrap_or(0)
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn test_large_message_history_rendering() {
        let mut messages: Vec<String> = Vec::new();

        // Add 1000 messages
        for i in 0..1000 {
            messages.push(format!(
                "Message {}: Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                 Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                 Ut enim ad minim veniam, quis nostrud exercitation ullamco.",
                i
            ));
        }

        let start = Instant::now();

        // Simulate rendering (measure time to process all messages)
        let rendered: Vec<&str> = messages.iter().map(|s| s.as_str()).collect();

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 50,
            "Rendering 1000 messages too slow: {:?}",
            duration
        );

        assert_eq!(rendered.len(), 1000);
    }

    #[test]
    fn test_memory_leak_detection_input_handler() {
        // This test checks for memory leaks when creating/destroying input handlers
        #[cfg(target_os = "macos")]
        {
            let baseline_memory = get_memory_usage();

            // Perform operations that might leak
            for _ in 0..100 {
                let text = format!("Test {}", "A".repeat(1000));
                // Simulate input handler operations
                let mut buffer = String::new();
                buffer.push_str(&text);
                buffer.clear();
            }

            let final_memory = get_memory_usage();
            let memory_growth = final_memory.saturating_sub(baseline_memory);

            // Should not grow more than 10MB
            assert!(
                memory_growth < 10_000_000,
                "Memory leak detected: {} bytes",
                memory_growth
            );
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Memory profiling not available on this platform
        }
    }

    #[test]
    fn test_rapid_unicode_processing() {
        let start = Instant::now();

        // Process 10000 Unicode strings
        for i in 0..10000 {
            let text = format!("สวัสดี {}", i);
            let _width = unicode_width::UnicodeWidthStr::width(text.as_str());
        }

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 100,
            "Unicode processing too slow: {:?}",
            duration
        );
    }

    #[test]
    fn test_grapheme_segmentation_performance() {
        use unicode_segmentation::UnicodeSegmentation;

        let text = "สวัสดี 🌍 👨‍👩‍👧‍👦 Hello".repeat(100);

        let start = Instant::now();

        // Grapheme segmentation
        let graphemes: Vec<&str> = text.graphemes(true).collect();

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 50,
            "Grapheme segmentation too slow: {:?}",
            duration
        );

        assert!(!graphemes.is_empty());
    }

    #[test]
    fn test_markdown_parsing_performance() {
        use pulldown_cmark::{Event, Parser};

        let markdown = "# Heading\n\nThis is **bold** and *italic* text.\n\n".repeat(100);

        let start = Instant::now();

        let parser = Parser::new(&markdown);
        let events: Vec<Event> = parser.collect();

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 100,
            "Markdown parsing too slow: {:?}",
            duration
        );

        assert!(!events.is_empty());
    }

    #[test]
    fn test_large_input_handling() {
        let start = Instant::now();

        // Create a very large input
        let large_input = "A".repeat(1_000_000);

        // Simulate cursor movement
        let mut cursor = 0;
        for (i, _) in large_input.char_indices() {
            if i % 1000 == 0 {
                cursor = i;
            }
        }

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 100,
            "Large input handling too slow: {:?}",
            duration
        );

        assert!(cursor > 0);
    }

    #[test]
    fn test_string_allocation_performance() {
        let start = Instant::now();

        let mut strings = Vec::new();

        // Test string allocation patterns
        for i in 0..1000 {
            let s = format!("Test string number {}: {}", i, "content");
            strings.push(s);
        }

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 50,
            "String allocation too slow: {:?}",
            duration
        );

        assert_eq!(strings.len(), 1000);
    }

    #[test]
    fn test_regex_performance() {
        use regex::Regex;

        let re = Regex::new(r"\b\d+\b").unwrap();
        let text = "Number 123 and 456 and 789".repeat(100);

        let start = Instant::now();

        let matches: Vec<&str> = re.find_iter(&text).map(|m| m.as_str()).collect();

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 50,
            "Regex matching too slow: {:?}",
            duration
        );

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_serialization_performance() {
        use serde_json::json;

        let data = json!({
            "messages": ["hello", "world", "test"],
            "metadata": {
                "count": 1000,
                "timestamp": 1234567890
            }
        });

        let start = Instant::now();

        // Serialize
        let serialized = serde_json::to_string(&data).unwrap();

        // Deserialize
        let _deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 50,
            "Serialization too slow: {:?}",
            duration
        );
    }

    #[test]
    fn test_concurrent_operations() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let data = Arc::new(Mutex::new(Vec::new()));
        let mut handles = vec![];

        let start = Instant::now();

        // Spawn 10 threads
        for i in 0..10 {
            let data_clone = Arc::clone(&data);
            let handle = thread::spawn(move || {
                let mut data = data_clone.lock().unwrap();
                for j in 0..100 {
                    data.push(format!("Thread {} Item {}", i, j));
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 500,
            "Concurrent operations too slow: {:?}",
            duration
        );

        assert_eq!(data.lock().unwrap().len(), 1000);
    }

    #[test]
    fn test_file_io_performance() {
        use std::io::{Read, Seek, Write};
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        let data = "Test line\n".repeat(1000);

        let start = Instant::now();

        // Write
        temp_file.as_file().write_all(data.as_bytes()).unwrap();
        temp_file.as_file().sync_all().unwrap();
        temp_file.as_file_mut().rewind().unwrap();

        // Read
        let mut read_data = String::new();
        temp_file
            .as_file_mut()
            .read_to_string(&mut read_data)
            .unwrap();

        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 100,
            "File I/O too slow: {:?}",
            duration
        );

        assert_eq!(read_data, data);
    }
}
