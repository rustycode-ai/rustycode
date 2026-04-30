use crate::bus::{BusHandle, OrchestrationEvent};

pub struct Skeptic {
    bus: BusHandle,
}

impl Skeptic {
    pub const fn new(bus: BusHandle) -> Self {
        Self { bus }
    }

    pub fn start_monitoring(&self) {
        let bus = self.bus.clone();
        let rx = bus.subscribe();
        let _ = tokio::runtime::Handle::try_current().map(|handle| {
            handle.spawn(async move {
                let mut rx = rx;
                while let Ok(event) = rx.recv().await {
                    if let OrchestrationEvent::PartialResult {
                        step_id, content, ..
                    } = event
                    {
                        if content.contains("ERROR:") || content.contains("PANIC:") {
                            tracing::warn!(step_id = %step_id, "Skeptic found concerning output");
                            bus.publish(OrchestrationEvent::Objection {
                                step_id,
                                reason:
                                    "Skeptic rejected partial result due to critical error pattern"
                                        .into(),
                            });
                        }
                    }
                }
            });
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_skeptic_creation() {
        let bus = BusHandle::new(16);
        let _skeptic = Skeptic::new(bus);
    }

    #[test]
    fn test_skeptic_start_monitoring_no_runtime() {
        let bus = BusHandle::new(16);
        let skeptic = Skeptic::new(bus);
        skeptic.start_monitoring();
    }

    #[test]
    fn test_skeptic_new_is_const() {
        let bus = BusHandle::new(8);
        let _skeptic = Skeptic::new(bus);
    }

    #[tokio::test]
    async fn test_skeptic_publishes_objection_on_error() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();

        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();

        // Give skeptic's spawned task time to start
        tokio::task::yield_now().await;

        bus.publish(OrchestrationEvent::PartialResult {
            step_id: "s1".into(),
            content: "ERROR: something broke".into(),
        });

        // Give skeptic time to process and publish objection
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // First event is the PartialResult we published
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::PartialResult { .. }));

        // Second should be the Objection from skeptic
        let objection = rx.try_recv().unwrap();
        assert!(matches!(objection, OrchestrationEvent::Objection { .. }));
    }

    #[tokio::test]
    async fn test_skeptic_ignores_normal_output() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();
        tokio::task::yield_now().await;

        bus.publish(OrchestrationEvent::PartialResult {
            step_id: "s1".into(),
            content: "All good, no errors here".into(),
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Should only get the PartialResult, no Objection
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::PartialResult { .. }));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_skeptic_flags_panic_pattern() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();
        tokio::task::yield_now().await;

        bus.publish(OrchestrationEvent::PartialResult {
            step_id: "s2".into(),
            content: "PANIC: unrecoverable error".into(),
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let _partial = rx.try_recv().unwrap();
        let objection = rx.try_recv().unwrap();
        assert!(
            matches!(objection, OrchestrationEvent::Objection { step_id, .. } if step_id == "s2")
        );
    }
}
