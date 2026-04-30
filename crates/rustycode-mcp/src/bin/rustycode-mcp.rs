use clap::Parser;
use rustycode_mcp::{
    build_lsp_tool_executor, McpServer, McpServerConfig, TerminalBackendKind, TmuxMcpConfig,
    TmuxMcpServer,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "rustycode-mcp",
    about = "RustyCode MCP server with terminal backend support"
)]
struct Cli {
    /// Terminal backend to use.
    #[arg(long, value_enum, default_value_t = TerminalBackendKind::Auto)]
    backend: TerminalBackendKind,

    /// Workspace root used for workspace.exec and related tools.
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,

    /// Prefix used when naming leased terminal sessions.
    #[arg(long, default_value = "rustycode")]
    session_prefix: String,

    /// Default lease TTL in seconds.
    #[arg(long, default_value_t = 3600)]
    default_ttl_secs: u64,

    /// Maximum number of lines captured from panes.
    #[arg(long, default_value_t = 200)]
    capture_lines: usize,

    /// Timeout for workspace.exec commands in seconds.
    #[arg(long, default_value_t = 300)]
    command_timeout_secs: u64,

    /// MCP server name.
    #[arg(long, default_value = "rustycode-mcp-server")]
    server_name: String,

    /// MCP server version string.
    #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
    server_version: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,rustycode_mcp=debug")
        .init();

    let cli = Cli::parse();
    let tmux_config = TmuxMcpConfig {
        workspace_root: cli.workspace_root,
        session_prefix: cli.session_prefix,
        preferred_backend: cli.backend,
        default_ttl_secs: cli.default_ttl_secs,
        capture_lines: cli.capture_lines,
        command_timeout_secs: cli.command_timeout_secs,
    };
    let workspace_root = tmux_config.workspace_root.clone();

    let server_config = McpServerConfig {
        server_name: cli.server_name,
        server_version: cli.server_version,
        enable_tools: true,
        enable_resources: true,
        enable_prompts: false,
        timeout_secs: cli.command_timeout_secs,
    };

    let mut server = McpServer::new("rustycode-mcp", server_config);
    server.register_tool_executor(build_lsp_tool_executor(workspace_root));
    let terminal_server = TmuxMcpServer::auto(tmux_config);
    terminal_server.register_into(&server).await?;

    server.run_stdio().await?;
    Ok(())
}
