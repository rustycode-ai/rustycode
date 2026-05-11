//! Shared model list caching with async background refresh.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::provider::ProviderError;

/// Cache TTL for model lists fetched from provider APIs.
#[allow(clippy::duration_subsec)]
const CACHE_TTL: Duration = Duration::from_mins(5);

/// Thread-safe cached model list with background refresh support.
pub struct ModelCache {
    state: Arc<Mutex<Option<(Instant, Vec<String>)>>>,
}

impl ModelCache {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(None)),
        }
    }

    /// Return cached models if fresh, otherwise `None`.
    pub fn get_cached(&self) -> Option<Vec<String>> {
        let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().and_then(|(fetched_at, models)| {
            if fetched_at.elapsed() < CACHE_TTL {
                Some(models.clone())
            } else {
                None
            }
        })
    }

    /// Store freshly fetched models in the cache.
    pub fn store(&self, models: Vec<String>) {
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some((Instant::now(), models));
    }

    /// Fetch models from a provider API, caching on success.
    /// Returns the fetched models or the previous cache on failure.
    pub async fn fetch_or_fallback<F, Fut>(
        &self,
        fallback: &[&str],
        fetch_fn: F,
    ) -> Result<Vec<String>, ProviderError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Vec<String>, ProviderError>>,
    {
        // Return fresh cache immediately
        if let Some(models) = self.get_cached() {
            return Ok(models);
        }

        // Try fetching from API
        match fetch_fn().await {
            Ok(models) if !models.is_empty() => {
                self.store(models.clone());
                Ok(models)
            }
            Ok(_) => {
                // Empty response — use stale cache or fallback
                let stale = {
                    let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    guard.as_ref().map(|(_, m)| m.clone())
                };
                Ok(stale.unwrap_or_else(|| fallback.iter().map(|s| s.to_string()).collect()))
            }
            Err(_) => {
                // Return stale cache or fallback on failure
                let stale = {
                    let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    guard.as_ref().map(|(_, m)| m.clone())
                };
                Ok(stale.unwrap_or_else(|| fallback.iter().map(|s| s.to_string()).collect()))
            }
        }
    }
}

impl Default for ModelCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cache_returns_none() {
        let cache = ModelCache::new();
        assert!(cache.get_cached().is_none());
    }

    #[test]
    fn store_and_retrieve() {
        let cache = ModelCache::new();
        cache.store(vec!["model-a".to_string(), "model-b".to_string()]);
        let cached = cache.get_cached().unwrap();
        assert_eq!(cached, vec!["model-a", "model-b"]);
    }

    #[test]
    fn expired_cache_returns_none() {
        let cache = ModelCache::new();
        // Manually insert with past timestamp
        {
            let mut guard = cache.state.lock().unwrap();
            *guard = Some((
                Instant::now() - Duration::from_mins(10),
                vec!["old-model".to_string()],
            ));
        }
        assert!(cache.get_cached().is_none());
    }

    #[tokio::test]
    async fn fetch_or_fallback_returns_fetched() {
        let cache = ModelCache::new();
        let result = cache
            .fetch_or_fallback(&["fallback"], || async {
                Ok(vec!["fetched-model".to_string()])
            })
            .await
            .unwrap();
        assert_eq!(result, vec!["fetched-model"]);
        // Now cached
        assert_eq!(
            cache.get_cached().unwrap(),
            vec!["fetched-model".to_string()]
        );
    }

    #[tokio::test]
    async fn fetch_or_fallback_returns_fallback_on_error() {
        let cache = ModelCache::new();
        let result = cache
            .fetch_or_fallback(&["fallback-model"], || async {
                Err(ProviderError::Network("timeout".to_string()))
            })
            .await
            .unwrap();
        assert_eq!(result, vec!["fallback-model"]);
    }

    #[tokio::test]
    async fn fetch_or_fallback_returns_stale_on_error() {
        let cache = ModelCache::new();
        cache.store(vec!["stale-model".to_string()]);
        // Expire the cache
        {
            let mut guard = cache.state.lock().unwrap();
            if let Some((_, ref models)) = *guard {
                *guard = Some((Instant::now() - Duration::from_mins(10), models.clone()));
            }
        }
        let result = cache
            .fetch_or_fallback(&["fallback"], || async {
                Err(ProviderError::Network("timeout".to_string()))
            })
            .await
            .unwrap();
        // Stale cache preferred over fallback
        assert_eq!(result, vec!["stale-model"]);
    }

    #[tokio::test]
    async fn fetch_or_fallback_returns_fallback_on_empty_response() {
        let cache = ModelCache::new();
        let result = cache
            .fetch_or_fallback(&["fallback-model"], || async { Ok(vec![]) })
            .await
            .unwrap();
        assert_eq!(result, vec!["fallback-model"]);
    }
}
