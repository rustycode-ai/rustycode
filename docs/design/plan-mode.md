# RustyCode Plan Mode Architecture

**Status**: Design Document
**Author**: RustyCode Ensemble
**Created**: 2026-03-12
**Related ADRs**: 0001-core-principles.md, 0002-context-budgeting.md

## Executive Summary

This document describes the architecture for a two-phase session mode system that separates planning from execution. Plan mode allows the AI to safely explore a codebase using read-only operations before seeking user approval to make changes. This provides transparency, control, and confidence in AI-assisted development workflows.

## Motivation

### Current State
- Single execution mode: AI has full access from the start
- No preview of intended changes before they happen
- Users must trust AI to make correct modifications
- No way to explore without side effects
- Difficult to understand AI's reasoning process

### Goals
1. **Safety**: Explore codebases without risk of unintended changes
2. **Transparency**: Users see planned changes before execution
3. **Control**: Explicit approval gate before execution phase
4. **Flexibility**: Seamless transition between planning and execution
5. **Auditability**: Plans stored as inspectable markdown documents
6. **Incremental**: Support for iterative refinement of plans

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    RustyCode Session Modes                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────────┐         ┌──────────────────┐             │
│  │  Planning Mode   │────────>│ Execution Mode   │             │
│  │  (Read-Only)     │ Approve │ (Full Access)    │             │
│  └──────────────────┘         └──────────────────┘             │
│           │                            ^                         │
│           │ Reject                     |                         │
│           └────────────────────────────┘                         │
│                                                                   │
│  Planning Mode Permissions:      Execution Mode Permissions:     │
│  - Git inspection                - All planning permissions     │
│  - LSP queries                   - Write files                  │
│  - Read operations               - Edit files                   │
│  - Memory access                 - Execute bash commands        │
│  - Skill discovery               - Commit changes               │
│                                                                   │
│  Blocked in Planning:            Blocked in Execution:           │
│  - Write/Edit tools              - (None - full access)         │
│  - Bash execution                                                   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## State Machine

```
                    ┌─────────────┐
                    │   Initial   │
                    └──────┬──────┘
                           │
                    User runs: rustycode plan "task"
                           │
                           ▼
              ┌────────────────────────┐
              │   PLANNING             │
              │   - Read-only access   │
              │   - Explore codebase   │
              │   - Build plan         │
              └──────────┬─────────────┘
                         │
                         │ AI generates plan
                         │
                         ▼
              ┌────────────────────────┐
              │   PLAN_READY           │
              │   - Plan saved to disk │
              │   - User reviews       │
              └──────────┬─────────────┘
                         │
            ┌────────────┴────────────┐
            │                         │
      User:                   User: reject
 rustycode approve          rustycode reject
            │                         │
            ▼                         ▼
  ┌───────────────────┐     ┌──────────────────┐
  │   EXECUTING       │     │   REJECTED        │
  │   - Full access   │     │   - Cleanup       │
  │   - Execute plan  │     │   - Archive plan  │
  └─────────┬─────────┘     └──────────────────┘
            │
            │ Plan completes
            │
            ▼
  ┌───────────────────┐
  │   COMPLETED       │
  │   - Summary saved │
  │   - Return to     │
  │     normal mode   │
  └───────────────────┘
```

## Protocol Changes

### SessionMode Enum

**Location**: `rustycode-protocol/src/lib.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionMode {
    Planning,
    Executing,
}

impl Default for SessionMode {
    fn default() -> Self {
        Self::Executing  // Backward compatible: direct execution
    }
}
```

### Extended Session Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub task: String,
    pub created_at: DateTime<Utc>,
    pub mode: SessionMode,           // NEW
    pub plan_path: Option<String>,   // NEW: Path to plan markdown
    pub status: SessionStatus,       // NEW
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Planning,      // Actively exploring
    PlanReady,     // Awaiting approval
    Executing,     // Running approved plan
    Completed,     // Successfully finished
    Rejected,      // Plan rejected by user
    Failed,        // Execution failed
}
```

### New Event Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    // Existing events...
    SessionStarted,
    ContextAssembled,
    InspectionCompleted,

    // NEW: Plan mode events
    PlanCreated,
    PlanApproved,
    PlanRejected,
    PlanExecutionStarted,
    PlanExecutionCompleted,
    PlanExecutionFailed,
    ToolBlockedInPlanningMode,  // Audit log
}
```

### Plan Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: Uuid,
    pub session_id: SessionId,
    pub task: String,
    pub created_at: DateTime<Utc>,
    pub status: PlanStatus,

    // Plan content
    pub summary: String,              // One-line description
    pub approach: String,             // Implementation strategy
    pub steps: Vec<PlanStep>,         // Ordered execution steps
    pub files_to_modify: Vec<String>, // Affected files
    pub estimated_changes: usize,     // Number of files/changes
    pub risks: Vec<String>,           // Potential issues
    pub dependencies: Vec<String>,    // Related work
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,        // Being written
    Ready,        // Awaiting review
    Approved,     // User approved
    Rejected,     // User rejected
    Executing,    // Running
    Completed,    // Successfully executed
    Failed,       // Execution error
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub order: usize,
    pub title: String,
    pub description: String,
    pub tools: Vec<String>,           // Tools to use
    pub expected_outcome: String,     // Success criteria
    pub rollback_hint: String,        // How to undo if needed
}
```

## Tool Permission Matrix

| Tool Category        | Specific Tools             | Planning Mode | Execution Mode |
|---------------------|----------------------------|---------------|----------------|
| **Git Inspection**  | `git status`, `git log`    | ✅ Allowed    | ✅ Allowed      |
|                     | `git diff`, `git show`     | ✅ Allowed    | ✅ Allowed      |
| **LSP Queries**     | `goToDefinition`           | ✅ Allowed    | ✅ Allowed      |
|                     | `findReferences`           | ✅ Allowed    | ✅ Allowed      |
|                     | `hover`, `documentSymbol`  | ✅ Allowed    | ✅ Allowed      |
| **File Operations** | `Read`                     | ✅ Allowed    | ✅ Allowed      |
|                     | `Glob` (search)            | ✅ Allowed    | ✅ Allowed      |
|                     | `Grep` (search)            | ✅ Allowed    | ✅ Allowed      |
|                     | `Write`                    | ❌ Blocked    | ✅ Allowed      |
|                     | `Edit`                     | ❌ Blocked    | ✅ Allowed      |
| **Execution**       | `Bash`                     | ❌ Blocked    | ✅ Allowed      |
| **Memory**          | All memory operations      | ✅ Allowed    | ✅ Allowed      |
| **Skills**          | Skill discovery/invocation | ⚠️ Restricted | ✅ Allowed      |
| **Planning**        | `TaskCreate`, `TaskUpdate` | ✅ Allowed    | ✅ Allowed      |
|                     | `TaskList`, `TaskGet`      | ✅ Allowed    | ✅ Allowed      |

**Note**: Skills in planning mode are restricted to read-only skills. Skills that attempt to use Write/Edit/Bash will be blocked.

## Core Runtime Changes

**Location**: `rustycode-core/src/lib.rs`

### Mode-Aware Tool Dispatcher

```rust
pub struct Runtime {
    config: Config,
    storage: Storage,
    current_session: Option<Session>,
}

impl Runtime {
    // NEW: Start a planning session
    pub fn start_planning(&mut self, cwd: &Path, task: &str) -> Result<PlanSession> {
        let session = Session {
            id: SessionId::new(),
            task: task.to_string(),
            created_at: Utc::now(),
            mode: SessionMode::Planning,
            plan_path: None,
            status: SessionStatus::Planning,
        };
        self.storage.insert_session(&session)?;
        self.current_session = Some(session.clone());

        Ok(PlanSession {
            session,
            runtime: self,
        })
    }

    // NEW: Execute an approved plan
    pub fn execute_plan(&mut self, plan_id: Uuid) -> Result<ExecutionSession> {
        let session = self.current_session.as_ref()
            .ok_or_else(|| anyhow!("no active session"))?;

        if session.mode != SessionMode::Planning {
            bail!("Can only execute from planning mode");
        }

        // Load plan
        let plan = self.storage.load_plan(plan_id)?;

        // Transition to execution mode
        let mut execution_session = session.clone();
        execution_session.mode = SessionMode::Executing;
        execution_session.status = SessionStatus::Executing;
        self.storage.update_session(&execution_session)?;

        Ok(ExecutionSession {
            session: execution_session,
            plan,
            runtime: self,
        })
    }

    // NEW: Reject current plan
    pub fn reject_plan(&mut self) -> Result<()> {
        let session = self.current_session.as_ref()
            .ok_or_else(|| anyhow!("no active session"))?;

        let mut rejected = session.clone();
        rejected.status = SessionStatus::Rejected;
        self.storage.update_session(&rejected)?;

        self.storage.insert_event(&SessionEvent {
            session_id: session.id.clone(),
            at: Utc::now(),
            kind: EventKind::PlanRejected,
            detail: "Plan rejected by user".to_string(),
        })?;

        Ok(())
    }
}

// Planning mode guard
pub struct PlanSession<'a> {
    session: Session,
    runtime: &'a mut Runtime,
}

impl<'a> PlanSession<'a> {
    pub fn execute_tool(&mut self, tool: &ToolCall) -> Result<ToolResult> {
        // Check tool permissions
        if !is_read_only_tool(tool) {
            self.runtime.storage.insert_event(&SessionEvent {
                session_id: self.session.id.clone(),
                at: Utc::now(),
                kind: EventKind::ToolBlockedInPlanningMode,
                detail: format!("Tool {} blocked in planning mode", tool.name),
            })?;
            bail!("Tool '{}' is not allowed in planning mode", tool.name);
        }

        // Execute tool
        dispatch_tool(tool)
    }

    pub fn save_plan(&mut self, plan: Plan) -> Result<PathBuf> {
        let plan_path = self.runtime.config.plans_dir
            .join(format!("{}.md", plan.id));

        // Save as markdown
        let markdown = render_plan_markdown(&plan);
        std::fs::write(&plan_path, markdown)?;

        // Update session
        self.session.plan_path = Some(plan_path.to_string_lossy().to_string());
        self.session.status = SessionStatus::PlanReady;
        self.runtime.storage.update_session(&self.session)?;

        // Store in database
        self.runtime.storage.insert_plan(&plan)?;

        Ok(plan_path)
    }
}

// Execution mode guard
pub struct ExecutionSession<'a> {
    session: Session,
    plan: Plan,
    runtime: &'a mut Runtime,
}

impl<'a> ExecutionSession<'a> {
    pub fn execute_tool(&mut self, tool: &ToolCall) -> Result<ToolResult> {
        // All tools allowed in execution mode
        dispatch_tool(tool)
    }

    pub fn complete_step(&mut self, step_order: usize) -> Result<()> {
        // Mark step as completed
        // Update plan status
        Ok(())
    }
}

fn is_read_only_tool(tool: &ToolCall) -> bool {
    matches!(tool.name.as_str(),
        "read" | "glob" | "grep" |
        "git_status" | "git_log" | "git_diff" |
        "lsp_hover" | "lsp_definition" | "lsp_references" |
        "task_create" | "task_update" | "task_list" | "task_get"
    )
}
```

## Storage Layer Changes

**Location**: `rustycode-storage/src/lib.rs`

### New Database Tables

```sql
-- Plans table
create table if not exists plans (
    id text primary key,
    session_id text not null,
    task text not null,
    created_at text not null,
    status text not null,
    summary text not null,
    approach text not null,
    steps text not null,           -- JSON array
    files_to_modify text not null, -- JSON array
    estimated_changes integer not null,
    risks text not null,           -- JSON array
    dependencies text not null,    -- JSON array
    foreign key (session_id) references sessions(id)
);

-- Plan execution log
create table if not exists plan_execution_log (
    id integer primary key autoincrement,
    plan_id text not null,
    step_order integer not null,
    started_at text not null,
    completed_at text,
    status text not null,
    output text,
    foreign key (plan_id) references plans(id)
);
```

### New Storage Methods

```rust
impl Storage {
    pub fn insert_plan(&self, plan: &Plan) -> Result<()> {
        self.conn.execute(
            "insert into plans (
                id, session_id, task, created_at, status,
                summary, approach, steps, files_to_modify,
                estimated_changes, risks, dependencies
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                plan.id.to_string(),
                plan.session_id.0.to_string(),
                plan.task,
                plan.created_at,
                serde_json::to_string(&plan.status)?,
                plan.summary,
                plan.approach,
                serde_json::to_string(&plan.steps)?,
                serde_json::to_string(&plan.files_to_modify)?,
                plan.estimated_changes,
                serde_json::to_string(&plan.risks)?,
                serde_json::to_string(&plan.dependencies)?,
            ],
        )?;
        Ok(())
    }

    pub fn load_plan(&self, plan_id: Uuid) -> Result<Plan> {
        // Load plan from database
        // ...
    }

    pub fn update_plan_status(&self, plan_id: Uuid, status: PlanStatus) -> Result<()> {
        // Update plan status
        // ...
    }

    pub fn list_plans(&self, session_id: &SessionId) -> Result<Vec<Plan>> {
        // List all plans for a session
        // ...
    }
}
```

## Plan Markdown Format

**Location**: `.rustycode/plans/{plan-id}.md`

```markdown
# Plan: {summary}

**Plan ID**: `{plan-id}`
**Session ID**: `{session-id}`
**Created**: {timestamp}
**Status**: {Draft | Ready | Approved | Rejected | Executing | Completed | Failed}

## Overview

{summary}

## Approach

{approach - implementation strategy}

## Estimated Impact

- **Files to modify**: {count}
- **Estimated changes**: {count}
- **Risk level**: {Low | Medium | High}

## Steps

### 1. {step-title}

{step-description}

**Tools**: `tool1`, `tool2`
**Expected outcome**: {expected_outcome}
**Rollback**: {rollback_hint}

### 2. {step-title}

...

## Files to Modify

{list of files with brief descriptions of changes}

- `path/to/file1.ext` - {description of change}
- `path/to/file2.ext` - {description of change}

## Risks & Considerations

{list of potential issues}

- {risk 1}
- {risk 2}

## Dependencies

{related work or prerequisites}

- {dependency 1}
- {dependency 2}

## Execution Log

*This section is populated during execution*

### [2026-03-12 14:32:15] Step 1: {step-title}
**Status**: ✅ Success
**Output**: {tool output}

### [2026-03-12 14:33:22] Step 2: {step-title}
**Status**: ❌ Failed
**Error**: {error message}
```

## CLI Commands

**Location**: `rustycode-cli/src/main.rs`

### New Command Structure

```rust
#[derive(Debug, Subcommand)]
enum Command {
    // Existing commands
    Doctor,
    Config { ... },
    Context { ... },
    Run { ... },

    // NEW: Plan mode commands
    Plan {
        prompt: String,
        #[arg(short, long)]
        output: Option<PathBuf>,  // Custom plan output path
    },
    Approve {
        #[arg(short, long)]
        plan: Option<String>,      // Plan ID (defaults to latest)
    },
    Reject {
        #[arg(short, long)]
        plan: Option<String>,      // Plan ID (defaults to latest)
    },
    List {
        #[arg(long)]
        all: bool,                 // Show all plans, not just pending
    },
    Show {
        plan_id: String,
    },
}
```

### Command Implementations

```rust
fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let runtime = Runtime::load(&cwd)?;

    match cli.command {
        // Existing commands...
        Command::Run { prompt } => {
            let report = runtime.run(&cwd, &prompt)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }

        // NEW: Plan mode commands
        Command::Plan { prompt, output } => {
            let mut plan_session = runtime.start_planning(&cwd, &prompt)?;

            println!("🔍 Planning mode activated");
            println!("📋 Task: {}", prompt);
            println!("🔒 Read-only access enabled");
            println!();

            // AI explores and creates plan
            let plan = plan_session.create_plan_with_ai(&prompt)?;

            let plan_path = plan_session.save_plan(plan)?;

            println!("✅ Plan created: {}", plan_path.display());
            println!();
            println!("Review the plan with:");
            println!("  rustycode show {}", plan_session.session.id);
            println!();
            println!("When ready, approve with:");
            println!("  rustycode approve");
        }

        Command::Approve { plan } => {
            let plan_id = resolve_plan_id(&runtime, plan)?;
            println!("📝 Approving plan: {}", plan_id);

            let mut execution_session = runtime.execute_plan(plan_id)?;

            println!("🚀 Execution mode activated");
            println!("🔓 Full access enabled");
            println!();

            // Execute plan steps
            execution_session.execute_all_steps()?;

            println!("✅ Plan completed successfully");
        }

        Command::Reject { plan } => {
            let plan_id = resolve_plan_id(&runtime, plan)?;
            println!("❌ Rejecting plan: {}", plan_id);

            runtime.reject_plan()?;

            println!("Plan rejected and archived");
        }

        Command::List { all } => {
            let plans = runtime.list_plans(all)?;

            if plans.is_empty() {
                println!("No plans found");
            } else {
                println!("Plans:");
                for plan in plans {
                    let status_icon = match plan.status {
                        PlanStatus::Ready => "⏳",
                        PlanStatus::Approved => "✅",
                        PlanStatus::Rejected => "❌",
                        PlanStatus::Executing => "🔄",
                        PlanStatus::Completed => "✓",
                        PlanStatus::Failed => "✗",
                        _ => "○",
                    };
                    println!("  {} {} - {} ({})",
                        status_icon,
                        plan.id,
                        plan.summary,
                        plan.status
                    );
                }
            }
        }

        Command::Show { plan_id } => {
            let plan = runtime.load_plan(&plan_id)?;
            let markdown = render_plan_markdown(&plan);
            println!("{}", markdown);
        }
    }

    Ok(())
}

fn resolve_plan_id(runtime: &Runtime, plan_spec: Option<String>) -> Result<Uuid> {
    if let Some(spec) = plan_spec {
        // Parse as UUID or resolve by index
        Ok(Uuid::parse_str(&spec)?)
    } else {
        // Get most recent pending plan
        runtime.get_latest_pending_plan()?
    }
}
```

## Integration Points

### 1. Tool Dispatch Layer

**Location**: `rustycode-core/src/tool_dispatcher.rs` (new file)

```rust
pub struct ToolDispatcher {
    mode: SessionMode,
    storage: Storage,
    session_id: SessionId,
}

impl ToolDispatcher {
    pub fn dispatch(&self, tool: ToolCall) -> Result<ToolResult> {
        // Check permissions based on mode
        self.check_permission(&tool)?;

        // Execute tool
        let result = self.execute_tool(tool)?;

        // Audit log
        self.storage.insert_event(&SessionEvent {
            session_id: self.session_id.clone(),
            at: Utc::now(),
            kind: EventKind::ToolExecuted,
            detail: format!("{}: {}", tool.name, result.summary),
        })?;

        Ok(result)
    }

    fn check_permission(&self, tool: &ToolCall) -> Result<()> {
        match self.mode {
            SessionMode::Planning => {
                if !is_read_only_tool(tool) {
                    bail!("Tool '{}' not allowed in planning mode", tool.name);
                }
            }
            SessionMode::Executing => {
                // All tools allowed
            }
        }
        Ok(())
    }
}
```

### 2. AI Planning Agent

**Location**: `rustycode-core/src/planning_agent.rs` (new file)

```rust
pub struct PlanningAgent {
    context_builder: ContextBuilder,
}

impl PlanningAgent {
    pub fn create_plan(
        &self,
        task: &str,
        session: &PlanSession,
    ) -> Result<Plan> {
        // Build context with read-only tools
        let context = self.context_builder.build_planning_context(task, session)?;

        // Use LLM to analyze and create plan
        let plan_prompt = format!(
            "Task: {}\n\nContext:\n{}\n\nCreate a detailed execution plan.",
            task, context
        );

        // Call LLM
        let response = self.call_llm(&plan_prompt)?;

        // Parse response into Plan
        let plan = self.parse_plan(response, task)?;

        Ok(plan)
    }

    fn parse_plan(&self, response: String, task: &str) -> Result<Plan> {
        // Parse LLM response into structured Plan
        // Expect JSON or structured markdown
        // ...
    }
}
```

### 3. Session Management

**Location**: `rustycode-core/src/session.rs` (new file)

```rust
pub struct SessionManager {
    storage: Storage,
    current_session: Option<Session>,
}

impl SessionManager {
    pub fn transition_to_executing(&mut self, plan_id: Uuid) -> Result<()> {
        let session = self.current_session.as_ref()
            .ok_or_else(|| anyhow!("no active session"))?;

        // Validate transition
        if session.mode != SessionMode::Planning {
            bail!("Can only transition from planning to executing");
        }

        if session.status != SessionStatus::PlanReady {
            bail!("Plan must be ready before execution");
        }

        // Update session
        let mut updated = session.clone();
        updated.mode = SessionMode::Executing;
        updated.status = SessionStatus::Executing;
        self.storage.update_session(&updated)?;

        // Log event
        self.storage.insert_event(&SessionEvent {
            session_id: session.id.clone(),
            at: Utc::now(),
            kind: EventKind::PlanExecutionStarted,
            detail: format!("Executing plan {}", plan_id),
        })?;

        self.current_session = Some(updated);
        Ok(())
    }
}
```

## Example User Workflow

### Scenario: Add logging to a function

```bash
# User starts planning mode
$ rustycode plan "Add debug logging to the authenticate() function in src/auth.rs"

🔍 Planning mode activated
📋 Task: Add debug logging to authenticate() function
🔒 Read-only access enabled

# AI explores the codebase (read-only)
[AI] Reading src/auth.rs...
[AI] Found authenticate() function at line 42
[AI] Checking for existing logging infrastructure...
[AI] Found tracing crate in dependencies
[AI] Looking at existing log patterns in the codebase...

# AI creates plan
[AI] Creating execution plan...
✅ Plan created: .rustycode/plans/a1b2c3d4-...md

# Plan summary shown
Plan: Add debug logging to authenticate() function
Status: Ready
Steps: 3
Files to modify: 1 (src/auth.rs)
Estimated changes: Low risk

Review the plan:
  $ rustycode show a1b2c3d4

# User reviews the plan
$ rustycode show a1b2c3d4

# Plan: Add debug logging to authenticate() function
# ...
# Steps:
# 1. Add tracing::info! at function entry
# 2. Add tracing::debug! for each validation step
# 3. Add tracing::error! for error paths
# ...

# User approves
$ rustycode approve

🚀 Execution mode activated
🔓 Full access enabled

Executing plan a1b2c3d4...

[1/3] Adding entry logging... ✅
[2/3] Adding validation logging... ✅
[3/3] Adding error logging... ✅

✅ Plan completed successfully

# User can now test the changes
$ cargo test
```

### Scenario: User rejects plan

```bash
$ rustycode plan "Refactor User struct to use builder pattern"

# AI explores and creates plan
✅ Plan created: .rustycode/plans/e5f6g7h8-...md

# User reviews plan
$ rustycode show e5f6g7h8

# Plan: Refactor User struct to builder pattern
# Steps: 15
# Files to modify: 8
# Risk level: High

# User decides this is too risky
$ rustycode reject

❌ Plan rejected and archived

# User can try a different approach
$ rustycode plan "Add builder pattern for User creation only"
```

## Error Handling

### Planning Mode Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum PlanningError {
    #[error("Tool '{0}' is not allowed in planning mode")]
    ToolBlocked(String),

    #[error("Plan not found: {0}")]
    PlanNotFound(Uuid),

    #[error("Invalid plan transition from {0} to {1}")]
    InvalidTransition(SessionStatus, SessionStatus),

    #[error("No active planning session")]
    NoActiveSession,

    #[error("Plan execution failed: {0}")]
    ExecutionFailed(String),
}
```

### Error Recovery

1. **Tool blocked**: Log event, return helpful error message
2. **Plan not found**: List available plans with `rustycode list`
3. **Invalid transition**: Explain state machine requirements
4. **Execution failure**: Log step where it failed, provide rollback hints

## Testing Strategy

### Unit Tests

- Session mode state transitions
- Tool permission checking
- Plan serialization/deserialization
- Markdown rendering

### Integration Tests

- End-to-end planning workflow
- Plan approval and execution
- Error scenarios (invalid transitions, blocked tools)
- Plan persistence and recovery

### Example Test

```rust
#[test]
fn planning_mode_blocks_write_operations() {
    let runtime = setup_test_runtime();
    let mut plan_session = runtime.start_planning(&cwd(), "test task").unwrap();

    let write_tool = ToolCall {
        name: "write".to_string(),
        arguments: vec![],
    };

    let result = plan_session.execute_tool(&write_tool);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        PlanningError::ToolBlocked(_)
    ));
}
```

## Migration Path

### Phase 1: Core Infrastructure
1. Add `SessionMode` enum to protocol
2. Extend `Session` structure
3. Add plan storage tables
4. Implement mode-aware tool dispatcher

### Phase 2: Planning Mode
1. Implement planning agent
2. Add plan markdown rendering
3. Create planning CLI commands
4. Add read-only tool enforcement

### Phase 3: Execution Mode
1. Implement plan executor
2. Add approval/rejection commands
3. Implement execution logging
4. Add rollback support

### Phase 4: Polish
1. Add interactive review UI
2. Implement plan diff visualization
3. Add plan templates
4. Performance optimization

## Future Enhancements

### Plan Diff Visualization
Show side-by-side comparison of planned vs. actual changes

### Plan Templates
Common patterns for frequent tasks (add logging, refactor, etc.)

### Interactive Plan Editing
Allow users to modify AI-generated plans before approval

### Plan Chaining
Use completed plans as dependencies for future tasks

### Plan Analytics
Track plan success rates, common patterns, optimization opportunities

### Multi-File Plan Preview
Show all planned changes in a unified diff view

### Rollback Automation
Automatic rollback on plan failure with configurable policies

## Security Considerations

### Plan Approval Gate
- Plans must be explicitly approved by user
- No automatic execution without review
- Clear audit trail of all approvals

### Read-Only Enforcement
- Kernel-level file system monitoring (future)
- Comprehensive permission matrix
- Audit logging of all blocked operations

### Plan Sandboxing
- Execute plans in isolated environment (future)
- Dry-run mode to preview changes
- Timeboxed execution limits

### Data Isolation
- Plans stored in user-controlled directory
- Clear separation of planning and execution data
- Easy plan deletion and cleanup

## Performance Considerations

### Plan Generation
- Cache context exploration results
- Incremental plan updates for small changes
- Parallel read-only operations

### Plan Storage
- Indexed queries for fast plan lookup
- Compressed storage for large plans
- Lazy loading of plan details

### Execution Logging
- Streaming log writes
- Buffered commit for performance
- Async log flushing

## Documentation Requirements

### User Documentation
1. Getting started with plan mode
2. CLI command reference
3. Plan format specification
4. Best practices for plan review

### Developer Documentation
1. Architecture overview
2. State machine documentation
3. Extension points for custom tools
4. Testing guide

### API Documentation
1. Session management APIs
2. Tool dispatch APIs
3. Plan storage APIs
4. Event system integration

## Success Metrics

### User Adoption
- Percentage of sessions using plan mode
- Average plan approval rate
- User satisfaction scores

### Technical Metrics
- Plan execution success rate
- Average plan creation time
- Tool blocking rate (false positives)

### Safety Metrics
- Data loss incidents (goal: 0)
- Rollback frequency
- Plan rejection reasons analysis

## Open Questions

1. **Plan versioning**: Should we support plan iterations and revisions?
2. **Collaborative planning**: How can multiple users review/modify plans?
3. **Plan templates**: Should we provide pre-built plans for common tasks?
4. **Time limits**: Should planning sessions have automatic timeouts?
5. **Plan sharing**: How can plans be shared between ensemble members?

## References

- [Claude Code Plan Mode](https://claude.ai/code)
- [GitHub Copilot](https://github.com/features/copilot)
- [Aider AI Planning](https://aider.chat/)
- [Cursor AI Planning](https://cursor.sh/)
