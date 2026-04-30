use std::time::Duration;

fn blocking_sleep(duration: Duration) {
    std::thread::sleep(duration);
}

fn blocking_sleep_off_runtime(duration: Duration) {
    let handle = std::thread::spawn(move || blocking_sleep(duration));
    if let Err(e) = handle.join() {
        tracing::error!("blocking sleep thread panicked: {:?}", e);
    }
}

/// Hybrid sleep helper that uses `tokio::time::sleep` when running inside a
/// Tokio runtime, otherwise falls back to blocking `std::thread::sleep`.
pub async fn hybrid_sleep(duration: Duration) {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::time::sleep(duration).await;
    } else {
        blocking_sleep_off_runtime(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn hybrid_sleep_blocks_outside_tokio() {
        let dur = Duration::from_millis(20);
        let start = Instant::now();
        // Run the async function on a dedicated thread to ensure no Tokio
        // runtime is present on the current thread (avoids nested runtime
        // panics when tests run in parallel with tokio tests).
        let handle = std::thread::spawn(move || {
            futures::executor::block_on(hybrid_sleep(dur));
        });
        handle.join().unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed >= dur,
            "expected blocking sleep to last at least the duration"
        );
    }

    #[tokio::test]
    async fn hybrid_sleep_runs_inside_tokio() {
        let dur = Duration::from_millis(20);
        let start = Instant::now();
        hybrid_sleep(dur).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= dur,
            "expected async sleep to last at least the duration"
        );
    }
}
