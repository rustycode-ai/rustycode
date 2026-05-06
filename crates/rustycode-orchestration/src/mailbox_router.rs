//! MailboxRouter — directed inter-agent message routing.
//!
//! Provides point-to-point and broadcast message delivery between registered
//! agents using tokio MPSC channels. Each send/broadcast emits an
//! `OrchestrationEvent` for observability via the existing bus.

use crate::bus::{BusHandle, OrchestrationEvent};
use rustycode_protocol::agent_protocol::{AgentMessage, AgentPayload, AgentRole};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Capacity of each agent's message queue.
const DEFAULT_MAILBOX_CAPACITY: usize = 64;

/// Handle to a mailbox for receiving messages.
pub type Mailbox = mpsc::Receiver<AgentMessage<AgentPayload>>;

/// Sender half of a mailbox.
pub type MailboxTx = mpsc::Sender<AgentMessage<AgentPayload>>;

/// Error type for mailbox operations.
#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    /// Target agent is not registered.
    #[error("agent not registered: {0}")]
    AgentNotRegistered(String),
    /// Target agent's mailbox is full.
    #[error("agent mailbox full: {0}")]
    MailboxFull(String),
    /// Send channel closed for agent.
    #[error("send channel closed for agent: {0}")]
    ChannelClosed(String),
}

/// Thin directed routing layer for inter-agent messages.
///
/// Each agent registers to get a receiving end of an MPSC channel.
/// `send()` delivers to a specific agent's inbox.
/// `broadcast()` delivers to all registered agents except the sender.
/// Every operation emits a bus event for observability.
#[derive(Debug, Clone)]
pub struct MailboxRouter {
    /// Map of agent_id to sender handle.
    inboxes: Arc<Mutex<HashMap<String, MailboxTx>>>,
    /// Bus handle for publishing observability events.
    bus: BusHandle,
    /// Max queued messages per agent.
    capacity: usize,
}

impl MailboxRouter {
    /// Create a new router backed by the given bus for observability.
    pub fn new(bus: BusHandle) -> Self {
        Self::with_capacity(bus, DEFAULT_MAILBOX_CAPACITY)
    }

    /// Create a new router with specified per-agent queue capacity.
    pub fn with_capacity(bus: BusHandle, capacity: usize) -> Self {
        Self {
            inboxes: Arc::new(Mutex::new(HashMap::new())),
            bus,
            capacity,
        }
    }

    /// Register a new agent and return its message receiver.
    ///
    /// If the agent is already registered, the old mailbox is dropped and replaced.
    pub async fn register(&self, agent_id: impl Into<String>) -> Mailbox {
        let id = agent_id.into();
        let (tx, rx) = mpsc::channel(self.capacity);
        self.inboxes.lock().await.insert(id, tx);
        rx
    }

    /// Unregister an agent, preventing further message delivery.
    ///
    /// Returns `true` if the agent was registered.
    pub async fn unregister(&self, agent_id: &str) -> bool {
        self.inboxes.lock().await.remove(agent_id).is_some()
    }

    /// Send a directed message to a specific agent using the message's `to` role.
    ///
    /// Extracts the kind string before sending to avoid consuming the message
    /// before the bus event is published.
    pub async fn send(&self, message: AgentMessage<AgentPayload>) -> Result<(), MailboxError> {
        let recipient = match &message.to {
            Some(role) => role.to_string(),
            None => {
                return Err(MailboxError::AgentNotRegistered("no recipient".into()));
            }
        };

        let kind = payload_kind_str(&message.payload);
        let from_str = message.from.to_string();

        let inboxes = self.inboxes.lock().await;
        let sender = inboxes
            .get(&recipient)
            .ok_or_else(|| MailboxError::AgentNotRegistered(recipient.clone()))?;

        sender
            .send(message)
            .await
            .map_err(|_| MailboxError::ChannelClosed(recipient.clone()))?;

        drop(inboxes);

        self.bus.publish(OrchestrationEvent::MessageRouted {
            from: from_str,
            to: recipient,
            kind,
        });

        Ok(())
    }

    /// Send a directed message to a specific agent by string ID.
    ///
    /// More flexible than `send()` — routes by string ID rather than `AgentRole`.
    pub async fn send_to(
        &self,
        from_id: &str,
        recipient_id: &str,
        message: AgentMessage<AgentPayload>,
    ) -> Result<(), MailboxError> {
        let kind = payload_kind_str(&message.payload);

        let inboxes = self.inboxes.lock().await;
        let sender = inboxes
            .get(recipient_id)
            .ok_or_else(|| MailboxError::AgentNotRegistered(recipient_id.to_string()))?;

        sender
            .send(message)
            .await
            .map_err(|_| MailboxError::ChannelClosed(recipient_id.to_string()))?;

        drop(inboxes);

        self.bus.publish(OrchestrationEvent::MessageRouted {
            from: from_id.to_string(),
            to: recipient_id.to_string(),
            kind,
        });

        Ok(())
    }

    /// Broadcast a payload to all registered agents except the sender.
    ///
    /// Returns a vector of results for each delivery attempt.
    pub async fn broadcast(
        &self,
        from: &str,
        payload: AgentPayload,
    ) -> Vec<Result<(), MailboxError>> {
        let kind = payload_kind_str(&payload);
        let inboxes = self.inboxes.lock().await;
        let recipient_count = inboxes.len().saturating_sub(1);

        let mut results = Vec::new();
        for (agent_id, sender) in inboxes.iter() {
            if agent_id == from {
                continue;
            }
            let msg = AgentMessage::new(AgentRole::Coordinator, payload.clone());
            match sender.send(msg).await {
                Ok(()) => results.push(Ok(())),
                Err(_) => {
                    results.push(Err(MailboxError::ChannelClosed(agent_id.clone())));
                }
            }
        }

        drop(inboxes);
        self.bus.publish(OrchestrationEvent::MessageBroadcast {
            from: from.to_string(),
            recipient_count,
            kind,
        });

        results
    }

    /// Check if an agent is registered.
    pub async fn is_registered(&self, agent_id: &str) -> bool {
        self.inboxes.lock().await.contains_key(agent_id)
    }

    /// List all registered agent IDs.
    pub async fn list_agents(&self) -> Vec<String> {
        self.inboxes.lock().await.keys().cloned().collect()
    }

    /// Get the count of registered agents.
    pub async fn agent_count(&self) -> usize {
        self.inboxes.lock().await.len()
    }
}

/// Extract a human-readable kind string from an `AgentPayload`.
fn payload_kind_str(payload: &AgentPayload) -> String {
    match payload {
        AgentPayload::TaskDelegation { .. } => "task_delegation".to_string(),
        AgentPayload::TaskResult { .. } => "task_result".to_string(),
        AgentPayload::CapabilityAdvertise { .. } => "capability_advertise".to_string(),
        AgentPayload::CapabilityQuery { .. } => "capability_query".to_string(),
        AgentPayload::CapabilityResponse { .. } => "capability_response".to_string(),
        AgentPayload::Objection { .. } => "objection".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_router() -> MailboxRouter {
        MailboxRouter::new(BusHandle::new(16))
    }

    #[tokio::test]
    async fn register_and_send_directed() {
        let router = test_router();
        let mut rx = router.register("agent-1").await;

        let msg = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-1",
            "fix the bug",
            AgentRole::Builder,
        );

        router.send_to("coordinator", "agent-1", msg).await.unwrap();

        let received = rx.try_recv().unwrap();
        match &received.payload {
            AgentPayload::TaskDelegation { task_id, .. } => assert_eq!(task_id, "task-1"),
            _ => panic!("expected TaskDelegation"),
        }
    }

    #[tokio::test]
    async fn send_to_unknown_recipient_errors() {
        let router = test_router();
        let msg = AgentMessage::task_result(
            AgentRole::Builder,
            AgentRole::Coordinator,
            "task-1",
            true,
            "done",
        );

        let result = router.send_to("builder", "unknown", msg).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MailboxError::AgentNotRegistered(id) => assert_eq!(id, "unknown"),
            other => panic!("expected AgentNotRegistered, got: {other}"),
        }
    }

    #[tokio::test]
    async fn broadcast_reaches_all_except_sender() {
        let router = test_router();
        let mut rx1 = router.register("agent-1").await;
        let mut rx2 = router.register("agent-2").await;
        let _rx3 = router.register("agent-3").await;

        let results = router
            .broadcast(
                "agent-1",
                AgentPayload::CapabilityQuery {
                    capability: "security".into(),
                },
            )
            .await;

        assert_eq!(results.len(), 2); // agent-2 and agent-3
        assert!(results.iter().all(|r| r.is_ok()));

        // agent-1 should NOT have received anything
        assert!(rx1.try_recv().is_err());

        // agent-2 should have received
        let msg = rx2.try_recv().unwrap();
        match &msg.payload {
            AgentPayload::CapabilityQuery { capability } => {
                assert_eq!(capability, "security");
            }
            _ => panic!("expected CapabilityQuery"),
        }
    }

    #[tokio::test]
    async fn unregister_prevents_delivery() {
        let router = test_router();
        let mut rx = router.register("agent-1").await;

        router.unregister("agent-1").await;

        let msg = AgentMessage::task_result(
            AgentRole::Builder,
            AgentRole::Coordinator,
            "task-1",
            true,
            "done",
        );

        let result = router.send_to("builder", "agent-1", msg).await;
        assert!(result.is_err());

        // Receiver should still work but have nothing
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn registered_agents_listing() {
        let router = test_router();
        assert!(router.list_agents().await.is_empty());

        let _rx1 = router.register("a").await;
        let _rx2 = router.register("b").await;

        let mut agents = router.list_agents().await;
        agents.sort();
        assert_eq!(agents, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn is_registered_check() {
        let router = test_router();
        assert!(!router.is_registered("x").await);

        let _rx = router.register("x").await;
        assert!(router.is_registered("x").await);
    }

    #[tokio::test]
    async fn send_emits_message_routed_event() {
        let router = test_router();
        let mut bus_rx = router.bus.subscribe();
        let _rx = router.register("agent-1").await;

        let msg = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-99",
            "do work",
            AgentRole::Builder,
        );
        router.send_to("coordinator", "agent-1", msg).await.unwrap();

        let event = bus_rx.try_recv().unwrap();
        match event {
            OrchestrationEvent::MessageRouted { from, to, kind } => {
                assert_eq!(from, "coordinator");
                assert_eq!(to, "agent-1");
                assert_eq!(kind, "task_delegation");
            }
            other => panic!("expected MessageRouted, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn broadcast_emits_message_broadcast_event() {
        let router = test_router();
        let mut bus_rx = router.bus.subscribe();
        let _rx1 = router.register("agent-1").await;
        let _rx2 = router.register("agent-2").await;
        let _rx3 = router.register("agent-3").await;

        router
            .broadcast(
                "agent-1",
                AgentPayload::CapabilityQuery {
                    capability: "testing".into(),
                },
            )
            .await;

        let event = bus_rx.try_recv().unwrap();
        match event {
            OrchestrationEvent::MessageBroadcast {
                from,
                recipient_count,
                kind,
            } => {
                assert_eq!(from, "agent-1");
                assert_eq!(recipient_count, 2);
                assert_eq!(kind, "capability_query");
            }
            other => panic!("expected MessageBroadcast, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_via_role_errors_without_recipient() {
        let router = test_router();
        let msg = AgentMessage::new(
            AgentRole::Coordinator,
            AgentPayload::CapabilityQuery {
                capability: "test".into(),
            },
        );
        // No `to` role set, so send() should fail
        let result = router.send(msg).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MailboxError::AgentNotRegistered(msg) => assert_eq!(msg, "no recipient"),
            other => panic!("expected AgentNotRegistered, got: {other}"),
        }
    }

    #[tokio::test]
    async fn send_via_role_succeeds_with_registered_role_name() {
        let router = test_router();
        let _rx = router.register("Builder").await;

        let msg = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-42",
            "implement feature",
            AgentRole::Builder,
        );

        router.send(msg).await.unwrap();
    }

    #[tokio::test]
    async fn payload_kind_str_all_variants() {
        assert_eq!(
            payload_kind_str(&AgentPayload::TaskDelegation {
                task_id: "t".into(),
                prompt: "p".into(),
                role: AgentRole::Worker,
            }),
            "task_delegation"
        );
        assert_eq!(
            payload_kind_str(&AgentPayload::TaskResult {
                task_id: "t".into(),
                success: true,
                output: "o".into(),
            }),
            "task_result"
        );
        assert_eq!(
            payload_kind_str(&AgentPayload::CapabilityAdvertise {
                agent_id: "a".into(),
                capabilities: vec![],
            }),
            "capability_advertise"
        );
        assert_eq!(
            payload_kind_str(&AgentPayload::CapabilityQuery {
                capability: "c".into(),
            }),
            "capability_query"
        );
        assert_eq!(
            payload_kind_str(&AgentPayload::CapabilityResponse { agents: vec![] }),
            "capability_response"
        );
        assert_eq!(
            payload_kind_str(&AgentPayload::Objection {
                reason: "r".into(),
                evidence: "e".into(),
            }),
            "objection"
        );
    }
}
