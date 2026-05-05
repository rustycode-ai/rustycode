use lsp_types::Uri as Url;
use rustycode_lsp::{LspClient, LspClientConfig};
use url::Url as FileUrl;

#[allow(clippy::unwrap_used)]
fn uri_from_file_path(path: impl AsRef<std::path::Path>) -> Url {
    FileUrl::from_file_path(path)
        .unwrap()
        .as_str()
        .parse()
        .unwrap()
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn test_lsp_active_verification() {
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

    // Create a temporary directory with a proper Rust project structure
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_dir = temp_dir.path();
    std::fs::write(
        project_dir.join("Cargo.toml"),
        r#"[package]
name = "test-verify"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    let src_dir = project_dir.join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let file_path = src_dir.join("main.rs");
    let text = "fn main() { let x: i32 = \"invalid\"; }";
    std::fs::write(&file_path, text).unwrap();

    let uri = uri_from_file_path(&file_path);

    // Set root_uri in config
    let config = LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri: Some(
            FileUrl::from_directory_path(project_dir)
                .unwrap()
                .to_string(),
        ),
        capabilities: lsp_types::ClientCapabilities::default(),
    };

    let mut client = LspClient::new(config);
    if client.start().await.is_err() {
        return;
    }

    // Open document
    client
        .open_document(uri.clone(), "rust", 1, text)
        .await
        .unwrap();

    // Trigger active verification
    let result = client.validate_document(uri.clone()).await;
    assert!(result.is_ok(), "validate_document should succeed");

    // Poll for diagnostics with a timeout
    let mut diagnostics = Vec::new();
    for _ in 0..10 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        diagnostics = client.fetch_diagnostics(&uri).await;
        if !diagnostics.is_empty() {
            break;
        }
    }
    assert!(
        !diagnostics.is_empty(),
        "Should have received diagnostics after validate_document"
    );

    let _ = client.shutdown().await;
    let _ = client.exit().await;
}
