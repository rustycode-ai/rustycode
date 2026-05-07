//! Test LSP hover functionality

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lsp_types::{Position, Uri as Url};
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

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[tokio::test]
async fn test_lsp_hover() {
    // Create a temporary Rust project
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    std::fs::write(
        project_dir.join("Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    let src_dir = project_dir.join("src");
    std::fs::create_dir(&src_dir).unwrap();

    let main_rs = src_dir.join("main.rs");
    let code = r#"fn main() {
    let x = 42;
    println!("{}", x);
}"#;
    std::fs::write(&main_rs, code).unwrap();

    let uri = uri_from_file_path(&main_rs);
    let root_uri = Some(uri_from_directory_path(project_dir).to_string());

    let mut client = LspClient::new(LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri,
        capabilities: lsp_types::ClientCapabilities::default(),
    });

    println!("Starting LSP...");
    if client.start().await.is_err() {
        println!("rust-analyzer not available, skipping");
        return;
    }

    println!("Opening document...");
    if client
        .open_document(uri.clone(), "rust", 1, code)
        .await
        .is_err()
    {
        println!("Failed to open document");
        return;
    }

    // Wait for rust-analyzer to initialize
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    println!("Testing hover at position (line 2, col 8) - should be on 'x' variable...");
    let hover_result = client.hover(uri.clone(), Position::new(2, 8)).await;

    match hover_result {
        Ok(Some(hover)) => {
            println!("Got hover result!");
            println!("  Full Hover struct: {hover:#?}");
        }
        Ok(None) => {
            println!("No hover result (might be expected if position is not on a symbol)");
        }
        Err(e) => {
            println!("Hover error: {e}");
        }
    }

    // Test hovering on 'println'
    println!("Testing hover at position (line 3, col 4) - should be on 'println' macro...");
    let hover_result = client.hover(uri.clone(), Position::new(3, 4)).await;

    match hover_result {
        Ok(Some(hover)) => {
            println!("Got hover result!");
            println!("  Full Hover struct: {hover:#?}");
        }
        Ok(None) => {
            println!("No hover result");
        }
        Err(e) => {
            println!("Hover error: {e}");
        }
    }

    println!("Shutting down...");
    let _ = client.shutdown().await;
    let _ = client.exit().await;

    println!("Test complete");
}
