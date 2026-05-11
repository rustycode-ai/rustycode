use super::config::AnalyticsConfig;
use super::events::{AnalyticsEvent, EventContext, Ga4Payload};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;

/// Minimum interval between GA4 HTTP requests.
const SEND_INTERVAL: Duration = Duration::from_millis(200);

/// Channel capacity for queued events.
const CHANNEL_CAPACITY: usize = 256;

/// Maximum events batched into a single GA4 request.
const MAX_BATCH_SIZE: usize = 10;

/// Handle to the analytics background worker.
/// Cheaply cloneable and thread-safe.
/// Drop to shut down the worker.
#[derive(Clone, Debug)]
pub struct AnalyticsClient {
    tx: mpsc::Sender<AnalyticsEvent>,
}

/// Create a new analytics client.
///
/// Returns `None` if analytics is disabled or misconfigured.
/// All sends are non-blocking — if the channel is full, events are dropped.
pub fn create_client(config: &AnalyticsConfig, context: EventContext) -> Option<AnalyticsClient> {
    if !config.can_send() {
        tracing::debug!("analytics disabled or misconfigured, skipping client creation");
        return None;
    }

    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

    let measurement_id = config.measurement_id.clone();
    let api_secret = config.api_secret.clone();
    let endpoint = config.active_endpoint().to_string();
    let client_id = context.install_id.clone();

    // Spawn background worker
    tokio::spawn(async move {
        worker(rx, client_id, measurement_id, api_secret, endpoint, context).await;
    });

    Some(AnalyticsClient { tx })
}

impl AnalyticsClient {
    /// Send an analytics event. Non-blocking; drops the event if the channel is full.
    pub fn send(&self, event: AnalyticsEvent) {
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::debug!("analytics channel full, dropping event");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!("analytics channel closed, dropping event");
            }
        }
    }

    /// Convenience: create an enriched event and send it.
    /// No-op if client is None.
    pub fn send_enriched(this: &Option<Self>, ctx: &EventContext, event: AnalyticsEvent) {
        if let Some(client) = this {
            client.send(ctx.enrich(event));
        }
    }
}

async fn worker(
    mut rx: mpsc::Receiver<AnalyticsEvent>,
    client_id: String,
    measurement_id: String,
    api_secret: String,
    endpoint: String,
    context: EventContext,
) {
    let mut batch: Vec<AnalyticsEvent> = Vec::with_capacity(MAX_BATCH_SIZE);
    let last_send_ms = Arc::new(AtomicU64::new(0));

    loop {
        // Wait for next event
        let Some(event) = rx.recv().await else {
            // Channel closed — flush remaining and exit
            if !batch.is_empty() {
                flush_batch(batch, &client_id, &measurement_id, &api_secret, &endpoint).await;
            }
            return;
        };

        let enriched = context.enrich(event);
        batch.push(enriched);

        // Drain any additional queued events up to batch size
        while batch.len() < MAX_BATCH_SIZE {
            match rx.try_recv() {
                Ok(event) => {
                    let enriched = context.enrich(event);
                    batch.push(enriched);
                }
                Err(_) => break,
            }
        }

        // Rate limit
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let last = last_send_ms.load(Ordering::Relaxed);
        if last != 0 && now_ms.saturating_sub(last) < SEND_INTERVAL.as_millis() as u64 {
            tokio::time::sleep(SEND_INTERVAL).await;
        }

        // Send batch
        flush_batch(batch, &client_id, &measurement_id, &api_secret, &endpoint).await;
        last_send_ms.store(now_ms, Ordering::Relaxed);
        batch = Vec::with_capacity(MAX_BATCH_SIZE);
    }
}

async fn flush_batch(
    events: Vec<AnalyticsEvent>,
    client_id: &str,
    measurement_id: &str,
    api_secret: &str,
    endpoint: &str,
) {
    if events.is_empty() {
        return;
    }

    // Clone strings for the 'static spawned task
    let client_id = client_id.to_string();
    let measurement_id = measurement_id.to_string();
    let api_secret = api_secret.to_string();
    let endpoint = endpoint.to_string();

    let payload = Ga4Payload::new(client_id, events);

    // Fire-and-forget: any failure is silently logged, never propagated
    let result = tokio::task::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("analytics: failed to build HTTP client: {e}");
                return;
            }
        };

        let url = format!(
            "{}?measurement_id={}&api_secret={}",
            endpoint, measurement_id, api_secret
        );

        match client.post(&url).json(&payload).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    tracing::debug!("analytics: GA4 returned status {}", resp.status());
                }
            }
            Err(e) => {
                tracing::debug!("analytics: send failed: {e}");
            }
        }
    })
    .await;

    if let Err(e) = result {
        tracing::debug!("analytics: task join error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AnalyticsConfig {
        AnalyticsConfig {
            enabled: true,
            measurement_id: "G-TEST123".into(),
            api_secret: "secret123".into(),
            endpoint: "https://httpbin.org/post".into(),
            debug: false,
        }
    }

    fn test_context() -> EventContext {
        EventContext::new("test-install-id".into(), "test-session".into())
    }

    #[tokio::test]
    async fn client_sends_without_blocking() {
        let config = test_config();
        let ctx = test_context();
        let client = create_client(&config, ctx).unwrap();

        for i in 0..20 {
            client.send(AnalyticsEvent::new(format!("test_event_{i}")));
        }

        drop(client);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    #[tokio::test]
    async fn returns_none_when_disabled() {
        let config = AnalyticsConfig {
            enabled: false,
            ..test_config()
        };
        let ctx = test_context();
        assert!(create_client(&config, ctx).is_none());
    }

    #[tokio::test]
    async fn returns_none_when_no_secret() {
        let config = AnalyticsConfig {
            api_secret: String::new(),
            ..test_config()
        };
        let ctx = test_context();
        assert!(create_client(&config, ctx).is_none());
    }

    #[tokio::test]
    async fn drops_events_when_channel_full() {
        let config = test_config();
        let ctx = test_context();
        let client = create_client(&config, ctx).unwrap();

        for i in 0..CHANNEL_CAPACITY + 50 {
            client.send(AnalyticsEvent::new(format!("flood_{i}")));
        }

        drop(client);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    #[tokio::test]
    async fn send_enriched_works() {
        let config = test_config();
        let ctx = test_context();
        let client = create_client(&config, ctx.clone()).unwrap();

        let opt_client: Option<AnalyticsClient> = Some(client);
        AnalyticsClient::send_enriched(&opt_client, &ctx, AnalyticsEvent::new("enriched_test"));

        let none_client: Option<AnalyticsClient> = None;
        AnalyticsClient::send_enriched(&none_client, &ctx, AnalyticsEvent::new("no_op"));

        if let Some(c) = opt_client {
            drop(c);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
