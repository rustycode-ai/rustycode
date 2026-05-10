use crate::mailbox_router::MailboxRouter;
use rustycode_protocol::agent_protocol::{AgentMessage, AgentPayload, AgentRole};

/// Sync adapter for the tools-layer `MessageSender` trait.
///
/// Keeps `MailboxRouter` async-only and isolates the sync/async bridge to the
/// tool execution boundary where `send_message` is invoked.
#[derive(Debug, Clone)]
pub struct MailboxSender {
    router: MailboxRouter,
}

impl MailboxSender {
    pub fn new(router: MailboxRouter) -> Self {
        Self { router }
    }
}

impl rustycode_tools_api::MessageSender for MailboxSender {
    fn send(&self, to: &str, message: &str, _summary: &str) -> Result<(), String> {
        let payload = AgentPayload::TaskDelegation {
            task_id: format!("msg-{}", uuid::Uuid::new_v4()),
            prompt: message.to_string(),
            role: AgentRole::Coordinator,
        };
        let msg = AgentMessage::new(AgentRole::Coordinator, payload);
        let inner = run_sync(self.router.send_to("coordinator", to, msg))?;
        inner.map_err(|e| e.to_string())
    }

    fn broadcast(&self, message: &str, _summary: &str) -> Result<(), String> {
        let payload = AgentPayload::CapabilityAdvertise {
            agent_id: "coordinator".to_string(),
            capabilities: vec![message.to_string()],
        };
        let results = run_sync(self.router.broadcast("coordinator", payload))?;
        let failures: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!("broadcast had {} failures", failures.len()))
        }
    }
}

fn run_sync<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // Use block_in_place to avoid panicking when called from within a
        // multi-threaded tokio runtime (e.g. during orchestration).
        Ok(tokio::task::block_in_place(|| handle.block_on(future)))
    } else {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("runtime error: {e}"))?;
        Ok(runtime.block_on(future))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::BusHandle;
    use std::sync::mpsc;

    #[test]
    fn adapter_delivers_direct_message() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        let runtime_thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async move {
                let router = MailboxRouter::new(BusHandle::new(16));
                let sender = MailboxSender::new(router.clone());
                let mut rx = router.register("builder-1").await;
                ready_tx.send(sender).expect("ready send");

                let received = rx.recv().await.expect("received message");
                match &received.payload {
                    AgentPayload::TaskDelegation { prompt, .. } => {
                        assert_eq!(prompt, "fix the auth bug in login.rs");
                    }
                    other => panic!("expected TaskDelegation, got: {other:?}"),
                }

                done_tx.send(()).expect("done send");
            });
        });

        let sender = ready_rx.recv().expect("ready recv");
        let sync_thread = std::thread::spawn(move || {
            rustycode_tools_api::MessageSender::send(
                &sender,
                "builder-1",
                "fix the auth bug in login.rs",
                "delegated fix task",
            )
            .expect("send");
        });

        sync_thread.join().expect("sync thread join");
        done_rx.recv().expect("done recv");
        runtime_thread.join().expect("runtime thread join");
    }

    #[test]
    fn adapter_errors_for_unknown_agent() {
        let thread = std::thread::spawn(|| {
            let router = MailboxRouter::new(BusHandle::new(16));
            let sender = MailboxSender::new(router);
            let result =
                rustycode_tools_api::MessageSender::send(&sender, "ghost", "hello", "greeting");
            assert!(result.is_err());
        });

        thread.join().expect("thread join");
    }
}
