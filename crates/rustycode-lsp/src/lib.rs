#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! `RustyCode` LSP Integration (see [`LspClient`] for the main entry point)
//!
//! Provides Language Server Protocol client support including:
//! - LSP server discovery and status checking
//! - Full LSP client implementation for communicating with language servers
//! - Support for diagnostics, hover, goto-definition, completion, and more
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_lsp::{LspClient, LspClientConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = LspClientConfig::default();
//!     let mut client = LspClient::new(config);
//!
//!     client.start().await?;
//!
//!     // Open a document
//!     let uri: lsp_types::Uri = "file:///path/to/file.rs".parse().unwrap();
//!     client.open_document(uri.clone(), "rust", 1, "fn main() {}").await?;
//!
//!     // Get hover information
//!     let hover = client.hover(uri, lsp_types::Position::new(0, 0)).await?;
//!
//!     client.shutdown().await?;
//!     client.exit().await?;
//!     Ok(())
//! }
//! ```

use serde::Serialize;

pub mod client;
pub mod detect;
pub mod transport;
pub mod types;

pub use client::{
    create_client_config, create_client_config_for_language, create_client_config_with_override,
    LspClient, LspClientConfig, LspClientState,
};
pub use detect::{BuildSystem, ProjectDetector, ProjectToolDetection};
pub use types::{LanguageId, LspConfig, LspServerConfig};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LspServerStatus {
    pub name: String,
    pub installed: bool,
    pub path: Option<String>,
}

pub fn default_servers() -> Vec<String> {
    vec![
        "rust-analyzer".to_string(),
        "typescript-language-server".to_string(),
        "pyright-langserver".to_string(),
        "gopls".to_string(),
        "clangd".to_string(),
        "jdtls".to_string(),
        "solargraph".to_string(),
        "phpactor".to_string(),
    ]
}

pub fn discover(candidates: &[String]) -> Vec<LspServerStatus> {
    candidates
        .iter()
        .map(|name| {
            which::which(name).map_or_else(
                |_| LspServerStatus {
                    name: name.clone(),
                    installed: false,
                    path: None,
                },
                |path| LspServerStatus {
                    name: name.clone(),
                    installed: true,
                    path: Some(path.display().to_string()),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_rust_analyzer() {
        let servers = vec!["rust-analyzer".to_string()];
        let statuses = discover(&servers);

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "rust-analyzer");

        if statuses[0].installed {
            println!("✓ rust-analyzer found at: {:?}", statuses[0].path);
        } else {
            println!("✗ rust-analyzer not found");
        }
    }

    #[test]
    fn test_discover_multiple_servers() {
        let servers = vec![
            "rust-analyzer".to_string(),
            "typescript-language-server".to_string(),
            "pyright-langserver".to_string(),
        ];

        let statuses = discover(&servers);

        println!("\nLSP Server Discovery Results:");
        for status in &statuses {
            let status_str = if status.installed { "✓" } else { "✗" };
            let path = status.path.as_deref().unwrap_or("Not found");
            println!("  {} {}: {}", status_str, status.name, path);
        }

        assert_eq!(statuses.len(), 3);
    }

    #[test]
    fn test_default_servers_contains_key_languages() {
        let servers = default_servers();
        assert!(servers.contains(&"rust-analyzer".to_string()));
        assert!(servers.contains(&"typescript-language-server".to_string()));
        assert!(servers.contains(&"pyright-langserver".to_string()));
    }

    #[test]
    fn test_discover_nonexistent_server() {
        let servers = vec!["nonexistent-lsp-server-xyz".to_string()];
        let statuses = discover(&servers);
        assert_eq!(statuses.len(), 1);
        assert!(!statuses[0].installed);
        assert!(statuses[0].path.is_none());
    }

    #[test]
    fn test_lsp_server_status_serialization() {
        let status = LspServerStatus {
            name: "test-server".to_string(),
            installed: true,
            path: Some("/usr/bin/test-server".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("test-server"));
        assert!(json.contains("/usr/bin/test-server"));
    }

    #[test]
    fn test_lsp_server_status_not_installed() {
        let status = LspServerStatus {
            name: "missing".to_string(),
            installed: false,
            path: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"installed\":false"));
    }

    #[test]
    fn test_discover_empty_list() {
        let statuses = discover(&[]);
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_lsp_server_status_roundtrip() {
        let status = LspServerStatus {
            name: "test-lsp".to_string(),
            installed: true,
            path: Some("/usr/local/bin/test-lsp".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        let decoded: LspServerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "test-lsp");
        assert!(decoded.installed);
        assert_eq!(decoded.path, Some("/usr/local/bin/test-lsp".to_string()));
    }

    #[test]
    fn test_default_servers_count() {
        let servers = default_servers();
        assert_eq!(servers.len(), 8);
    }

    #[test]
    fn test_discover_mixed_servers() {
        let servers = vec![
            "nonexistent-abc-xyz".to_string(),
            "also-nonexistent-def".to_string(),
        ];
        let statuses = discover(&servers);
        assert_eq!(statuses.len(), 2);
        assert!(!statuses[0].installed);
        assert!(!statuses[1].installed);
    }

    #[test]
    fn test_lsp_server_status_deserialization() {
        let json = r#"{"name":"test","installed":true,"path":"/usr/bin/test"}"#;
        let status: LspServerStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.name, "test");
        assert!(status.installed);
        assert_eq!(status.path, Some("/usr/bin/test".to_string()));
    }

    #[test]
    fn test_lsp_server_status_deserialization_null_path() {
        let json = r#"{"name":"test","installed":false,"path":null}"#;
        let status: LspServerStatus = serde_json::from_str(json).unwrap();
        assert!(!status.installed);
        assert!(status.path.is_none());
    }

    #[test]
    fn test_lsp_server_status_debug_format() {
        let status = LspServerStatus {
            name: "test".to_string(),
            installed: false,
            path: None,
        };
        let debug_str = format!("{status:?}");
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_discover_preserves_order() {
        let servers = vec![
            "nonexistent-zzz".to_string(),
            "nonexistent-aaa".to_string(),
            "nonexistent-mmm".to_string(),
        ];
        let statuses = discover(&servers);
        assert_eq!(statuses[0].name, "nonexistent-zzz");
        assert_eq!(statuses[1].name, "nonexistent-aaa");
        assert_eq!(statuses[2].name, "nonexistent-mmm");
    }

    #[test]
    fn test_discover_duplicate_names() {
        let servers = vec!["nonexistent-dup".to_string(), "nonexistent-dup".to_string()];
        let statuses = discover(&servers);
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].name, statuses[1].name);
    }

    #[test]
    fn test_lsp_server_status_clone() {
        let status = LspServerStatus {
            name: "cloned".to_string(),
            installed: true,
            path: Some("/path".to_string()),
        };
        let cloned = status.clone();
        assert_eq!(cloned.name, status.name);
        assert_eq!(cloned.installed, status.installed);
        assert_eq!(cloned.path, status.path);
    }

    #[test]
    fn test_default_servers_returns_strings() {
        let servers = default_servers();
        for server in &servers {
            assert!(!server.is_empty());
        }
    }
}
