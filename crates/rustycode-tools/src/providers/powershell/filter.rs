//! PowerShell-specific boilerplate output filtering.
//!
//! Filters PowerShell startup banners, prompts, and protocol artifacts
//! from command output.

/// PS-specific boilerplate lines to filter from output.
pub fn is_ps_boilerplate(trimmed: &str) -> bool {
    trimmed.starts_with("PowerShell")
        || trimmed.starts_with("Windows PowerShell")
        || trimmed.starts_with("PS ")
        || trimmed.contains("> Write-Host")
        || trimmed == ">>"
        // Delimiter and exit-code query echoed back
        || trimmed.contains("Write-Output '---END---'")
        || trimmed.contains("Write-Output $LASTEXITCODE")
}

pub fn filter_ps_boilerplate(text: &str) -> String {
    text.lines()
        .filter(|line| !is_ps_boilerplate(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ps_boilerplate() {
        assert!(is_ps_boilerplate("PowerShell 7.4.0"));
        assert!(is_ps_boilerplate("PS C:\\Users> "));
        assert!(is_ps_boilerplate("Windows PowerShell"));
        assert!(!is_ps_boilerplate("Hello, World!"));
        assert!(!is_ps_boilerplate("Get-Process"));
    }

    #[test]
    fn test_filter_ps_boilerplate_multiline() {
        let input = "PowerShell 7.4.0\nHello\nPS C:\\> \nWorld\nWrite-Output '---END---'";
        let filtered = filter_ps_boilerplate(input);
        assert_eq!(filtered, "Hello\nWorld");
    }
}
