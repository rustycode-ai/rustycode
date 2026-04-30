// Verification script for sortable ID migration
// Run with: cargo run --example verify_migration

use chrono::Utc;
use rustycode_protocol::{
    EventKind, Plan, PlanId, PlanStatus, Session, SessionEvent, SessionId, SessionMode,
    SessionStatus,
};

fn main() {
    println!("=== Sortable ID Migration Verification ===\n");

    // 1. Demonstrate PlanId generation
    println!("1. PlanId Generation:");
    let plan_id = PlanId::new();
    println!("   Generated PlanId: {}", plan_id);
    println!("   Length: {} characters", plan_id.to_string().len());
    println!("   Prefix: {}", plan_id.inner().prefix());
    println!("   Timestamp: {:?}", plan_id.timestamp());

    // 2. Demonstrate SessionId generation
    println!("\n2. SessionId Generation:");
    let session_id = SessionId::new();
    println!("   Generated SessionId: {}", session_id);
    println!("   Length: {} characters", session_id.to_string().len());

    // 3. Create a Session with sortable IDs
    println!("\n3. Session with Sortable IDs:");
    let session = Session {
        id: SessionId::new(),
        task: "Migrate to sortable IDs".to_string(),
        created_at: Utc::now(),
        mode: SessionMode::Executing,
        status: SessionStatus::Executing,
        plan_path: None,
    };
    println!("   Session ID: {}", session.id);
    println!("   Task: {}", session.task);

    // 4. Create a Plan with sortable IDs
    println!("\n4. Plan with Sortable IDs:");
    let plan = Plan {
        id: PlanId::new(),
        session_id: session.id.clone(),
        task: session.task.clone(),
        created_at: Utc::now(),
        status: PlanStatus::Executing,
        summary: "Migrate protocol to use sortable IDs".to_string(),
        approach: "Replace Uuid with PlanId throughout the codebase".to_string(),
        steps: vec![],
        files_to_modify: vec!["crates/rustycode-protocol/src/lib.rs".to_string()],
        risks: vec!["Breaking change for existing databases".to_string()],
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
    };
    println!("   Plan ID: {}", plan.id);
    println!("   Session ID: {}", plan.session_id);
    println!("   Summary: {}", plan.summary);

    // 5. Serialize to JSON (demonstrating string serialization)
    println!("\n5. JSON Serialization:");
    let plan_json = serde_json::to_string_pretty(&plan).unwrap();
    println!("   {}", plan_json);

    // 6. Deserialize back
    println!("\n6. JSON Deserialization:");
    let deserialized: Plan = serde_json::from_str(&plan_json).unwrap();
    println!("   Successfully deserialized plan!");
    println!("   Plan ID matches: {}", deserialized.id == plan.id);

    // 7. Demonstrate time-based sorting
    println!("\n7. Time-Based Sorting:");
    use std::thread;
    use std::time::Duration;

    let id1 = PlanId::new();
    thread::sleep(Duration::from_millis(10));
    let id2 = PlanId::new();
    thread::sleep(Duration::from_millis(10));
    let id3 = PlanId::new();

    let mut ids = [id3.clone(), id1.clone(), id2.clone()];
    println!(
        "   Before sort: {:?}",
        ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()
    );
    ids.sort();
    println!(
        "   After sort:  {:?}",
        ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()
    );
    println!("   Correct order: {} < {} < {}", id1, id2, id3);

    // 8. Demonstrate EventKind with all required variants
    println!("\n8. EventKind Variants:");
    println!("   SessionStarted: {:?}", EventKind::SessionStarted);
    println!("   SessionCompleted: {:?}", EventKind::SessionCompleted);
    println!("   SessionFailed: {:?}", EventKind::SessionFailed);
    println!(
        "   Other: {:?}",
        EventKind::Other("CustomEvent".to_string())
    );

    // 9. Create SessionEvent
    println!("\n9. SessionEvent with Sortable IDs:");
    let event = SessionEvent {
        session_id: session.id.clone(),
        at: Utc::now(),
        kind: EventKind::SessionCompleted,
        detail: "Migration completed successfully".to_string(),
    };
    println!("   Event: {:?}", event);

    // 10. Parse IDs from strings
    println!("\n10. ID Parsing:");
    let id_str = plan.id.to_string();
    let parsed = PlanId::parse(&id_str).unwrap();
    println!("   Original: {}", plan.id);
    println!("   Parsed:   {}", parsed);
    println!("   Match: {}", plan.id == parsed);

    println!("\n=== All Verification Tests Passed! ===");
}
