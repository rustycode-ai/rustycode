use chrono::Utc;
use rustycode_protocol::StepStatus;
use rustycode_protocol::{
    Plan, PlanId, PlanStatus, PlanStep, Session, SessionId, SessionMode, SessionStatus,
};
use rustycode_storage::Storage;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    println!("=== Plan Storage Demo ===\n");

    // Create a temporary database
    let db_path = Path::new("demo_plan_storage.db");
    let storage = Storage::open(db_path)?;

    // Create and save a session
    let session_id = SessionId::new();
    let session = Session {
        id: session_id.clone(),
        task: "Implement user authentication system".to_string(),
        created_at: Utc::now(),
        mode: SessionMode::Planning,
        status: SessionStatus::Planning,
        plan_path: None,
    };
    storage.insert_session(&session)?;
    println!("Created session: {}", session_id);

    // Create a comprehensive plan
    let plan = Plan {
        id: PlanId::new(),
        session_id: session_id.clone(),
        task: "Implement user authentication system".to_string(),
        created_at: Utc::now(),
        status: PlanStatus::Draft,
        summary: "Add JWT-based authentication with role-based access control".to_string(),
        approach: "Implement auth middleware, JWT token generation, and user store".to_string(),
        steps: vec![
            PlanStep {
                order: 1,
                title: "Design auth data models".to_string(),
                description: "Define User, Role, and Permission structures".to_string(),
                tools: vec!["editor".to_string()],
                expected_outcome: "Data models defined in src/auth/models.rs".to_string(),
                rollback_hint: "Delete src/auth/models.rs".to_string(),
                execution_status: StepStatus::Pending,
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            },
            PlanStep {
                order: 2,
                title: "Implement JWT token generation".to_string(),
                description: "Create token generation and validation functions".to_string(),
                tools: vec!["editor".to_string(), "bash".to_string()],
                expected_outcome: "JWT utils in src/auth/jwt.rs".to_string(),
                rollback_hint: "Delete src/auth/jwt.rs".to_string(),
                execution_status: StepStatus::Pending,
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            },
            PlanStep {
                order: 3,
                title: "Add authentication middleware".to_string(),
                description: "Create middleware to validate tokens on protected routes".to_string(),
                tools: vec!["editor".to_string()],
                expected_outcome: "Middleware in src/auth/middleware.rs".to_string(),
                rollback_hint: "Delete src/auth/middleware.rs".to_string(),
                execution_status: StepStatus::Pending,
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            },
        ],
        files_to_modify: vec![
            "src/auth/models.rs".to_string(),
            "src/auth/jwt.rs".to_string(),
            "src/auth/middleware.rs".to_string(),
            "src/main.rs".to_string(),
        ],
        risks: vec![
            "JWT secret management needs secure solution".to_string(),
            "Token refresh logic may be complex".to_string(),
            "Performance impact on every request".to_string(),
        ],
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
    };

    println!("\n📝 Created plan:");
    println!("  ID: {}", plan.id);
    println!("  Task: {}", plan.task);
    println!("  Summary: {}", plan.summary);
    println!("  Steps: {}", plan.steps.len());
    println!("  Files to modify: {}", plan.files_to_modify.len());
    println!("  Risks identified: {}", plan.risks.len());

    // Save the plan
    storage.insert_plan(&plan)?;
    println!("\n✅ Plan saved to database");

    // Update plan status through lifecycle
    println!("\n🔄 Updating plan status through lifecycle:");
    for status in [
        PlanStatus::Ready,
        PlanStatus::Approved,
        PlanStatus::Executing,
    ] {
        storage.update_plan_status(&plan.id, &status)?;
        println!("  → {:?}", status);
    }

    // Load the plan back
    let loaded = storage.load_plan(&plan.id)?.expect("Plan should exist");
    println!("\n📖 Loaded plan from database:");
    println!("  Status: {:?}", loaded.status);
    println!(
        "  Steps: {} (first: '{}')",
        loaded.steps.len(),
        loaded.steps[0].title
    );
    println!("  Files: {}", loaded.files_to_modify.len());
    println!("  Risks: {}", loaded.risks.len());

    // List all plans for the session
    let plans = storage.list_plans(&session_id)?;
    println!("\n📋 Plans for session {}: {}", session_id, plans.len());

    // Demonstrate plan completion
    storage.update_plan_status(&plan.id, &PlanStatus::Completed)?;
    let final_plan = storage.load_plan(&plan.id)?.unwrap();
    println!("\n🎉 Plan completed with status: {:?}", final_plan.status);

    // Clean up
    std::fs::remove_file(db_path)?;
    println!("\n🧹 Cleaned up demo database");

    println!("\n=== Demo Complete ===");
    Ok(())
}
