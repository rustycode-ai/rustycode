//! Rate limiter for concurrent bash executions.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Rate limiter for concurrent bash executions.
///
/// Limits the number of bash commands that can run simultaneously
/// to prevent resource exhaustion and ensure system stability.
pub(super) struct BashRateLimiter {
    /// Current number of active executions
    active: AtomicUsize,
    /// Maximum allowed concurrent executions (public for error messages)
    pub max_concurrent: usize,
}

impl BashRateLimiter {
    /// Create a new rate limiter with the specified maximum concurrency.
    pub(super) const fn new(max_concurrent: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            max_concurrent,
        }
    }

    /// Try to acquire a permit to execute a bash command.
    ///
    /// Returns Ok(permit) if successful, Err if rate limit exceeded.
    /// The permit should be dropped after execution completes.
    pub(super) fn try_acquire(&self) -> Result<BashPermit<'_>, ()> {
        let current = self.active.load(Ordering::Relaxed);

        if current >= self.max_concurrent {
            return Err(());
        }

        let mut old = current;
        loop {
            if old >= self.max_concurrent {
                return Err(());
            }

            match self.active.compare_exchange_weak(
                old,
                old + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => old = actual,
            }
        }

        Ok(BashPermit {
            limiter: self,
            _private: (),
        })
    }

    fn release(&self) {
        self.active.fetch_sub(1, Ordering::Release);
    }

    pub(super) fn active_count(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }
}

/// Permit that represents an acquired slot for bash execution.
///
/// When dropped, automatically releases the permit back to the limiter.
pub(super) struct BashPermit<'a> {
    limiter: &'a BashRateLimiter,
    _private: (),
}

impl Drop for BashPermit<'_> {
    fn drop(&mut self) {
        self.limiter.release();
    }
}

/// Global rate limiter — limits to 5 concurrent bash commands by default.
pub(super) static BASH_RATE_LIMITER: BashRateLimiter = BashRateLimiter::new(5);
