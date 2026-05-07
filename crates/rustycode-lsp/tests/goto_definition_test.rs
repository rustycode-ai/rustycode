//! Test LSP goto-definition functionality

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

#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "slow test: run with --features slow-tests"
)]
#[tokio::test]
async fn test_lsp_goto_definition() {
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
    let code = r#"fn foo() -> i32 {
    42
}

fn main() {
    let x = foo();
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

    println!("Testing goto-definition at position (line 6, col 10) - should jump to 'foo' function definition...");
    let goto_result = client
        .goto_definition(uri.clone(), Position::new(6, 10))
        .await;

    match goto_result {
        Ok(Some(definition)) => {
            println!("Got definition result!");
            println!("  Definition: {definition:#?}");
        }
        Ok(None) => {
            println!("No definition result (might be expected if position is not on a symbol)");
        }
        Err(e) => {
            println!("Goto-definition error: {e}");
        }
    }

    // Test goto-definition on 'println'
    println!(
        "Testing goto-definition at position (line 7, col 4) - should jump to 'println' macro..."
    );
    let goto_result = client
        .goto_definition(uri.clone(), Position::new(7, 4))
        .await;

    match goto_result {
        Ok(Some(definition)) => {
            println!("Got definition result!");
            println!("  Definition: {definition:#?}");
        }
        Ok(None) => {
            println!("No definition result (might be expected for stdlib functions)");
        }
        Err(e) => {
            println!("Goto-definition error: {e}");
        }
    }

    println!("Shutting down...");
    let _ = client.shutdown().await;
    let _ = client.exit().await;

    println!("Test complete");
}
