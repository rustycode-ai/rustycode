// ============================================================================
// Worked example: BashTool with Sandbox → RateLimit → Audit middleware stack
// ============================================================================

use serde::Deserialize;

// --- Step 1: Define typed params ---

#[derive(Deserialize)]
pub struct BashParams {
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub workdir: Option<String>,
}

fn default_timeout() -> u64 {
    30
}

// --- Step 2: Implement Tool (what the tool author writes) ---

pub struct BashTool;

impl Tool for BashTool {
    type Params = BashParams;

    fn meta() -> ToolMeta {
        ToolMeta {
            name: "bash".into(),
            description: "Execute shell commands".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command to run" },
                    "timeout_secs": { "type": "integer", "default": 30 },
                    "workdir": { "type": "string" }
                },
                "required": ["command"]
            }),
            permissions: Permissions {
                destructive: true,
                network_access: true,
                requires_approval: true,
                ..Default::default()
            },
        }
    }

    fn validate(params: &Self::Params) -> Result<(), ToolError> {
        if params.command.is_empty() {
            return Err(ToolError::Validation("command must not be empty".into()));
        }
        if params.timeout_secs == 0 {
            return Err(ToolError::Validation("timeout must be > 0".into()));
        }
        Ok(())
    }

    async fn invoke(params: BashParams, ctx: &ToolContext) -> ToolResult {
        let workdir = params.workdir.as_deref().unwrap_or(".");
        let resolved_dir = ctx
            .get::<WorkingDir>()
            .map(|wd| wd.0.as_path())
            .unwrap_or(std::path::Path::new(workdir));

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&params.command)
            .current_dir(resolved_dir)
            .output()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(ToolOutput::text(stdout).with_metadata(serde_json::json!({
                "exit_code": 0,
                "command": params.command,
            })))
        } else {
            Err(ToolError::Execution(format!(
                "exit {}: {stderr}",
                output.status.code().unwrap_or(-1)
            )))
        }
    }
}

// --- Step 3: Compose the middleware stack ---

fn build_registry(
    audit_log: Arc<AuditLog>,
    sandbox_config: SandboxConfig,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    let meta = BashTool::meta();

    let layered = Stack::new()
        .layer(SandboxLayer::new(sandbox_config))
        .layer(RateLimitLayer::new(5))
        .layer(AuditLayer::new(audit_log))
        .into_service();
        // Execution order: Audit → RateLimit → Sandbox → BashTool

    registry.register_service(meta, layered);

    registry
}

// --- Step 4: Use it ---

async fn example_invocation(registry: &ToolRegistry) {
    let mut ext = Extensions::new();
    ext.insert(WorkingDir("/tmp/project".into()));

    let ctx = ToolContext::new(ext);

    let result = registry
        .invoke(
            "bash",
            serde_json::json!({ "command": "cargo test", "timeout_secs": 120 }),
            ctx,
        )
        .await;

    match result {
        Ok(output) => println!("{}", output.text),
        Err(ToolError::RateLimited { retry_after }) => {
            println!("slow down, retry in {retry_after:?}");
        }
        Err(e) => println!("error: {e}"),
    }
}

// --- Step 5: MCP tool — same registry, same interface ---

async fn register_mcp_tool(registry: &mut ToolRegistry, client: Arc<dyn McpClient>) {
    let meta = ToolMeta {
        name: "mcp__filesystem__read".into(),
        description: "Read a file via MCP filesystem server".into(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
        permissions: Permissions {
            read_only: true,
            ..Default::default()
        },
    };

    let svc = McpToolService {
        client,
        tool_name: "read".into(),
    };

    registry.register_dynamic(
        meta,
        Stack::new()
            .layer(RateLimitLayer::new(10))
            .layer(AuditLayer::new(Arc::new(AuditLog {
                tx: tokio::sync::mpsc::unbounded_channel().0,
            })))
            .into_service(),
    );
}
