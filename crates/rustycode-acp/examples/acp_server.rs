#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
//! RustyCode ACP Server Binary
//!
//! Standalone ACP server for use with ACP-compatible clients like Zed, VS Code, etc.

use rustycode_acp::ACPServer;
use std::env;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("RustyCode ACP Server starting...");

    // Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    let cwd = if args.len() > 1 && args[1] == "--cwd" {
        if args.len() > 2 {
            Some(args[2].clone())
        } else {
            eprintln!("Usage: {} [--cwd <path>]", args[0]);
            std::process::exit(1);
        }
    } else {
        None
    };

    // Change to specified directory if provided
    if let Some(ref dir) = cwd {
        env::set_current_dir(dir)
            .map_err(|e| anyhow::anyhow!("Failed to change directory to {}: {}", dir, e))?;
        info!("Working directory: {}", dir);
    } else {
        // Use current directory
        let current_dir = env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
        info!("Working directory: {}", current_dir.display());
    }

    // Create and run server
    let mut server = ACPServer::new();
    server.run()?;

    Ok(())
}
