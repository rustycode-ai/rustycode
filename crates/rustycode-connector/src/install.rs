//! Connector Installation Helpers
//!
//! Provides detection and installation instructions for terminal connectors.

use std::fmt;
use std::process::{Command, Stdio};

/// Installation status for a connector
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallStatus {
    /// Connector is installed and ready to use
    Installed,
    /// Connector is not installed, but can be installed
    NotInstalled {
        install_command: String,
        install_instructions: String,
    },
    /// Connector cannot be installed on this platform
    Unsupported { reason: String },
    /// Connector binary found but API/service not available (e.g., iTerm2 not running)
    ServiceUnavailable {
        reason: String,
        setup_instructions: String,
    },
}

impl fmt::Display for InstallStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Installed => write!(f, "Installed"),
            Self::NotInstalled {
                install_command, ..
            } => {
                write!(f, "Not installed\n  Install command: {install_command}")
            }
            Self::Unsupported { reason } => {
                write!(f, "Unsupported: {reason}")
            }
            Self::ServiceUnavailable {
                reason,
                setup_instructions,
            } => {
                write!(
                    f,
                    "Service unavailable: {reason}\n  Setup: {setup_instructions}"
                )
            }
        }
    }
}

/// Information about a connector
#[derive(Debug, Clone)]
pub struct ConnectorInfo {
    /// Connector name
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Platform support
    pub platforms: &'static [&'static str],
    /// Current installation status
    pub status: InstallStatus,
    /// Performance tier (lower is better)
    pub performance_tier: u8,
}

impl fmt::Display for ConnectorInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.name)?;
        writeln!(f, "  Description: {}", self.description)?;
        writeln!(f, "  Platforms: {}", self.platforms.join(", "))?;
        writeln!(f, "  Performance tier: {}", self.performance_tier)?;
        writeln!(f, "  Status: {}", self.status)
    }
}

/// Check if tmux is installed
pub fn check_tmux() -> InstallStatus {
    let output = Command::new("which")
        .arg("tmux")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match output {
        Ok(s) if s.success() => InstallStatus::Installed,
        _ => InstallStatus::NotInstalled {
            install_command: "brew install tmux".to_string(),
            install_instructions: {
                #[cfg(target_os = "macos")]
                {
                    "Install Homebrew (brew.sh) then run: brew install tmux"
                }
                #[cfg(target_os = "linux")]
                {
                    "Ubuntu/Debian: sudo apt install tmux\n\
                     Fedora: sudo dnf install tmux\n\
                     Arch: sudo pacman -S tmux"
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    "tmux is not available on this platform"
                }
            }
            .to_string(),
        },
    }
}

/// Check if it2 CLI is installed
pub fn check_it2() -> InstallStatus {
    #[cfg(not(target_os = "macos"))]
    {
        InstallStatus::Unsupported {
            reason: "it2 CLI is only available on macOS".to_string(),
        }
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("which")
            .arg("it2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(s) if s.success() => InstallStatus::Installed,
            _ => InstallStatus::NotInstalled {
                install_command: "pip install it2-iterm2".to_string(),
                install_instructions: {
                    "Install via pip:\n\
                     pip install it2-iterm2\n\n\
                     Or from source: https://github.com/mkusaka/it2"
                }
                .to_string(),
            },
        }
    }
}

/// Check if iTerm2 is installed and API is available
pub fn check_iterm2_native() -> InstallStatus {
    #[cfg(not(target_os = "macos"))]
    {
        InstallStatus::Unsupported {
            reason: "iTerm2 is only available on macOS".to_string(),
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Check if iTerm2 application exists
        let iterm2_app = std::path::Path::new("/Applications/iTerm.app");
        let iterm2_app_alt = std::path::Path::new("/Applications/iTerm2.app");

        if !iterm2_app.exists() && !iterm2_app_alt.exists() {
            return InstallStatus::NotInstalled {
                install_command: "brew install --cask iterm2".to_string(),
                install_instructions: {
                    "Install iTerm2:\n\
                     1. Download from https://iterm2.com\n\
                     2. Or use Homebrew: brew install --cask iterm2\n\
                     3. Move to /Applications folder"
                }
                .to_string(),
            };
        }

        // Check if iTerm2 is running
        let output = Command::new("pgrep")
            .arg("-x")
            .arg("iTerm2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let is_running = output.is_ok_and(|s| s.success());

        if !is_running {
            return InstallStatus::ServiceUnavailable {
                reason: "iTerm2 is not running".to_string(),
                setup_instructions: {
                    "Start iTerm2 and enable the API:\n\
                     1. Open iTerm2\n\
                     2. Go to iTerm2 > Settings > General\n\
                     3. Check 'Enable API server'\n\
                     4. The API socket will be created automatically"
                }
                .to_string(),
            };
        }

        // Check if API socket exists
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let socket_dir = format!("{home}/Library/Application Support/iTerm2");

        if let Ok(entries) = std::fs::read_dir(&socket_dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("iterm2-socket")
                {
                    return InstallStatus::Installed;
                }
            }
        }

        // iTerm2 is running but socket not found - API might not be enabled
        InstallStatus::ServiceUnavailable {
            reason: "iTerm2 API socket not found".to_string(),
            setup_instructions: {
                "Enable the iTerm2 API:\n\
                 1. In iTerm2, go to Settings > General\n\
                 2. Check 'Enable API server'\n\
                 3. Restart iTerm2 if necessary"
            }
            .to_string(),
        }
    }
}

/// Check if iTerm2 (`AppleScript`) is available
pub fn check_iterm2_applescript() -> InstallStatus {
    #[cfg(not(target_os = "macos"))]
    {
        InstallStatus::Unsupported {
            reason: "iTerm2 is only available on macOS".to_string(),
        }
    }

    #[cfg(target_os = "macos")]
    {
        let iterm2_app = std::path::Path::new("/Applications/iTerm.app");
        let iterm2_app_alt = std::path::Path::new("/Applications/iTerm2.app");

        if !iterm2_app.exists() && !iterm2_app_alt.exists() {
            return InstallStatus::NotInstalled {
                install_command: "brew install --cask iterm2".to_string(),
                install_instructions: {
                    "Install iTerm2:\n\
                     1. Download from https://iterm2.com\n\
                     2. Or use Homebrew: brew install --cask iterm2\n\
                     3. Move to /Applications folder"
                }
                .to_string(),
            };
        }

        // Check if osascript is available
        let output = Command::new("which")
            .arg("osascript")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(s) if s.success() => InstallStatus::Installed,
            _ => InstallStatus::Unsupported {
                reason: "AppleScript (osascript) not available".to_string(),
            },
        }
    }
}

/// Get information about all connectors
pub fn get_all_connectors() -> Vec<ConnectorInfo> {
    vec![
        ConnectorInfo {
            name: "tmux",
            description: "Cross-platform terminal multiplexer with full API support",
            platforms: &["macOS", "Linux", "BSD"],
            status: check_tmux(),
            performance_tier: 1, // Best performance
        },
        ConnectorInfo {
            name: "iterm2-native",
            description: "iTerm2 via native Unix socket + Protobuf (fastest iTerm2 option)",
            platforms: &["macOS"],
            status: check_iterm2_native(),
            performance_tier: 2,
        },
        ConnectorInfo {
            name: "it2",
            description: "iTerm2 via Python CLI (slower but cross-platform within macOS)",
            platforms: &["macOS"],
            status: check_it2(),
            performance_tier: 4,
        },
        ConnectorInfo {
            name: "iterm2-applescript",
            description: "iTerm2 via AppleScript (limited API, no capture_output)",
            platforms: &["macOS"],
            status: check_iterm2_applescript(),
            performance_tier: 3,
        },
    ]
}

/// Print installation status for all connectors
pub fn print_connector_status() {
    println!("{}", "=".repeat(70));
    println!("TERMINAL CONNECTOR STATUS");
    println!("{}", "=".repeat(70));
    println!();

    let connectors = get_all_connectors();

    for connector in &connectors {
        let status_icon = match &connector.status {
            InstallStatus::Installed => "✅",
            InstallStatus::NotInstalled { .. } => "❌",
            InstallStatus::Unsupported { .. } => "⚠️",
            InstallStatus::ServiceUnavailable { .. } => "⏸️",
            #[allow(unreachable_patterns)]
            _ => "❓",
        };

        println!(
            "{} {} (Tier {})",
            status_icon, connector.name, connector.performance_tier
        );
        println!("   {}", connector.description);
        println!("   Status: {}", connector.status);
        println!();
    }

    println!("{}", "=".repeat(70));
    println!("RECOMMENDATIONS");
    println!("{}", "=".repeat(70));

    // Find installed connectors
    let installed: Vec<_> = connectors
        .iter()
        .filter(|c| matches!(c.status, InstallStatus::Installed))
        .collect();

    if installed.is_empty() {
        println!("\nNo connectors installed. Install tmux for best experience:");
        println!("  macOS:   brew install tmux");
        println!("  Ubuntu:  sudo apt install tmux");
        println!("  Fedora:  sudo dnf install tmux");
    } else {
        println!("\nInstalled connectors:");
        for c in &installed {
            println!("  - {} (Tier {})", c.name, c.performance_tier);
        }

        let best = installed.iter().min_by_key(|c| c.performance_tier);

        if let Some(best) = best {
            println!("\nRecommended: {} (best performance)", best.name);
        }
    }
}

/// Check if a specific connector is available and return installation help if not
pub fn check_connector(name: &str) -> Option<InstallStatus> {
    match name {
        "tmux" => Some(check_tmux()),
        "iterm2-native" | "iterm2" => Some(check_iterm2_native()),
        "it2" => Some(check_it2()),
        "iterm2-applescript" => Some(check_iterm2_applescript()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_tmux() {
        let status = check_tmux();
        println!("tmux status: {status:?}");
    }

    #[test]
    fn test_check_it2() {
        let status = check_it2();
        println!("it2 status: {status:?}");
    }

    #[test]
    fn test_check_iterm2_native() {
        let status = check_iterm2_native();
        println!("iterm2-native status: {status:?}");
    }

    #[test]
    fn test_get_all_connectors() {
        let connectors = get_all_connectors();
        assert_eq!(connectors.len(), 4);

        for connector in &connectors {
            println!("{connector}");
        }
    }

    #[test]
    fn test_install_status_display_installed() {
        let status = InstallStatus::Installed;
        assert_eq!(status.to_string(), "Installed");
    }

    #[test]
    fn test_install_status_display_not_installed() {
        let status = InstallStatus::NotInstalled {
            install_command: "brew install tmux".to_string(),
            install_instructions: "Use brew".to_string(),
        };
        let display = status.to_string();
        assert!(display.contains("Not installed"));
        assert!(display.contains("brew install tmux"));
    }

    #[test]
    fn test_install_status_display_unsupported() {
        let status = InstallStatus::Unsupported {
            reason: "Not available on Windows".to_string(),
        };
        let display = status.to_string();
        assert!(display.contains("Unsupported"));
        assert!(display.contains("Not available on Windows"));
    }

    #[test]
    fn test_install_status_display_service_unavailable() {
        let status = InstallStatus::ServiceUnavailable {
            reason: "iTerm2 not running".to_string(),
            setup_instructions: "Start iTerm2".to_string(),
        };
        let display = status.to_string();
        assert!(display.contains("Service unavailable"));
        assert!(display.contains("iTerm2 not running"));
        assert!(display.contains("Start iTerm2"));
    }

    #[test]
    fn test_install_status_equality() {
        assert_eq!(InstallStatus::Installed, InstallStatus::Installed);

        let a = InstallStatus::Unsupported { reason: "x".into() };
        let b = InstallStatus::Unsupported { reason: "x".into() };
        assert_eq!(a, b);

        let c = InstallStatus::Unsupported { reason: "y".into() };
        assert_ne!(a, c);
    }

    #[test]
    fn test_install_status_clone() {
        let status = InstallStatus::NotInstalled {
            install_command: "cmd".to_string(),
            install_instructions: "instr".to_string(),
        };
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_install_status_debug() {
        let status = InstallStatus::Installed;
        let debug = format!("{status:?}");
        assert!(debug.contains("Installed"));
    }

    #[test]
    fn test_connector_info_display() {
        let info = ConnectorInfo {
            name: "tmux",
            description: "A multiplexer",
            platforms: &["macOS", "Linux"],
            status: InstallStatus::Installed,
            performance_tier: 1,
        };
        let display = info.to_string();
        assert!(display.contains("tmux"));
        assert!(display.contains("A multiplexer"));
        assert!(display.contains("macOS, Linux"));
        assert!(display.contains("Performance tier: 1"));
    }

    #[test]
    fn test_connector_info_debug() {
        let info = ConnectorInfo {
            name: "test",
            description: "desc",
            platforms: &[],
            status: InstallStatus::Installed,
            performance_tier: 5,
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("ConnectorInfo"));
    }

    #[test]
    fn test_connector_info_clone() {
        let info = ConnectorInfo {
            name: "tmux",
            description: "multiplexer",
            platforms: &["macOS"],
            status: InstallStatus::Installed,
            performance_tier: 1,
        };
        let cloned = info;
        assert_eq!(cloned.name, "tmux");
        assert_eq!(cloned.performance_tier, 1);
    }

    #[test]
    fn test_get_all_connectors_names() {
        let connectors = get_all_connectors();
        let names: Vec<&str> = connectors.iter().map(|c| c.name).collect();
        assert!(names.contains(&"tmux"));
        assert!(names.contains(&"iterm2-native"));
        assert!(names.contains(&"it2"));
        assert!(names.contains(&"iterm2-applescript"));
    }

    #[test]
    fn test_get_all_connectors_performance_tiers() {
        let connectors = get_all_connectors();
        for connector in &connectors {
            assert!(
                connector.performance_tier > 0,
                "Performance tier should be > 0 for {}",
                connector.name
            );
            assert!(
                connector.performance_tier <= 10,
                "Performance tier should be <= 10 for {}",
                connector.name
            );
        }
    }

    #[test]
    fn test_check_connector_known_names() {
        // All these should return Some
        assert!(check_connector("tmux").is_some());
        assert!(check_connector("iterm2-native").is_some());
        assert!(check_connector("iterm2").is_some());
        assert!(check_connector("it2").is_some());
        assert!(check_connector("iterm2-applescript").is_some());
    }

    #[test]
    fn test_check_connector_unknown_name() {
        assert!(check_connector("nonexistent").is_none());
        assert!(check_connector("").is_none());
        assert!(check_connector("TMUX").is_none()); // Case-sensitive
    }

    #[test]
    fn test_print_connector_status_does_not_panic() {
        // Just verify it doesn't panic
        print_connector_status();
    }

    #[test]
    fn test_check_tmux_returns_valid_status() {
        let status = check_tmux();
        // Verify it's one of the expected statuses
        match &status {
            InstallStatus::Installed => {}
            InstallStatus::NotInstalled {
                install_command,
                install_instructions,
            } => {
                assert!(!install_command.is_empty());
                assert!(!install_instructions.is_empty());
            }
            _ => {}
        }
    }

    #[test]
    fn test_check_it2_returns_valid_status() {
        let status = check_it2();
        // On non-macOS this should be Unsupported; on macOS it varies
        match &status {
            InstallStatus::Installed => {}
            InstallStatus::NotInstalled {
                install_command, ..
            } => {
                assert!(install_command.contains("pip"));
            }
            InstallStatus::Unsupported { .. } => {}
            _ => {}
        }
    }

    #[test]
    fn test_install_status_not_installed_fields() {
        let status = InstallStatus::NotInstalled {
            install_command: "cmd".into(),
            install_instructions: "steps".into(),
        };
        if let InstallStatus::NotInstalled {
            install_command,
            install_instructions,
        } = &status
        {
            assert_eq!(install_command, "cmd");
            assert_eq!(install_instructions, "steps");
        } else {
            panic!("Expected NotInstalled");
        }
    }

    #[test]
    fn test_install_status_service_unavailable_fields() {
        let status = InstallStatus::ServiceUnavailable {
            reason: "r".into(),
            setup_instructions: "s".into(),
        };
        if let InstallStatus::ServiceUnavailable {
            reason,
            setup_instructions,
        } = &status
        {
            assert_eq!(reason, "r");
            assert_eq!(setup_instructions, "s");
        } else {
            panic!("Expected ServiceUnavailable");
        }
    }
}
