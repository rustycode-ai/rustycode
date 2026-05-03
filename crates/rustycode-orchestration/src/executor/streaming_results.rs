//! Streaming tool executor that delivers results as they complete.
//!
//! [`StreamingToolExecutor`] spawns all tools concurrently and sends each
//! [`ToolResult`] through an `mpsc` channel immediately upon completion. This
//! lets consumers process fast results without waiting for slower tools.

use super::parallel_executor::ToolExecution;
use std::fmt;
use std::time::{Duration, Instant};

/// A single tool result delivered through the streaming channel.
pub struct ToolResult {
    /// Name or identifier of the tool that produced this result.
    pub tool_name: String,
    /// The outcome: `Ok(output_string)` or `Err(error)`.
    pub result: anyhow::Result<String>,
    /// Wall-clock time from task spawn to completion.
    pub completion_time: Duration,
}

impl Clone for ToolResult {
    fn clone(&self) -> Self {
        Self {
            tool_name: self.tool_name.clone(),
            result: self.result.as_ref().map(String::from).map_err(|e| anyhow::anyhow!("{e}")),
            completion_time: self.completion_time,
        }
    }
}

impl fmt::Debug for ToolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolResult")
            .field("tool_name", &self.tool_name)
            .field("result", &self.result.as_ref().map(|_| "...").map_err(std::string::ToString::to_string))
            .field("completion_time", &self.completion_time)
            .finish()
    }
}

/// Executes tools and streams results through an `mpsc` channel.
///
/// Unlike [`super::ParallelExecutor`], results are sent **in completion order**
/// (fastest first), and the caller processes them incrementally via the
/// receiver half.
#[derive(Default)]
pub struct StreamingToolExecutor;

impl StreamingToolExecutor {
    /// Create a new streaming executor (stateless; the type is a namespace).
    pub const fn new() -> Self {
        Self
    }

    /// Spawn all `tools` and send a [`ToolResult`] through `result_tx` as each
    /// completes.
    ///
    /// The returned future resolves once every tool has finished and its result
    /// has been sent (or the channel is closed).
    ///
    /// If the receiver is dropped before all results are sent, the remaining
    /// tool tasks continue to run to completion but their results are silently
    /// discarded -- this does **not** panic.
    pub async fn execute_streaming<T>(
        tools: Vec<(String, T)>,
        result_tx: tokio::sync::mpsc::Sender<ToolResult>,
    ) -> anyhow::Result<()>
    where
        T: ToolExecution,
        T::Output: Send + fmt::Display,
    {
        let mut set = tokio::task::JoinSet::new();

        for (tool_name, tool) in tools {
            let tx = result_tx.clone();
            set.spawn(async move {
                let start = Instant::now();
                let result = tool.execute().await.map(|out| out.to_string());
                let completion_time = start.elapsed();
                // If the receiver is dropped, send returns Err -- that is fine,
                // the consumer has opted out of further results.
                let _ = tx.send(ToolResult {
                    tool_name,
                    result,
                    completion_time,
                }).await;
            });
        }

        // Drop our clone so the channel closes when all senders are gone.
        drop(result_tx);

        // Await every task. Errors from the JoinSet itself (e.g. panic) are
        // propagated; tool-level errors are already captured in ToolResult.
        while let Some(res) = set.join_next().await {
            res.map_err(|e| anyhow::anyhow!("streaming task failed: {e}"))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A tool that sleeps for a configured duration then returns a label.
    struct TimedTool {
        delay_ms: u64,
        output: &'static str,
    }

    #[async_trait::async_trait]
    impl ToolExecution for TimedTool {
        type Output = String;

        async fn execute(&self) -> anyhow::Result<Self::Output> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(self.output.to_string())
        }
    }

    /// A tool that always fails.
    struct FailingTool {
        message: &'static str,
    }

    #[async_trait::async_trait]
    impl ToolExecution for FailingTool {
        type Output = String;

        async fn execute(&self) -> anyhow::Result<Self::Output> {
            Err(anyhow::anyhow!("{}", self.message))
        }
    }

    #[tokio::test]
    async fn test_streams_results_as_they_complete() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);

        let tools: Vec<(String, TimedTool)> = vec![
            (
                "slow-tool".to_string(),
                TimedTool {
                    delay_ms: 200,
                    output: "slow",
                },
            ),
            (
                "fast-tool".to_string(),
                TimedTool {
                    delay_ms: 10,
                    output: "fast",
                },
            ),
            (
                "medium-tool".to_string(),
                TimedTool {
                    delay_ms: 80,
                    output: "medium",
                },
            ),
        ];

        let handle = tokio::spawn(async move {
            StreamingToolExecutor::execute_streaming(tools, tx)
                .await
                .expect("streaming executor should succeed");
        });

        let mut results = Vec::new();
        while let Some(tool_result) = rx.recv().await {
            results.push(tool_result);
        }

        handle.await.expect("join should succeed");
        assert_eq!(results.len(), 3);

        // The fast tool should arrive first (lowest delay).
        let first_name = &results[0].tool_name;
        assert_eq!(
            first_name, "fast-tool",
            "expected fast-tool first, got {first_name}"
        );

        // Verify all three results are present.
        let names: Vec<&str> = results.iter().map(|r| r.tool_name.as_str()).collect();
        assert!(names.contains(&"fast-tool"));
        assert!(names.contains(&"medium-tool"));
        assert!(names.contains(&"slow-tool"));

        // Verify result payloads.
        for tr in &results {
            let output = tr.result.as_ref().expect("tool should succeed");
            match tr.tool_name.as_str() {
                "fast-tool" => assert_eq!(output, "fast"),
                "medium-tool" => assert_eq!(output, "medium"),
                "slow-tool" => assert_eq!(output, "slow"),
                _ => panic!("unexpected tool name"),
            }
        }
    }

    #[tokio::test]
    async fn test_streaming_allows_early_termination() {
        // Drop the receiver immediately after spawning. The executor must not
        // panic; it should complete cleanly.
        let (tx, rx) = tokio::sync::mpsc::channel(4);

        let tools: Vec<(String, TimedTool)> = (0..5)
            .map(|i| {
                (
                    format!("tool-{i}"),
                    TimedTool {
                        delay_ms: 10,
                        output: "done",
                    },
                )
            })
            .collect();

        let handle = tokio::spawn(async move {
            StreamingToolExecutor::execute_streaming(tools, tx).await
        });

        // Drop the receiver immediately.
        drop(rx);

        // The executor should still succeed (sends are silently discarded).
        let result = handle.await.expect("task should not panic");
        assert!(result.is_ok(), "executor should succeed even with dropped receiver: {result:?}");
    }

    #[tokio::test]
    async fn test_streaming_mixed_success_and_failure() {
        // Test with two separate homogeneous batches since execute_streaming
        // requires a concrete ToolExecution type (Box<dyn> does not work).
        let ok_tools: Vec<(String, TimedTool)> = vec![(
            "ok-tool".to_string(),
            TimedTool {
                delay_ms: 5,
                output: "success",
            },
        )];
        let fail_tools: Vec<(String, FailingTool)> = vec![(
            "fail-tool".to_string(),
            FailingTool {
                message: "intentional failure",
            },
        )];

        let (tx1, mut rx1) = tokio::sync::mpsc::channel(4);
        let (tx2, mut rx2) = tokio::sync::mpsc::channel(4);

        let h1 = tokio::spawn(StreamingToolExecutor::execute_streaming(ok_tools, tx1));
        let h2 = tokio::spawn(StreamingToolExecutor::execute_streaming(fail_tools, tx2));

        h1.await.expect("ok batch join").expect("ok batch exec");
        h2.await.expect("fail batch join").expect("fail batch exec");

        let ok_result = rx1
            .recv()
            .await
            .expect("should receive ok result");
        assert!(ok_result.result.is_ok());
        assert_eq!(ok_result.result.as_ref().unwrap(), "success");

        let fail_result = rx2
            .recv()
            .await
            .expect("should receive fail result");
        assert!(fail_result.result.is_err());
        assert!(
            fail_result
                .result
                .as_ref()
                .unwrap_err()
                .to_string()
                .contains("intentional failure")
        );
    }

    #[tokio::test]
    async fn test_streaming_empty_input() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);

        let tools: Vec<(String, TimedTool)> = vec![];
        StreamingToolExecutor::execute_streaming(tools, tx)
            .await
            .expect("empty input should succeed");

        // Channel should be closed with no messages.
        assert!(rx.recv().await.is_none(), "expected no results for empty input");
    }

    #[tokio::test]
    async fn test_tool_result_has_completion_time() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);

        let tools: Vec<(String, TimedTool)> = vec![(
            "timed-tool".to_string(),
            TimedTool {
                delay_ms: 50,
                output: "done",
            },
        )];

        let handle = tokio::spawn(StreamingToolExecutor::execute_streaming(tools, tx));

        let result = rx.recv().await.expect("should receive result");
        handle.await.expect("join").expect("exec");

        // The tool slept for 50ms, so completion_time should be at least that.
        assert!(
            result.completion_time >= Duration::from_millis(40),
            "completion_time {:?} too short",
            result.completion_time
        );
    }
}
