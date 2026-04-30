//! LSP (Language Server Protocol) slash commands
//!
//! Provides commands for inspecting LSP server availability.

pub async fn handle_lsp_command(input: &str) -> Result<Option<String>, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.len() < 2 {
        return Ok(Some(lsp_help()));
    }

    let subcommand = parts[1];

    match subcommand {
        "help" | "" => Ok(Some(lsp_help())),
        "status" => handle_lsp_status().await,
        "start" => Ok(Some(
            "LSP servers start automatically when needed. Use /lsp status to check availability."
                .to_string(),
        )),
        "stop" => Ok(Some(
            "LSP server lifecycle is managed automatically.".to_string(),
        )),
        _ => Ok(Some(format!(
            "Unknown LSP command: {}\n\n{}",
            subcommand,
            lsp_help()
        ))),
    }
}

async fn handle_lsp_status() -> Result<Option<String>, String> {
    let candidates = rustycode_lsp::default_servers();
    let statuses = rustycode_lsp::discover(&candidates);

    let installed: Vec<_> = statuses.iter().filter(|s| s.installed).collect();
    let missing: Vec<_> = statuses.iter().filter(|s| !s.installed).collect();

    let mut output = String::from("LSP Server Status\n\n");

    if installed.is_empty() {
        output.push_str("No LSP servers found on PATH.\n\n");
    } else {
        output.push_str(&format!("Available ({}):\n", installed.len()));
        for status in &installed {
            let path = status.path.as_deref().unwrap_or("unknown path");
            output.push_str(&format!("  ✓ {} ({})\n", status.name, path));
        }
        output.push('\n');
    }

    if !missing.is_empty() {
        output.push_str(&format!("Not installed ({}):\n", missing.len()));
        for status in &missing {
            output.push_str(&format!("  ✗ {}\n", status.name));
        }
        output.push('\n');
    }

    output.push_str("Use /lsp help for available commands.");

    Ok(Some(output))
}

fn lsp_help() -> String {
    "LSP (Language Server Protocol) - Code Intelligence Servers\n\
    \n\
    Commands:\n\
    \n\
    * /lsp status   - Show discovered and available LSP servers\n\
    * /lsp start    - (automatic) LSP servers start when needed\n\
    * /lsp stop     - (automatic) LSP servers are managed by RustyCode\n\
    * /lsp help     - Show this help text\n\
    \n\
    LSP servers provide hover info, goto-definition, completions,\n\
    diagnostics, and more. RustyCode auto-detects your project's\n\
    language and starts the appropriate server."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_help_returns_usage() {
        let help = lsp_help();
        assert!(!help.is_empty());
        assert!(help.contains("LSP (Language Server Protocol)"));
        assert!(help.contains("/lsp status"));
        assert!(help.contains("/lsp start"));
        assert!(help.contains("/lsp stop"));
        assert!(help.contains("/lsp help"));
    }

    #[test]
    fn test_lsp_help_not_empty() {
        let help = lsp_help();
        assert!(help.len() > 50);
    }

    #[tokio::test]
    async fn test_lsp_command_help_subcommand() {
        let result = handle_lsp_command("/lsp help").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("LSP (Language Server Protocol)"));
    }

    #[tokio::test]
    async fn test_lsp_command_empty_returns_help() {
        let result = handle_lsp_command("/lsp").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("LSP (Language Server Protocol)"));
    }

    #[tokio::test]
    async fn test_lsp_command_unknown_returns_error() {
        let result = handle_lsp_command("/lsp unknown_cmd").await.unwrap();
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.contains("Unknown LSP command: unknown_cmd"));
        assert!(output.contains("LSP (Language Server Protocol)"));
    }

    #[tokio::test]
    async fn test_lsp_status_shows_discovered() {
        let result = handle_lsp_status().await.unwrap();
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.contains("LSP Server Status"));
        assert!(output.contains("/lsp help"));
    }

    #[tokio::test]
    async fn test_lsp_status_lists_servers() {
        let result = handle_lsp_command("/lsp status").await.unwrap();
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.contains("LSP Server Status"));
    }

    #[tokio::test]
    async fn test_lsp_start_placeholder() {
        let result = handle_lsp_command("/lsp start").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("start automatically"));
    }

    #[tokio::test]
    async fn test_lsp_stop_placeholder() {
        let result = handle_lsp_command("/lsp stop").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("managed automatically"));
    }
}
