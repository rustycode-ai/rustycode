#![allow(clippy::expect_used)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::uninlined_format_args,
        clippy::needless_collect,
        clippy::redundant_clone,
        clippy::missing_const_for_fn,
        clippy::doc_lazy_continuation
    )
)]

use tokio::runtime::Runtime;

/// Shared multi-thread Tokio runtime.
///
/// Worker threads = CPU cores, named "shared-runtime-worker-*".
/// Avoids creating many short-lived runtimes that caused allocator/TLS growth.
pub static SHARED_RUNTIME: std::sync::LazyLock<Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get().min(4))
        .thread_name_fn(|| {
            static ATOMIC_ID: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("shared-runtime-worker-{id}")
        })
        .enable_all()
        .build()
        .expect("failed to build shared tokio runtime")
});

/// Spawn a future onto the shared runtime.
pub fn spawn_on_shared<F, R>(future: F) -> tokio::task::JoinHandle<R>
where
    F: std::future::Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    SHARED_RUNTIME.spawn(future)
}

/// Block on a future using the shared runtime.
pub fn block_on_shared<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    // If no Tokio runtime is active, use the shared runtime directly.
    if tokio::runtime::Handle::try_current().is_err() {
        return SHARED_RUNTIME.block_on(future);
    }

    // We're inside a Tokio runtime. Prefer to use `block_in_place` when the
    // runtime supports it (multi-threaded). We cannot directly attempt to
    // call `block_in_place` with the user's future inside a `catch_unwind`
    // because the future may be moved into the closure and lost if the
    // closure panics. Instead, first probe whether `block_in_place` is
    // permitted by running a no-op inside `block_in_place` inside
    // `catch_unwind`.
    let can_block_in_place =
        std::panic::catch_unwind(|| tokio::task::block_in_place(|| ())).is_ok();

    if can_block_in_place {
        // We're in a multi-threaded runtime. Use block_in_place to exit the
        // runtime context, then use futures::executor::block_on.
        return tokio::task::block_in_place(|| futures::executor::block_on(future));
    }

    // As a last-resort fallback (single-threaded runtime where block_in_place
    // is not permitted) run the future using `futures::executor::block_on`.
    // This will block the current thread until completion. It may be less
    // efficient and can starve other tasks on the current-thread runtime, but
    // it's a pragmatic compatibility fallback for tests and callers that
    // require a synchronous wrapper.
    futures::executor::block_on(future)
}

/// Block on a future on a background thread via the shared runtime.
///
/// Use when `block_in_place` is disallowed. The future and output must
/// be `Send + 'static` for cross-thread transfer.
pub fn block_on_shared_send<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    // If no runtime is active, run directly on the shared runtime.
    if tokio::runtime::Handle::try_current().is_err() {
        return SHARED_RUNTIME.block_on(future);
    }

    // We're inside a runtime which may not permit blocking. Spawn a
    // dedicated thread that runs the future on the SHARED_RUNTIME and
    // return the result via a channel. This avoids panics caused by
    // nested runtimes or disallowed block_in_place.
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let res = SHARED_RUNTIME.block_on(future);
        // Ignore send errors (receiver dropped) as the caller will panic or
        // be unwinding in that case.
        let _ = tx.send(res);
    });

    match rx.recv() {
        Ok(v) => v,
        Err(e) => {
            panic!("block_on_shared_send: background thread panicked or sender disconnected: {e}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_runtime_is_initialized() {
        // Verify the runtime was created and has the expected thread count
        let worker_count = SHARED_RUNTIME.metrics().num_workers();
        assert!(
            worker_count > 0,
            "runtime should have at least one worker thread"
        );
        assert_eq!(worker_count, num_cpus::get().min(4));
    }

    #[test]
    fn test_spawn_on_shared() {
        let handle = spawn_on_shared(async { 42 });
        let result = SHARED_RUNTIME
            .block_on(handle)
            .expect("task should complete");
        assert_eq!(result, 42);
    }

    #[test]
    fn test_block_on_shared_from_outside_runtime() {
        let result = block_on_shared(async { "hello" });
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_block_on_shared_send_from_outside_runtime() {
        let result = block_on_shared_send(async { 99_usize });
        assert_eq!(result, 99);
    }

    #[test]
    fn test_spawn_on_shared_concurrent() {
        let handles: Vec<_> = (0..10)
            .map(|i| spawn_on_shared(async move { i * 2 }))
            .collect();

        let results: Vec<_> = handles
            .into_iter()
            .map(|h| SHARED_RUNTIME.block_on(h).expect("task should complete"))
            .collect();

        assert_eq!(results, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
    }

    #[tokio::test]
    async fn test_block_on_shared_from_inside_runtime() {
        let result = block_on_shared(async { 123 });
        assert_eq!(result, 123);
    }

    #[tokio::test]
    async fn test_block_on_shared_send_from_inside_runtime() {
        let result = block_on_shared_send(async { 456_usize });
        assert_eq!(result, 456);
    }

    #[test]
    fn test_spawn_captures_data() {
        let data = [1, 2, 3];
        let handle = spawn_on_shared(async move { data.len() });
        let result = SHARED_RUNTIME
            .block_on(handle)
            .expect("task should complete");
        assert_eq!(result, 3);
    }

    // --- Shared runtime metrics ---

    #[test]
    fn test_runtime_metrics_alive() {
        let metrics = SHARED_RUNTIME.metrics();
        // Multi-thread runtime should report num_workers > 0
        assert!(metrics.num_workers() > 0);
        // num_alive_tasks is a usize, always >= 0; just verify it's accessible
        let _ = metrics.num_alive_tasks();
    }

    // --- spawn_on_shared error handling ---

    #[test]
    fn test_spawn_on_shared_panic_is_caught() {
        let handle = spawn_on_shared(async {
            panic!("intentional test panic");
        });
        let result = SHARED_RUNTIME.block_on(handle);
        assert!(result.is_err(), "panicked task should return JoinError");
    }

    // --- block_on_shared with async state ---

    #[test]
    fn test_block_on_shared_async_computation() {
        let result = block_on_shared(async {
            let mut sum = 0u64;
            for i in 0..100 {
                sum += i;
            }
            sum
        });
        assert_eq!(result, 4950);
    }

    #[test]
    fn test_block_on_shared_send_string() {
        let result = block_on_shared_send(async {
            let s = String::from("hello shared runtime");
            s.to_uppercase()
        });
        assert_eq!(result, "HELLO SHARED RUNTIME");
    }

    // --- Concurrent spawning ---

    #[test]
    fn test_spawn_many_concurrent_tasks() {
        let handles: Vec<_> = (0..50).map(|i| spawn_on_shared(async move { i })).collect();

        let sum: i32 = handles
            .into_iter()
            .map(|h| SHARED_RUNTIME.block_on(h).expect("task should complete"))
            .sum();

        assert_eq!(sum, (0..50).sum());
    }

    // --- block_on_shared with tokio features ---

    #[test]
    fn test_block_on_shared_with_tokio_sleep() {
        let result = block_on_shared(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            42
        });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_spawn_on_shared_with_tokio_spawn() {
        let handle = spawn_on_shared(async {
            let inner = tokio::spawn(async { 99 });
            inner.await.expect("inner task should complete")
        });
        let result = SHARED_RUNTIME
            .block_on(handle)
            .expect("task should complete");
        assert_eq!(result, 99);
    }

    // --- block_on_shared_send with large data ---

    #[test]
    fn test_block_on_shared_send_large_vec() {
        let result = block_on_shared_send(async {
            let v: Vec<u64> = (0..10_000).collect();
            v.len()
        });
        assert_eq!(result, 10_000);
    }

    // --- Nested async ---

    #[test]
    fn test_block_on_shared_nested_async() {
        let result = block_on_shared(async {
            let inner = block_on_shared(async { 7 });
            inner * 6
        });
        assert_eq!(result, 42);
    }
}
