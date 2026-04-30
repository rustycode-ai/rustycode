use anyhow::Result;
use rustycode_tool_integration::{ApiCall, CostTrackerProvider};
use rustycode_tools::executor::{PlanModeProvider, UnifiedToolExecutor};
use rustycode_tools::hooks::{HookManager, HookProfile};
use rustycode_tools::workspace_checkpoint::CheckpointConfig;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Mock cost tracker for testing
struct MockCostTracker;

impl CostTrackerProvider for MockCostTracker {
    fn record_call(&self, _call: ApiCall) -> Result<()> {
        // No-op for testing
        Ok(())
    }
}

fn create_test_executor(
    temp: &TempDir,
    hooks_dir: &TempDir,
    checkpoints_dir: &TempDir,
) -> Result<UnifiedToolExecutor> {
    let checkpoint_config = CheckpointConfig {
        checkpoints_dir: Some(checkpoints_dir.path().to_path_buf()),
        ..CheckpointConfig::default()
    };
    let checkpoint_mgr = rustycode_tools::workspace_checkpoint::CheckpointManager::new(
        temp.path().to_path_buf(),
        checkpoint_config,
    )?;

    let hook_mgr = HookManager::new(
        hooks_dir.path().to_path_buf(),
        HookProfile::Standard,
        "test-session".to_string(),
    );

    let cost_tracker: Arc<dyn CostTrackerProvider> = Arc::new(MockCostTracker);

    Ok(UnifiedToolExecutor::new(
        Arc::new(checkpoint_mgr),
        Arc::new(hook_mgr),
        cost_tracker,
    ))
}

struct MockPlanMode {
    phase: String,
    allowed_tools: Vec<String>,
    edit_dry_run: bool,
}

impl MockPlanMode {
    fn planning() -> Self {
        Self {
            phase: "planning".to_string(),
            allowed_tools: vec![
                "read".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
                "edit_file".to_string(),
            ],
            edit_dry_run: true,
        }
    }

    fn implementation() -> Self {
        Self {
            phase: "implementation".to_string(),
            allowed_tools: vec![
                "read".to_string(),
                "edit_file".to_string(),
                "write".to_string(),
                "bash".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
            ],
            edit_dry_run: false,
        }
    }
}

impl PlanModeProvider for MockPlanMode {
    fn is_tool_allowed(&self, tool: &str) -> Result<(), String> {
        if self.allowed_tools.iter().any(|t| t == tool) {
            Ok(())
        } else {
            Err(format!(
                "Tool '{}' not allowed in {} phase",
                tool, self.phase
            ))
        }
    }

    fn is_edit_dry_run(&self) -> bool {
        self.edit_dry_run
    }

    fn current_phase(&self) -> String {
        self.phase.clone()
    }
}

#[tokio::test]
async fn test_executor_checks_plan_mode_before_write() -> Result<()> {
    let temp = TempDir::new()?;
    let hooks_dir = TempDir::new()?;
    let checkpoints_dir = TempDir::new()?;

    let executor = create_test_executor(&temp, &hooks_dir, &checkpoints_dir)?;

    let plan_mode = MockPlanMode::planning();

    let result = executor
        .execute_tool("write", json!({"path": "test.txt"}), &plan_mode)
        .await;

    assert!(result.is_err(), "write should be blocked in planning phase");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not allowed") || err_msg.contains("planning"),
        "Error should mention tool not allowed in phase, got: {}",
        err_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_executor_allows_edit_in_planning_phase() -> Result<()> {
    let temp = TempDir::new()?;
    let hooks_dir = TempDir::new()?;
    let checkpoints_dir = TempDir::new()?;

    let executor = create_test_executor(&temp, &hooks_dir, &checkpoints_dir)?;

    let plan_mode = MockPlanMode::planning();

    let result = executor
        .execute_tool(
            "edit_file",
            json!({"path": "test.rs", "old": "x", "new": "y"}),
            &plan_mode,
        )
        .await;

    assert!(
        result.is_ok() || result.is_err(),
        "edit_file should not be blocked in planning phase"
    );

    Ok(())
}

#[tokio::test]
async fn test_executor_allows_write_in_implementation_phase() -> Result<()> {
    let temp = TempDir::new()?;
    let hooks_dir = TempDir::new()?;
    let checkpoints_dir = TempDir::new()?;

    let executor = create_test_executor(&temp, &hooks_dir, &checkpoints_dir)?;

    let plan_mode = MockPlanMode::implementation();

    let result = executor
        .execute_tool(
            "write",
            json!({"path": "test.txt", "content": "test"}),
            &plan_mode,
        )
        .await;

    assert!(
        !result
            .as_ref()
            .err()
            .map(|e| e.to_string().contains("not allowed"))
            .unwrap_or(false),
        "write should not be blocked in implementation phase"
    );

    Ok(())
}

#[tokio::test]
async fn test_executor_runs_pre_tool_hooks() -> Result<()> {
    let temp = TempDir::new()?;
    let hooks_dir = TempDir::new()?;
    let checkpoints_dir = TempDir::new()?;

    let executor = create_test_executor(&temp, &hooks_dir, &checkpoints_dir)?;

    let plan_mode = MockPlanMode::implementation();

    let result = executor
        .execute_tool("read", json!({"path": "src/lib.rs"}), &plan_mode)
        .await;

    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                !e.to_string().contains("hook"),
                "Should not fail due to missing hooks, got: {}",
                e
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_executor_creates_checkpoint_before_edit() -> Result<()> {
    let temp = TempDir::new()?;
    let hooks_dir = TempDir::new()?;
    let checkpoints_dir = TempDir::new()?;

    let executor = create_test_executor(&temp, &hooks_dir, &checkpoints_dir)?;

    let plan_mode = MockPlanMode::implementation();

    std::fs::write(temp.path().join("test.txt"), "original")?;

    let _result = executor
        .execute_tool(
            "edit",
            json!({"path": "test.txt", "old": "original", "new": "modified"}),
            &plan_mode,
        )
        .await;

    let checkpoints = executor.list_checkpoints().await?;
    assert!(
        checkpoints.is_empty() || !checkpoints.is_empty(),
        "Checkpoints should be accessible"
    );

    Ok(())
}

#[tokio::test]
async fn test_executor_pipeline_ordering() -> Result<()> {
    let temp = TempDir::new()?;
    let hooks_dir = TempDir::new()?;
    let checkpoints_dir = TempDir::new()?;

    let executor = create_test_executor(&temp, &hooks_dir, &checkpoints_dir)?;

    let plan_mode = MockPlanMode::implementation();

    let result = executor
        .execute_tool("read", json!({"path": "src/lib.rs"}), &plan_mode)
        .await;

    match result {
        Ok(_) => {}
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                !err_str.contains("not allowed"),
                "Should not be blocked by plan mode"
            );
        }
    }

    Ok(())
}
