//! Repetition detector — warns when tool outputs repeat.

use async_trait::async_trait;
use serde_json::Value;

use super::AgentPlugin;

/// Detects repeated tool outputs and appends a warning.
///
/// Keeps a sliding window of recent outputs. When a new output matches one
/// already in the window, a NOTE is appended suggesting a different approach.
pub struct RepetitionDetector {
    history: Vec<String>,
    max_history: usize,
}

impl RepetitionDetector {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::with_capacity(max_history),
            max_history,
        }
    }
}

#[async_trait]
impl AgentPlugin for RepetitionDetector {
    async fn on_tool_result(
        &mut self,
        _tool_name: &str,
        _tool_id: &str,
        _input: &Value,
        output: &mut String,
    ) {
        if self.history.contains(output) {
            output.push_str(
                "\n\nNOTE: Repetition detected: This output has appeared previously. \
                 Consider a different approach.",
            );
        }
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(output.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_ctx() -> crate::plugins::TurnContext {
        crate::plugins::TurnContext {
            turn: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            cwd: PathBuf::from("/tmp"),
        }
    }

    #[tokio::test]
    async fn no_warning_on_first_occurrence() {
        let mut detector = RepetitionDetector::new(5);
        let mut output = "file contents".to_string();
        detector
            .on_tool_result("Read", "1", &Value::Null, &mut output)
            .await;
        assert!(!output.contains("Repetition detected"));
    }

    #[tokio::test]
    async fn warns_on_repeated_output() {
        let mut detector = RepetitionDetector::new(5);
        let mut output = "file contents".to_string();
        detector
            .on_tool_result("Read", "1", &Value::Null, &mut output)
            .await;

        let mut output2 = "file contents".to_string();
        detector
            .on_tool_result("Read", "2", &Value::Null, &mut output2)
            .await;
        assert!(output2.contains("Repetition detected"));
    }

    #[tokio::test]
    async fn sliding_window_evicts_old_entries() {
        let mut detector = RepetitionDetector::new(2);
        for i in 0..3 {
            let mut output = format!("output {i}");
            detector
                .on_tool_result("Read", "1", &Value::Null, &mut output)
                .await;
        }
        let mut output = "output 0".to_string();
        detector
            .on_tool_result("Read", "1", &Value::Null, &mut output)
            .await;
        assert!(!output.contains("Repetition detected"));
    }

    #[tokio::test]
    async fn default_methods_are_noop() {
        let mut detector = RepetitionDetector::new(5);
        let ctx = make_ctx();
        detector.on_start(&ctx).await;
        assert!(!detector.should_stop(&ctx).await);
        detector.on_done(&ctx).await;
    }
}
