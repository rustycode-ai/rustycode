use rustycode_lsp::{LspClient, LspClientConfig};
use lsp_types::Uri as Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = LspClientConfig::default();
    let mut client = LspClient::new(config);

    client.start().await?;

    // Create a temporary file to test with
    let path = std::env::temp_dir().join("test_file.rs");
    std::fs::write(&path, "fn main() { let x = 1; }")?;
    
    let uri = Url::from_file_path(path).unwrap();
    client.open_document(uri.clone(), "rust", 1, "fn main() { let x = 1; }").await?;

    // Get hover information
    let hover = client.hover(uri, lsp_types::Position::new(0, 12)).await?;
    println!("Hover result: {:?}", hover);

    client.shutdown().await?;
    client.exit().await?;
    Ok(())
}
