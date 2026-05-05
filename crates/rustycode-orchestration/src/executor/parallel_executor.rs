//! Semaphore-bounded parallel tool executor.
//!
//! [`ParallelExecutor`] runs a batch of tools concurrently, limited by a
//! configurable `max_concurrent` cap. Results are returned in **input order**
//! regardless of completion order so callers can correlate outputs by index.

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

/// Trait for async operations that can be executed by the parallel executor.
///
/// Implementors define a single `execute` method. The trait is object-safe and
/// `Send` so tasks can be spawned on the Tokio runtime.
#[async_trait]
pub trait ToolExecution: Send + 'static {
    /// The output type produced on success.
    type Output: Send;

    /// Execute the tool and return its output or an error.
    async fn execute(&self) -> anyhow::Result<Self::Output>;
}

/// Runs multiple [`ToolExecution`] tasks concurrently with a bounded semaphore.
///
/// Results preserve the order of the input slice so that `results[i]`
/// corresponds to `tools[i]`.
pub struct ParallelExecutor {
    /// Maximum number of tools that may execute simultaneously.
    max_concurrent: usize,
}

impl ParallelExecutor {
    /// Create a new executor that allows at most `max_concurrent` tools in
    /// flight at once.
    ///
    pub fn new(max_concurrent: usize) -> Self {
        assert!(max_concurrent > 0, "max_concurrent must be > 0");
        Self { max_concurrent }
    }

    /// Execute all `tools` concurrently (up to `max_concurrent` at a time) and
    /// return a `Vec` of results in the same order as the input.
    ///
    /// Individual tool failures do **not** abort the batch. The caller inspects
    /// each `Result` to decide how to handle partial failures.
    pub async fn execute_all<T>(&self, tools: Vec<T>) -> Vec<anyhow::Result<T::Output>>
    where
        T: ToolExecution,
    {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));
        let total = tools.len();
        let mut futures = FuturesUnordered::new();

        for (index, tool) in tools.into_iter().enumerate() {
            let sem = Arc::clone(&semaphore);
            futures.push(async move {
                // Wait for a permit before starting execution.
                // Semaphore is never closed while tasks are in-flight, so
                // acquire is infallible. Use unwrap_or_else to satisfy clippy
                // and provide context if the invariant is ever violated.
                let _permit = sem.acquire().await.unwrap_or_else(|e| {
                    unreachable!("semaphore acquire failed (should never close): {e}")
                });
                let start = Instant::now();
                let result = tool.execute().await;
                let elapsed = start.elapsed();
                (index, result, elapsed)
            });
        }

        // Collect results, then re-order by original index.
        // Using (0..total).map().collect() avoids requiring Clone on the
        // element type (anyhow::Result<T::Output> is not Clone).
        let mut ordered: Vec<Option<anyhow::Result<T::Output>>> =
            (0..total).map(|_| None).collect();
        while let Some((index, result, _elapsed)) = futures.next().await {
            ordered[index] = Some(result);
        }

        // Every slot is guaranteed to be filled because we awaited all futures.
        ordered
            .into_iter()
            .map(|opt| {
                opt.unwrap_or_else(|| {
                    unreachable!("every slot is filled after awaiting all futures")
                })
            })
            .collect()
    }

    /// Returns the configured concurrency limit.
    pub const fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// A trivial tool that sleeps then returns a string.
    struct DelayedTool {
        delay_ms: u64,
        label: &'static str,
    }

    #[async_trait]
    impl ToolExecution for DelayedTool {
        type Output = String;

        async fn execute(&self) -> anyhow::Result<Self::Output> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(self.label.to_string())
        }
    }

    /// A tool that always fails.
    struct FailingTool {
        message: &'static str,
    }

    #[async_trait]
    impl ToolExecution for FailingTool {
        type Output = String;

        async fn execute(&self) -> anyhow::Result<Self::Output> {
            Err(anyhow::anyhow!("{}", self.message))
        }
    }

    /// A tool that tracks peak concurrency via an atomic counter.
    struct ConcurrencyTracker {
        peak: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        delay_ms: u64,
    }

    #[async_trait]
    impl ToolExecution for ConcurrencyTracker {
        type Output = usize;

        async fn execute(&self) -> anyhow::Result<Self::Output> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            // Track the highest concurrency observed.
            let mut current_peak = self.peak.load(Ordering::SeqCst);
            while active > current_peak {
                match self.peak.compare_exchange_weak(
                    current_peak,
                    active,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => current_peak = actual,
                }
            }
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(active)
        }
    }

    #[tokio::test]
    async fn test_executes_tools_in_parallel() {
        let executor = ParallelExecutor::new(10);
        let tools: Vec<DelayedTool> = (0..3)
            .map(|i| DelayedTool {
                delay_ms: 100,
                label: match i {
                    0 => "first",
                    1 => "second",
                    _ => "third",
                },
            })
            .collect();

        let start = Instant::now();
        let results = executor.execute_all(tools).await;
        let elapsed = start.elapsed();

        // 3 tools at 100ms each, parallel => well under 300ms.
        assert!(
            elapsed < Duration::from_millis(300),
            "parallel execution took {elapsed:?}, expected < 300ms"
        );

        assert_eq!(results.len(), 3);
        assert!(results[0].as_ref().is_ok_and(|r| r == "first"));
        assert!(results[1].as_ref().is_ok_and(|r| r == "second"));
        assert!(results[2].as_ref().is_ok_and(|r| r == "third"));
    }

    #[tokio::test]
    async fn test_respects_max_concurrent_limit() {
        let max = 2;
        let executor = ParallelExecutor::new(max);
        let peak = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));

        let tools: Vec<ConcurrencyTracker> = (0..6)
            .map(|_| ConcurrencyTracker {
                peak: Arc::clone(&peak),
                active: Arc::clone(&active),
                delay_ms: 50,
            })
            .collect();

        let results = executor.execute_all(tools).await;
        assert_eq!(results.len(), 6);
        for result in &results {
            assert!(result.is_ok(), "expected success, got {:?}", result);
        }

        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= max,
            "peak concurrency {observed_peak} exceeded limit {max}"
        );
    }

    #[tokio::test]
    async fn test_partial_failure_returns_errors() {
        let executor = ParallelExecutor::new(4);

        // Test with two homogeneous batches to verify mixed success/failure handling.
        let ok_tools: Vec<DelayedTool> = vec![
            DelayedTool {
                delay_ms: 10,
                label: "ok-0",
            },
            DelayedTool {
                delay_ms: 10,
                label: "ok-2",
            },
        ];

        let fail_tools: Vec<FailingTool> = vec![
            FailingTool {
                message: "tool-1 failed",
            },
            FailingTool {
                message: "tool-3 failed",
            },
        ];

        let ok_results = executor.execute_all(ok_tools).await;
        let fail_results = executor.execute_all(fail_tools).await;

        assert_eq!(ok_results.len(), 2);
        assert!(ok_results[0].as_ref().is_ok_and(|r| r == "ok-0"));
        assert!(ok_results[1].as_ref().is_ok_and(|r| r == "ok-2"));

        assert_eq!(fail_results.len(), 2);
        assert!(fail_results[0]
            .as_ref()
            .is_err_and(|e| e.to_string().contains("tool-1 failed")));
        assert!(fail_results[1]
            .as_ref()
            .is_err_and(|e| e.to_string().contains("tool-3 failed")));
    }

    #[tokio::test]
    async fn test_empty_input_returns_empty() {
        let executor = ParallelExecutor::new(4);
        let results: Vec<anyhow::Result<String>> =
            executor.execute_all::<DelayedTool>(vec![]).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_results_preserve_input_order() {
        // Tools with varying durations so they complete out of order.
        let executor = ParallelExecutor::new(4);
        let tools: Vec<DelayedTool> = vec![
            DelayedTool {
                delay_ms: 150,
                label: "slow-first",
            },
            DelayedTool {
                delay_ms: 10,
                label: "fast-second",
            },
            DelayedTool {
                delay_ms: 75,
                label: "medium-third",
            },
        ];

        let results = executor.execute_all(tools).await;
        assert_eq!(results.len(), 3);
        assert!(results[0].as_ref().is_ok_and(|r| r == "slow-first"));
        assert!(results[1].as_ref().is_ok_and(|r| r == "fast-second"));
        assert!(results[2].as_ref().is_ok_and(|r| r == "medium-third"));
    }
}
