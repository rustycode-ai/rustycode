//! Unified agent architecture integration tests.
//!
//! Exercises the full inter-agent messaging flow through:
//! - MailboxRouter: directed routing, broadcast, registration lifecycle
//! - AgentPayload: task delegation, results, capability discovery, objections
//! - DelegationToken: depth enforcement and child spawning limits
//! - AgentRegistry: capability lookup, availability filtering, success ranking
//! - AgentRole conversions: TeamRole->AgentRole, TryFrom<TaskRole>->AgentRole
//!
//! No LLM providers are mocked. This tests plumbing only.

#![allow(
    clippy::unwrap_used,
    clippy::redundant_clone,
    clippy::uninlined_format_args,
    clippy::items_after_statements
)]

use rustycode_orchestration::agent_registry::{
    AgentRegistry, CapabilityDescriptor, SpecialistAgent, SpecialistType,
};
use rustycode_orchestration::bus::BusHandle;
use rustycode_orchestration::delegation::{DelegationToken, TaskRole};
use rustycode_orchestration::mailbox_router::MailboxRouter;
use rustycode_orchestration::MailboxError;
use rustycode_protocol::agent_protocol::{AgentMessage, AgentPayload, AgentRole};
use rustycode_protocol::team::TeamRole;

// ---------------------------------------------------------------------------
// 1. Register agents via MailboxRouter (coordinator + builder)
// ---------------------------------------------------------------------------

mod registration {
    use super::*;

    #[tokio::test]
    async fn register_multiple_agents_and_list() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let _coord_rx = router.register("coordinator").await;
        let _builder_rx = router.register("builder").await;

        assert_eq!(router.agent_count().await, 2);
        assert!(router.is_registered("coordinator").await);
        assert!(router.is_registered("builder").await);
        assert!(!router.is_registered("unknown").await);

        let mut agents = router.list_agents().await;
        agents.sort();
        assert_eq!(agents, vec!["builder", "coordinator"]);
    }

    #[tokio::test]
    async fn unregister_removes_agent_from_routing() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let _rx = router.register("temp-agent").await;
        assert!(router.is_registered("temp-agent").await);

        let was_registered = router.unregister("temp-agent").await;
        assert!(was_registered);
        assert!(!router.is_registered("temp-agent").await);

        // Unregistering again returns false
        let was_still_registered = router.unregister("temp-agent").await;
        assert!(!was_still_registered);
    }

    #[tokio::test]
    async fn re_register_replaces_mailbox() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let mut rx1 = router.register("agent").await;

        // Send a message to the first mailbox
        let msg = AgentMessage::task_result(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "t-old",
            true,
            "old result",
        );
        router.send_to("coordinator", "agent", msg).await.unwrap();

        // Re-register replaces the mailbox
        let mut rx2 = router.register("agent").await;

        // Send a message to the new mailbox
        let msg = AgentMessage::task_result(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "t-new",
            true,
            "new result",
        );
        router.send_to("coordinator", "agent", msg).await.unwrap();

        // Old receiver still has the old message but won't get new ones
        let old = rx1.try_recv().unwrap();
        match &old.payload {
            AgentPayload::TaskResult { task_id, .. } => assert_eq!(task_id, "t-old"),
            other => panic!("expected TaskResult, got {other:?}"),
        }

        // New receiver has the new message
        let new = rx2.try_recv().unwrap();
        match &new.payload {
            AgentPayload::TaskResult { task_id, .. } => assert_eq!(task_id, "t-new"),
            other => panic!("expected TaskResult, got {other:?}"),
        }

        // Old receiver gets nothing further
        assert!(rx1.try_recv().is_err());
    }
}

// ---------------------------------------------------------------------------
// 2-5. Delegate task, builder receives, responds, coordinator gets result
// ---------------------------------------------------------------------------

mod task_delegation_flow {
    use super::*;

    #[tokio::test]
    async fn full_delegation_round_trip() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        // Register coordinator and builder
        let mut coord_rx = router.register("coordinator").await;
        let mut builder_rx = router.register("builder").await;

        // --- Step 2: Coordinator delegates a task to Builder ---
        let delegation_msg = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-implement-auth",
            "Implement JWT authentication for the login endpoint",
            AgentRole::Builder,
        );
        router
            .send_to("coordinator", "builder", delegation_msg)
            .await
            .unwrap();

        // --- Step 3: Builder receives and verifies correct payload ---
        let received = builder_rx.try_recv().unwrap();
        assert_eq!(received.from, AgentRole::Coordinator);
        assert_eq!(received.to, Some(AgentRole::Builder));
        match &received.payload {
            AgentPayload::TaskDelegation {
                task_id,
                prompt,
                role,
            } => {
                assert_eq!(task_id, "task-implement-auth");
                assert!(prompt.contains("JWT"));
                assert_eq!(*role, AgentRole::Builder);
            }
            other => panic!("expected TaskDelegation, got {other:?}"),
        }

        // --- Step 4: Builder responds with TaskResult ---
        let result_msg = AgentMessage::task_result(
            AgentRole::Builder,
            AgentRole::Coordinator,
            "task-implement-auth",
            true,
            "JWT auth implemented in src/auth.rs. All 12 tests passing.",
        );
        router
            .send_to("builder", "coordinator", result_msg)
            .await
            .unwrap();

        // --- Step 5: Coordinator receives the result ---
        let result_received = coord_rx.try_recv().unwrap();
        assert_eq!(result_received.from, AgentRole::Builder);
        assert_eq!(result_received.to, Some(AgentRole::Coordinator));
        match &result_received.payload {
            AgentPayload::TaskResult {
                task_id,
                success,
                output,
            } => {
                assert_eq!(task_id, "task-implement-auth");
                assert!(*success);
                assert!(output.contains("12 tests passing"));
            }
            other => panic!("expected TaskResult, got {other:?}"),
        }

        // Both mailboxes should now be empty
        assert!(builder_rx.try_recv().is_err());
        assert!(coord_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn send_to_unregistered_agent_returns_error() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let msg = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-x",
            "do something",
            AgentRole::Builder,
        );

        let result = router.send_to("coordinator", "ghost-agent", msg).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MailboxError::AgentNotRegistered(id) => assert_eq!(id, "ghost-agent"),
            other => panic!("expected AgentNotRegistered, got {other}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Broadcast test
// ---------------------------------------------------------------------------

mod broadcast_flow {
    use super::*;

    #[tokio::test]
    async fn coordinator_broadcast_reaches_all_others() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let mut coord_rx = router.register("coordinator").await;
        let mut builder_rx = router.register("builder").await;
        let mut skeptic_rx = router.register("skeptic").await;

        let payload = AgentPayload::CapabilityQuery {
            capability: "security_audit".into(),
        };

        let results = router.broadcast("coordinator", payload).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(std::result::Result::is_ok));

        // Coordinator should NOT receive its own broadcast
        assert!(coord_rx.try_recv().is_err());

        // Builder receives
        let builder_msg = builder_rx.try_recv().unwrap();
        match &builder_msg.payload {
            AgentPayload::CapabilityQuery { capability } => {
                assert_eq!(capability, "security_audit");
            }
            other => panic!("expected CapabilityQuery, got {other:?}"),
        }

        // Skeptic receives
        let skeptic_msg = skeptic_rx.try_recv().unwrap();
        match &skeptic_msg.payload {
            AgentPayload::CapabilityQuery { capability } => {
                assert_eq!(capability, "security_audit");
            }
            other => panic!("expected CapabilityQuery, got {other:?}"),
        }

        // No more messages
        assert!(builder_rx.try_recv().is_err());
        assert!(skeptic_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn broadcast_with_single_agent_delivers_to_none() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let _rx = router.register("lonely-agent").await;

        let results = router
            .broadcast(
                "lonely-agent",
                AgentPayload::CapabilityQuery {
                    capability: "anything".into(),
                },
            )
            .await;

        assert!(results.is_empty());
    }
}

// ---------------------------------------------------------------------------
// 7. Capability flow: advertise, query, response
// ---------------------------------------------------------------------------

mod capability_flow {
    use super::*;

    #[tokio::test]
    async fn capability_advertise_query_and_response() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let mut coord_rx = router.register("coordinator").await;
        let mut builder_rx = router.register("builder").await;

        // Builder advertises its capabilities (broadcast — no specific `to`)
        let advert = AgentMessage::capability_advertise(
            AgentRole::Builder,
            "builder",
            vec!["code_generation".into(), "file_editing".into()],
        );
        // For a broadcast-style advert, use send_to for a single directed
        // delivery since capability_advertise creates a broadcast-style message.
        // We explicitly direct it to coordinator for this test.
        let advert = advert.directed(AgentRole::Coordinator);
        router
            .send_to("builder", "coordinator", advert)
            .await
            .unwrap();

        // Coordinator receives the advertisement
        let recv = coord_rx.try_recv().unwrap();
        match &recv.payload {
            AgentPayload::CapabilityAdvertise {
                agent_id,
                capabilities,
            } => {
                assert_eq!(agent_id, "builder");
                assert_eq!(capabilities.len(), 2);
                assert!(capabilities.contains(&"code_generation".to_string()));
                assert!(capabilities.contains(&"file_editing".to_string()));
            }
            other => panic!("expected CapabilityAdvertise, got {other:?}"),
        }

        // Coordinator queries for an agent with code_generation capability
        let query = AgentMessage::capability_query(AgentRole::Coordinator, "code_generation");
        let query = query.directed(AgentRole::Builder);
        router
            .send_to("coordinator", "builder", query)
            .await
            .unwrap();

        // Builder receives the query
        let query_recv = builder_rx.try_recv().unwrap();
        match &query_recv.payload {
            AgentPayload::CapabilityQuery { capability } => {
                assert_eq!(capability, "code_generation");
            }
            other => panic!("expected CapabilityQuery, got {other:?}"),
        }

        // Builder responds with the list of capable agents
        let response =
            AgentMessage::capability_response(AgentRole::Builder, vec!["builder".into()]);
        let response = response.directed(AgentRole::Coordinator);
        router
            .send_to("builder", "coordinator", response)
            .await
            .unwrap();

        // Coordinator receives the response
        let resp_recv = coord_rx.try_recv().unwrap();
        match &resp_recv.payload {
            AgentPayload::CapabilityResponse { agents } => {
                assert_eq!(agents.len(), 1);
                assert_eq!(agents[0], "builder");
            }
            other => panic!("expected CapabilityResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn objection_payload_routing() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let mut coord_rx = router.register("coordinator").await;
        let _skeptic_rx = router.register("skeptic").await;

        // Skeptic raises an objection
        let objection = AgentMessage::objection(
            AgentRole::Skeptic,
            "untested_change",
            "No test coverage for the new auth module",
        );
        let objection = objection.directed(AgentRole::Coordinator);
        router
            .send_to("skeptic", "coordinator", objection)
            .await
            .unwrap();

        let recv = coord_rx.try_recv().unwrap();
        assert_eq!(recv.from, AgentRole::Skeptic);
        match &recv.payload {
            AgentPayload::Objection { reason, evidence } => {
                assert_eq!(reason, "untested_change");
                assert!(evidence.contains("auth module"));
            }
            other => panic!("expected Objection, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 8. Role conversion: TeamRole->AgentRole and TryFrom<TaskRole>->AgentRole
// ---------------------------------------------------------------------------

mod role_conversions {
    use super::*;

    #[test]
    fn team_role_to_agent_role_all_variants() {
        assert_eq!(AgentRole::from(TeamRole::Builder), AgentRole::Builder);
        assert_eq!(AgentRole::from(TeamRole::Skeptic), AgentRole::Skeptic);
        assert_eq!(AgentRole::from(TeamRole::Judge), AgentRole::Judge);
        assert_eq!(
            AgentRole::from(TeamRole::Coordinator),
            AgentRole::Coordinator
        );
        assert_eq!(AgentRole::from(TeamRole::Architect), AgentRole::Architect);
        assert_eq!(AgentRole::from(TeamRole::Scalpel), AgentRole::Scalpel);
    }

    #[test]
    fn task_role_try_into_agent_role_all_variants() {
        use std::convert::TryFrom;

        assert_eq!(
            AgentRole::try_from(TaskRole::Explore).unwrap(),
            AgentRole::Researcher
        );
        assert_eq!(
            AgentRole::try_from(TaskRole::Research).unwrap(),
            AgentRole::Researcher
        );
        assert_eq!(
            AgentRole::try_from(TaskRole::Code).unwrap(),
            AgentRole::Builder
        );
        assert_eq!(
            AgentRole::try_from(TaskRole::Review).unwrap(),
            AgentRole::Reviewer
        );
        assert_eq!(
            AgentRole::try_from(TaskRole::Verify).unwrap(),
            AgentRole::Judge
        );
        assert_eq!(
            AgentRole::try_from(TaskRole::Plan).unwrap(),
            AgentRole::Planner
        );
        assert_eq!(
            AgentRole::try_from(TaskRole::Debug).unwrap(),
            AgentRole::Scalpel
        );
    }

    #[test]
    fn agent_role_display_variants() {
        // Verify the Display impl produces human-readable names
        assert_eq!(format!("{}", AgentRole::Architect), "Architect");
        assert_eq!(format!("{}", AgentRole::Builder), "Builder");
        assert_eq!(format!("{}", AgentRole::Skeptic), "Skeptic");
        assert_eq!(format!("{}", AgentRole::Judge), "Judge");
        assert_eq!(format!("{}", AgentRole::Scalpel), "Scalpel");
        assert_eq!(format!("{}", AgentRole::Coordinator), "Coordinator");
        assert_eq!(format!("{}", AgentRole::Planner), "Planner");
        assert_eq!(format!("{}", AgentRole::Worker), "Worker");
        assert_eq!(format!("{}", AgentRole::Reviewer), "Reviewer");
        assert_eq!(format!("{}", AgentRole::Researcher), "Researcher");
    }
}

// ---------------------------------------------------------------------------
// 9. DelegationToken: root, child, depth enforcement
// ---------------------------------------------------------------------------

mod delegation_token {
    use super::*;

    #[test]
    fn root_token_can_delegate() {
        let token = DelegationToken::root("coordinator-1");
        assert_eq!(token.parent_agent_id, "coordinator-1");
        assert_eq!(token.depth, 0);
        assert_eq!(token.max_depth, 3);
        assert!(token.can_delegate());
    }

    #[test]
    fn child_token_increments_depth() {
        let root = DelegationToken::root("coordinator-1");
        let child = root.child("builder-1").unwrap();

        assert_eq!(child.parent_agent_id, "builder-1");
        assert_eq!(child.depth, 1);
        assert_eq!(child.max_depth, root.max_depth);
        assert!(child.can_delegate());
    }

    #[test]
    fn max_depth_blocks_further_delegation() {
        let root = DelegationToken::root("coordinator-1");

        let depth1 = root.child("builder-1").unwrap();
        assert_eq!(depth1.depth, 1);
        assert!(depth1.can_delegate());

        let depth2 = depth1.child("scalpel-1").unwrap();
        assert_eq!(depth2.depth, 2);
        // depth=2, max_depth=3, so depth+1=3 >= max_depth=3 => cannot delegate
        assert!(!depth2.can_delegate());

        // Attempting to create a child at depth 3 returns None
        assert!(depth2.child("unreachable").is_none());
    }

    #[test]
    fn child_inherits_constraints() {
        let mut root = DelegationToken::root("coordinator-1");
        root.allowed_tools = vec!["read_file".into(), "edit_file".into()];

        let child = root.child("builder-1").unwrap();
        assert_eq!(child.allowed_tools.len(), 2);
        assert!(child.allowed_tools.contains(&"read_file".to_string()));
        assert!(child.allowed_tools.contains(&"edit_file".to_string()));
    }

    #[test]
    fn delegation_chain_depth_tracking() {
        let root = DelegationToken::root("root-agent");
        assert_eq!(root.depth, 0);

        let gen1 = root.child("gen-1").unwrap();
        assert_eq!(gen1.depth, 1);

        let gen2 = gen1.child("gen-2").unwrap();
        assert_eq!(gen2.depth, 2);

        // Chain terminates at max_depth
        assert!(gen2.child("gen-3").is_none());
    }
}

// ---------------------------------------------------------------------------
// 10. Registry capability lookup
// ---------------------------------------------------------------------------

mod registry_capabilities {
    use super::*;

    fn make_specialist(
        name: &str,
        cap_name: &str,
        success_rate: f64,
        available: bool,
    ) -> SpecialistAgent {
        let mut agent = SpecialistAgent::new(
            name.to_string(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            None,
        );
        agent.available = available;
        agent.capabilities.push(CapabilityDescriptor {
            name: cap_name.to_string(),
            description: format!("{cap_name} capability"),
            available,
            success_rate,
            tool_scope: vec![],
        });
        agent
    }

    #[test]
    fn find_by_capability_returns_matching_agents() {
        let mut registry = AgentRegistry::new();

        let agent1 = make_specialist("SecAgent1", "security_audit", 0.85, true);
        let agent2 = make_specialist("SecAgent2", "security_audit", 0.92, true);
        let agent3 = make_specialist("DbAgent", "db_migration", 0.70, true);

        registry.generated.insert(agent1.id.clone(), agent1);
        registry.generated.insert(agent2.id.clone(), agent2);
        registry.generated.insert(agent3.id.clone(), agent3);

        let found = registry.find_by_capability("security_audit");
        assert_eq!(found.len(), 2);

        let db_found = registry.find_by_capability("db_migration");
        assert_eq!(db_found.len(), 1);

        let none_found = registry.find_by_capability("nonexistent");
        assert!(none_found.is_empty());
    }

    #[test]
    fn find_available_excludes_busy_agents() {
        let mut registry = AgentRegistry::new();

        let available_agent = make_specialist("Free", "security_audit", 0.80, true);
        let busy_agent = make_specialist("Busy", "security_audit", 0.90, false);

        registry
            .generated
            .insert(available_agent.id.clone(), available_agent);
        registry.generated.insert(busy_agent.id.clone(), busy_agent);

        let available = registry.find_available("security_audit");
        assert_eq!(available.len(), 1);
        assert!(available[0].available);
    }

    #[test]
    fn rank_by_success_orders_highest_first() {
        let mut registry = AgentRegistry::new();

        let low_agent = make_specialist("Low", "security_audit", 0.60, true);
        let mid_agent = make_specialist("Mid", "security_audit", 0.78, true);
        let high_agent = make_specialist("High", "security_audit", 0.95, true);

        registry.generated.insert(low_agent.id.clone(), low_agent);
        registry.generated.insert(mid_agent.id.clone(), mid_agent);
        registry.generated.insert(high_agent.id.clone(), high_agent);

        let ranked = registry.rank_by_success("security_audit");
        assert_eq!(ranked.len(), 3);

        // Verify descending order
        let rate0 = ranked[0].capabilities[0].success_rate;
        let rate1 = ranked[1].capabilities[0].success_rate;
        let rate2 = ranked[2].capabilities[0].success_rate;
        assert!(
            rate0 >= rate1,
            "first rate ({rate0}) should be >= second ({rate1})"
        );
        assert!(
            rate1 >= rate2,
            "second rate ({rate1}) should be >= third ({rate2})"
        );

        assert!((rate0 - 0.95).abs() < f64::EPSILON);
        assert!((rate1 - 0.78).abs() < f64::EPSILON);
        assert!((rate2 - 0.60).abs() < f64::EPSILON);
    }

    #[test]
    fn mark_busy_and_mark_available_round_trip() {
        let mut registry = AgentRegistry::new();

        let agent = make_specialist("ToggleAgent", "security_audit", 0.85, true);
        let id = agent.id.clone();
        registry.generated.insert(id.clone(), agent);

        assert!(registry.generated.get(&id).unwrap().available);

        registry.mark_busy(&id);
        assert!(!registry.generated.get(&id).unwrap().available);

        registry.mark_available(&id);
        assert!(registry.generated.get(&id).unwrap().available);
    }

    #[test]
    fn all_agents_includes_builtin_and_generated() {
        let mut registry = AgentRegistry::new();

        // Built-in agents are pre-populated
        let built_in_count = registry.all_agents().len();
        assert!(built_in_count >= 6); // Architect, Builder, Skeptic, Judge, Scalpel, Coordinator

        // Add a generated specialist
        let agent = make_specialist("GenAgent", "security_audit", 0.80, true);
        let id = agent.id.clone();
        registry.generated.insert(id, agent);

        let total = registry.all_agents().len();
        assert_eq!(total, built_in_count + 1);
    }
}

// ---------------------------------------------------------------------------
// Cross-cutting: bus observability events emitted during routing
// ---------------------------------------------------------------------------

mod bus_observability {
    use super::*;
    use rustycode_orchestration::bus::OrchestrationEvent;

    #[tokio::test]
    async fn send_to_emits_message_routed_event() {
        let bus = BusHandle::new(16);
        let mut bus_rx = bus.subscribe();
        let router = MailboxRouter::new(bus);

        let _rx = router.register("builder").await;

        let msg = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-bus-1",
            "do work",
            AgentRole::Builder,
        );
        router.send_to("coordinator", "builder", msg).await.unwrap();

        let event = bus_rx.try_recv().unwrap();
        match event {
            OrchestrationEvent::MessageRouted { from, to, kind } => {
                assert_eq!(from, "coordinator");
                assert_eq!(to, "builder");
                assert_eq!(kind, "task_delegation");
            }
            other => panic!("expected MessageRouted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn broadcast_emits_message_broadcast_event() {
        let bus = BusHandle::new(16);
        let mut bus_rx = bus.subscribe();
        let router = MailboxRouter::new(bus);

        let _rx1 = router.register("coordinator").await;
        let _rx2 = router.register("builder").await;
        let _rx3 = router.register("skeptic").await;

        router
            .broadcast(
                "coordinator",
                AgentPayload::CapabilityAdvertise {
                    agent_id: "coordinator".into(),
                    capabilities: vec!["orchestration".into()],
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
                assert_eq!(from, "coordinator");
                assert_eq!(recipient_count, 2);
                assert_eq!(kind, "capability_advertise");
            }
            other => panic!("expected MessageBroadcast, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// End-to-end combined scenario
// ---------------------------------------------------------------------------

mod e2e_scenario {
    use super::*;

    #[tokio::test]
    async fn coordinator_delegates_builder_responds_skeptic_objected() {
        let bus = BusHandle::new(16);
        let router = MailboxRouter::new(bus);

        let mut coord_rx = router.register("coordinator").await;
        let mut builder_rx = router.register("builder").await;
        let mut skeptic_rx = router.register("skeptic").await;

        // 1. Coordinator delegates to Builder
        let delegation = AgentMessage::delegation(
            AgentRole::Coordinator,
            AgentRole::Builder,
            "task-combined",
            "Implement input validation for user registration",
            AgentRole::Builder,
        );
        router
            .send_to("coordinator", "builder", delegation)
            .await
            .unwrap();

        // 2. Builder receives
        let recv = builder_rx.try_recv().unwrap();
        match &recv.payload {
            AgentPayload::TaskDelegation {
                task_id, prompt, ..
            } => {
                assert_eq!(task_id, "task-combined");
                assert!(prompt.contains("validation"));
            }
            other => panic!("expected TaskDelegation, got {other:?}"),
        }

        // 3. Builder sends result back
        let result = AgentMessage::task_result(
            AgentRole::Builder,
            AgentRole::Coordinator,
            "task-combined",
            true,
            "Input validation added to registration endpoint",
        );
        router
            .send_to("builder", "coordinator", result)
            .await
            .unwrap();

        // 4. Skeptic raises an objection about the implementation
        let objection = AgentMessage::objection(
            AgentRole::Skeptic,
            "missing_edge_cases",
            "No validation for unicode characters in username field",
        );
        let objection = objection.directed(AgentRole::Coordinator);
        router
            .send_to("skeptic", "coordinator", objection)
            .await
            .unwrap();

        // 5. Coordinator receives both the result and the objection
        let first = coord_rx.try_recv().unwrap();
        match &first.payload {
            AgentPayload::TaskResult { success, .. } => assert!(*success),
            other => panic!("expected TaskResult, got {other:?}"),
        }

        let second = coord_rx.try_recv().unwrap();
        assert_eq!(second.from, AgentRole::Skeptic);
        match &second.payload {
            AgentPayload::Objection { reason, evidence } => {
                assert_eq!(reason, "missing_edge_cases");
                assert!(evidence.contains("unicode"));
            }
            other => panic!("expected Objection, got {other:?}"),
        }

        // 6. Verify delegation token chain for re-delegation
        let root_token = DelegationToken::root("coordinator");
        let child_token = root_token.child("builder").unwrap();
        assert_eq!(child_token.depth, 1);
        assert!(child_token.can_delegate());

        let grandchild = child_token.child("scalpel").unwrap();
        assert_eq!(grandchild.depth, 2);
        assert!(!grandchild.can_delegate());

        // 7. Verify registry can look up agents by capability
        let mut registry = AgentRegistry::new();
        let mut security_agent = SpecialistAgent::new(
            "SecurityAgent".to_string(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            Some("task-combined".to_string()),
        );
        security_agent.capabilities.push(CapabilityDescriptor {
            name: "input_validation".to_string(),
            description: "Validates user input".to_string(),
            available: true,
            success_rate: 0.88,
            tool_scope: vec!["read_file".into()],
        });
        registry
            .generated
            .insert(security_agent.id.clone(), security_agent);

        let found = registry.find_available("input_validation");
        assert_eq!(found.len(), 1);
        assert!((found[0].capabilities[0].success_rate - 0.88).abs() < f64::EPSILON);

        // All mailboxes should be drained
        assert!(coord_rx.try_recv().is_err());
        assert!(builder_rx.try_recv().is_err());
        assert!(skeptic_rx.try_recv().is_err());
    }
}
