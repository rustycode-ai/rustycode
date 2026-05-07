//! Debug LSP communication

#![allow(clippy::unwrap_used, clippy::expect_used)]

use rustycode_lsp::{LspClient, LspClientConfig};

#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow test: run with --features slow-tests"
)]
#[tokio::test]
async fn test_lsp_process_status() {
    let mut client = LspClient::new(LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri: None,
        capabilities: lsp_types::ClientCapabilities::default(),
    });

    println!("Starting LSP...");
    let start_result = client.start().await;

    match start_result {
        Ok(()) => println!("LSP started successfully"),
        Err(e) => {
            println!("Failed to start LSP: {e}");
            return;
        }
    }

    println!("Is running: {}", client.is_running());

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("Shutting down...");
    let _ = client.shutdown().await;
    let _ = client.exit().await;

    println!("Done");
}
