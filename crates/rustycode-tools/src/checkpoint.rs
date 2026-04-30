//! Checkpoint support for interruptible tool execution
//!
//! This module provides traits and utilities for tools to support
//! safe cancellation at checkpoints during long-running operations.

use crate::ToolContext;
use anyhow::Result;

/// Trait for types that support checkpoint-based cancellation
pub trait Checkpoint {
    /// Check if the operation should continue
    ///
    /// Returns Ok(true) if the operation should continue
    /// Returns Err if the operation has been cancelled
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn process_files(ctx: &ToolContext) -> Result<()> {
    ///     for file in files {
    ///         ctx.checkpoint()?;  // Check for cancellation
    ///         process_file(file)?;
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn checkpoint(&self) -> Result<()> {
        Ok(())
    }
}

impl Checkpoint for ToolContext {
    fn checkpoint(&self) -> Result<()> {
        if let Some(token) = &self.cancellation_token {
            if token.is_cancelled() {
                return Err(anyhow::anyhow!("operation cancelled by user"));
            }
        }
        Ok(())
    }
}

/// Helper extension trait for convenience
pub trait CheckpointExt: Checkpoint {
    /// Check for cancellation, continuing if not cancelled
    fn checkpoint_if_needed(&self) -> Result<()> {
        self.checkpoint()
    }
}

impl<T: Checkpoint> CheckpointExt for T {}

/// Run a function with cancellation checking between iterations
///
/// # Example
///
/// ```ignore
/// let results = with_cancellation(&ctx, |ctx| {
///     for item in items {
///         ctx.checkpoint()?;
///         process_item(item)?;
///     }
///     Ok(())
/// });
/// ```
pub fn with_cancellation<F, T>(ctx: &ToolContext, f: F) -> Result<T>
where
    F: FnOnce(&ToolContext) -> Result<T>,
{
    ctx.checkpoint()?;
    f(ctx)
}

/// Execute an iterator with cancellation checking between items
///
/// # Example
///
/// ```ignore
/// let results = cancellable_iter(&ctx, items.iter(), |item| {
///     process(item)
/// })?;
/// ```
pub fn cancellable_iter<I, F, T>(ctx: &ToolContext, iter: I, mut f: F) -> Result<Vec<T>>
where
    I: Iterator,
    F: FnMut(I::Item) -> Result<T>,
{
    let mut results = Vec::new();

    for item in iter {
        ctx.checkpoint()?;
        results.push(f(item)?);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CancellationToken;

    #[test]
    fn test_checkpoint_not_cancelled() {
        let ctx = ToolContext::new("/tmp");
        assert!(ctx.checkpoint().is_ok());
    }

    #[test]
    fn test_checkpoint_cancelled() {
        let token = CancellationToken::cancelled();
        let ctx = ToolContext::new("/tmp").with_cancellation(token);
        assert!(ctx.checkpoint().is_err());
        assert!(ctx
            .checkpoint()
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
    }

    #[test]
    fn test_with_cancellation_success() {
        let ctx = ToolContext::new("/tmp");
        let result = with_cancellation(&ctx, |_ctx| Ok(42));
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_with_cancellation_cancelled() {
        let token = CancellationToken::cancelled();
        let ctx = ToolContext::new("/tmp").with_cancellation(token);
        let result = with_cancellation(&ctx, |_ctx| Ok(42));
        assert!(result.is_err());
    }

    #[test]
    fn test_cancellable_iter_success() {
        let ctx = ToolContext::new("/tmp");
        let items = vec![1, 2, 3, 4, 5];
        let result = cancellable_iter(&ctx, items.into_iter(), |x| Ok(x * 2));
        assert_eq!(result.unwrap(), vec![2, 4, 6, 8, 10]);
    }

    #[test]
    fn test_cancellable_iter_cancelled() {
        let token = CancellationToken::cancelled();
        let ctx = ToolContext::new("/tmp").with_cancellation(token);
        let items = vec![1, 2, 3, 4, 5];
        let result = cancellable_iter(&ctx, items.into_iter(), |x| Ok(x * 2));
        // Should return empty vec since checkpoint fails immediately
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
