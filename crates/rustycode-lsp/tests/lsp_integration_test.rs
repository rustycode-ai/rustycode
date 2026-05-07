#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::if_not_else,
    clippy::map_unwrap_or,
    clippy::uninlined_format_args
)]

//! Integration tests for LSP client functionality
//!
//! These tests require an LSP server to be installed (e.g., rust-analyzer)

use lsp_types::Uri as Url;
use rustycode_lsp::{LspClient, LspClientConfig};
use url::Url as FileUrl;

fn uri_from_file_path(path: impl AsRef<std::path::Path>) -> Url {
    FileUrl::from_file_path(path)
        .unwrap()
        .as_str()
        .parse()
        .unwrap()
}

fn uri_from_directory_path(path: impl AsRef<std::path::Path>) -> Url {
    FileUrl::from_directory_path(path)
        .unwrap()
        .as_str()
        .parse()
        .unwrap()
}

#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow test: run with --features slow-tests"
)]
#[tokio::test]
async fn test_lsp_client_lifecycle() {
    // Check if rust-analyzer is available
    let config = LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri: None,
        capabilities: lsp_types::ClientCapabilities::default(),
    };

    let mut client = LspClient::new(config);

    // Test initial state
    assert!(!client.is_running());
    assert!(client.server_capabilities().is_none());

    // Start the server
    let start_result = client.start().await;
    if start_result.is_err() {
        println!("rust-analyzer not available, skipping test");
        return;
    }

    assert!(client.is_running());

    // Shutdown and exit
    let shutdown_result = client.shutdown().await;
    assert!(shutdown_result.is_ok());

    let exit_result = client.exit().await;
    assert!(exit_result.is_ok());
    assert!(!client.is_running());
}

#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow test: run with --features slow-tests"
)]
#[tokio::test]
async fn test_lsp_open_document() {
    let config = LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri: None,
        capabilities: lsp_types::ClientCapabilities::default(),
    };

    let mut client = LspClient::new(config);

    if client.start().await.is_err() {
        println!("rust-analyzer not available, skipping test");
        return;
    }

    // Open a document
    let uri = uri_from_file_path("/tmp/test.rs");
    let text = r#"fn main() {
    println!("Hello, world!");
}"#;

    let open_result = client.open_document(uri.clone(), "rust", 1, text).await;
    assert!(open_result.is_ok());

    // Check diagnostics (may be empty)
    let _diagnostics = client.fetch_diagnostics(&uri).await;
    // We don't assert on diagnostics content since rust-analyzer may not have finished analysis

    // Test sending a notification
    let change_result = client
        .change_document(
            uri.clone(),
            2,
            vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: r#"fn main() {
    println!("Hello, updated world!");
}"#
                .to_string(),
            }],
        )
        .await;
    assert!(change_result.is_ok(), "change_document should succeed");

    // Give rust-analyzer time to publish diagnostics
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Check if we received any diagnostics
    let diagnostics = client.fetch_diagnostics(&uri).await;
    println!("Received {} diagnostics for test.rs", diagnostics.len());

    // Cleanup
    let _ = client.shutdown().await;
    let _ = client.exit().await;
}

#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow test: run with --features slow-tests"
)]
#[tokio::test]
async fn test_lsp_shutdown_sequence() {
    let config = LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri: None,
        capabilities: lsp_types::ClientCapabilities::default(),
    };

    let mut client = LspClient::new(config);

    if client.start().await.is_err() {
        println!("rust-analyzer not available, skipping test");
        return;
    }

    // Verify client is running
    assert!(client.is_running(), "Client should be running after start");

    // Test proper shutdown sequence
    let shutdown_result = client.shutdown().await;
    assert!(shutdown_result.is_ok(), "Shutdown should succeed");

    let exit_result = client.exit().await;
    assert!(exit_result.is_ok(), "Exit should succeed");

    assert!(
        !client.is_running(),
        "Client should not be running after exit"
    );
}

#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow test: run with --features slow-tests"
)]
#[tokio::test]
async fn test_lsp_diagnostics_flow() {
    // Create a temporary directory with a proper Rust project structure
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create Cargo.toml
    let cargo_toml_content = r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;

    let cargo_toml_path = project_dir.join("Cargo.toml");
    std::fs::write(&cargo_toml_path, cargo_toml_content).unwrap();

    // Create src directory
    let src_dir = project_dir.join("src");
    std::fs::create_dir(&src_dir).unwrap();

    // Create main.rs with errors
    let main_rs_path = src_dir.join("main.rs");
    let text = r#"fn main() {
    let x: i32 = "hello";  // Type error
    unknown_function();     // Undefined function
    println!("Done");
}"#;

    std::fs::write(&main_rs_path, text).unwrap();

    let uri = uri_from_file_path(&main_rs_path);

    // Get root URI for the workspace
    let root_uri = Some(uri_from_directory_path(project_dir));

    let config = LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri: root_uri.map(|u| u.to_string()),
        capabilities: lsp_types::ClientCapabilities::default(),
    };

    let mut client = LspClient::new(config);

    if client.start().await.is_err() {
        println!("rust-analyzer not available, skipping test");
        return;
    }

    // Open the document
    let open_result = client.open_document(uri.clone(), "rust", 1, text).await;
    assert!(open_result.is_ok(), "open_document should succeed");

    // Give rust-analyzer more time to analyze and publish diagnostics
    // (First time analyzing a project can take longer)
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Check diagnostics
    let diagnostics = client.fetch_diagnostics(&uri).await;
    println!(
        "Received {} diagnostics for file with errors",
        diagnostics.len()
    );

    // We expect at least some diagnostics for the type error and undefined function
    if !diagnostics.is_empty() {
        println!("Found diagnostics:");
        for diag in &diagnostics {
            let severity = diag
                .severity
                .map_or_else(|| "None".to_string(), |s| format!("{s:?}"));
            println!("  - {severity}: {}", diag.message);
        }
    } else {
        println!("No diagnostics received - this might be expected if rust-analyzer is still initializing");
    }

    // Cleanup
    let _ = client.shutdown().await;
    let _ = client.exit().await;
    // Keep temp dir for inspection if needed
    let _ = temp_dir.keep();
}
