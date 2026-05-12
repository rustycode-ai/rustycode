//! PowerShell command protocol and binary extraction.
//!
//! Handles command parsing and binary/cmdlet name extraction for
//! security validation and logging.

/// Extract the binary/command name from a PowerShell command string.
/// Handles both cmdlet names (`Get-ChildItem`) and external binaries (`git`).
pub fn extract_binary_name(command: &str) -> anyhow::Result<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("empty command"));
    }

    // PowerShell commands often start with a cmdlet like Get-Something
    // or an alias like gci, ls. Split on first whitespace or pipe.
    let first_token = trimmed
        .split(|c: char| c.is_whitespace() || c == '|' || c == ';')
        .next()
        .unwrap_or(trimmed);

    // Strip any path components
    let name = if first_token.contains('/') {
        first_token.rsplit('/').next().unwrap_or(first_token)
    } else if first_token.contains('\\') {
        first_token.rsplit('\\').next().unwrap_or(first_token)
    } else {
        first_token
    };

    Ok(name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_binary_name_cmdlet() {
        assert_eq!(
            extract_binary_name("Get-ChildItem -Path .").unwrap(),
            "get-childitem"
        );
    }

    #[test]
    fn test_extract_binary_name_alias() {
        assert_eq!(extract_binary_name("gci -Path .").unwrap(), "gci");
    }

    #[test]
    fn test_extract_binary_name_external() {
        assert_eq!(extract_binary_name("git status").unwrap(), "git");
    }

    #[test]
    fn test_extract_binary_name_pipe() {
        assert_eq!(
            extract_binary_name("Get-Process | Where-Object { $_.CPU -gt 100 }").unwrap(),
            "get-process"
        );
    }

    #[test]
    fn test_extract_binary_name_path() {
        assert_eq!(
            extract_binary_name("/usr/bin/python3 -c 'hello'").unwrap(),
            "python3"
        );
    }

    #[test]
    fn test_extract_binary_name_empty() {
        assert!(extract_binary_name("").is_err());
        assert!(extract_binary_name("   ").is_err());
    }
}
