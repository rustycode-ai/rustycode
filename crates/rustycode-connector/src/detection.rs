//! Terminal Detection and Connector Availability
//!
//! Provides functions for detecting the current terminal environment
//! and finding the best available connector.

use crate::{
    DetectedConnector, ITerm2NativeConnector, ITermConnector, It2Connector, TmuxConnector,
};

/// Detected terminal type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TerminalType {
    /// Apple Terminal (macOS default)
    AppleTerminal,
    /// iTerm2 (macOS)
    ITerm2,
    /// tmux multiplexer
    Tmux,
    /// `WezTerm` terminal
    WezTerm,
    /// Alacritty terminal
    Alacritty,
    /// Kitty terminal
    Kitty,
    /// Ghostty terminal
    Ghostty,
    /// Windows Terminal
    WindowsTerminal,
    /// VS Code integrated terminal
    VSCodeTerminal,
    /// `JetBrains` IDE terminal
    JetBrainsTerminal,
    /// Warp terminal
    Warp,
    /// Hyper terminal
    Hyper,
    /// Unknown/other terminal
    Unknown,
}

impl std::fmt::Display for TerminalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AppleTerminal => write!(f, "Apple Terminal"),
            Self::ITerm2 => write!(f, "iTerm2"),
            Self::Tmux => write!(f, "tmux"),
            Self::WezTerm => write!(f, "WezTerm"),
            Self::Alacritty => write!(f, "Alacritty"),
            Self::Kitty => write!(f, "Kitty"),
            Self::Ghostty => write!(f, "Ghostty"),
            Self::WindowsTerminal => write!(f, "Windows Terminal"),
            Self::VSCodeTerminal => write!(f, "VS Code Terminal"),
            Self::JetBrainsTerminal => write!(f, "JetBrains Terminal"),
            Self::Warp => write!(f, "Warp"),
            Self::Hyper => write!(f, "Hyper"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Information about available connectors
#[derive(Debug, Clone)]
pub struct ConnectorAvailability {
    /// The detected terminal type
    pub terminal_type: TerminalType,
    pub tmux_available: bool,
    /// Whether iTerm2 (`AppleScript`) is available (macOS only)
    pub iterm2_applescript_available: bool,
    /// Whether it2 CLI is available (macOS only)
    pub it2_available: bool,
    /// Whether iTerm2 native API is available (macOS only)
    pub iterm2_native_available: bool,
    /// Whether we're inside a multiplexer (tmux, screen, zellij)
    pub inside_multiplexer: bool,
    /// The recommended connector to use
    pub recommended: Option<ConnectorRecommendation>,
}

/// Connector recommendation with priority
#[derive(Debug, Clone)]
pub struct ConnectorRecommendation {
    /// Connector name
    pub name: &'static str,
    /// Reason for recommendation
    pub reason: &'static str,
    /// Priority (higher = better)
    pub priority: u8,
}

/// Detect the current terminal type
pub fn detect_terminal() -> TerminalType {
    // Check TERM_PROGRAM first (most reliable on macOS)
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        let lower = term_program.to_lowercase();
        if lower.contains("iterm") {
            return TerminalType::ITerm2;
        }
        if lower.contains("apple_terminal") {
            return TerminalType::AppleTerminal;
        }
        if lower.contains("vscode") {
            return TerminalType::VSCodeTerminal;
        }
        if lower.contains("warp") {
            return TerminalType::Warp;
        }
        if lower.contains("hyper") {
            return TerminalType::Hyper;
        }
    }

    // Check TERMINAL_EMULATOR (used by some IDEs)
    if let Ok(emulator) = std::env::var("TERMINAL_EMULATOR") {
        let lower = emulator.to_lowercase();
        if lower.contains("jetbrains") {
            return TerminalType::JetBrainsTerminal;
        }
        if lower.contains("vscode") {
            return TerminalType::VSCodeTerminal;
        }
    }

    // Check TERM environment variable
    if let Ok(term) = std::env::var("TERM") {
        let lower = term.to_lowercase();
        if lower.contains("kitty") {
            return TerminalType::Kitty;
        }
        if lower.contains("alacritty") {
            return TerminalType::Alacritty;
        }
        if lower.contains("wezterm") {
            return TerminalType::WezTerm;
        }
        if lower.contains("ghostty") {
            return TerminalType::Ghostty;
        }
    }

    // Check for tmux (we're inside a tmux session)
    if std::env::var("TMUX").is_ok() {
        return TerminalType::Tmux;
    }

    // Check for Windows Terminal
    if std::env::var("WT_SESSION").is_ok() {
        return TerminalType::WindowsTerminal;
    }

    TerminalType::Unknown
}

/// Check if we're inside a terminal multiplexer
pub fn inside_multiplexer() -> bool {
    // tmux
    if std::env::var("TMUX").is_ok() {
        return true;
    }

    // GNU Screen
    if std::env::var("STY").is_ok() {
        return true;
    }

    // zellij
    if std::env::var("ZELLIJ").is_ok() {
        return true;
    }

    false
}

/// Get connector availability information
pub fn connector_availability() -> ConnectorAvailability {
    let terminal_type = detect_terminal();
    let tmux_available = TmuxConnector::check_available();
    let it2_available = It2Connector::check_available();
    let iterm2_native_available = ITerm2NativeConnector::check_available();

    #[cfg(target_os = "macos")]
    let iterm2_applescript_available = ITermConnector::is_available();
    #[cfg(not(target_os = "macos"))]
    let iterm2_applescript_available = false;

    let inside_mux = inside_multiplexer();

    // Determine recommendation
    let recommended = determine_best_connector(
        terminal_type,
        tmux_available,
        it2_available,
        iterm2_native_available,
        iterm2_applescript_available,
        inside_mux,
    );

    ConnectorAvailability {
        terminal_type,
        tmux_available,
        iterm2_applescript_available,
        it2_available,
        iterm2_native_available,
        inside_multiplexer: inside_mux,
        recommended,
    }
}

/// Determine the best connector to use
#[allow(unused_variables)]
fn determine_best_connector(
    terminal_type: TerminalType,
    tmux_available: bool,
    it2_available: bool,
    iterm2_native_available: bool,
    iterm2_applescript_available: bool,
    inside_mux: bool,
) -> Option<ConnectorRecommendation> {
    // If we're already inside tmux, prefer tmux connector
    if inside_mux && terminal_type == TerminalType::Tmux && tmux_available {
        return Some(ConnectorRecommendation {
            name: "tmux",
            reason: "Already running inside tmux session",
            priority: 100,
        });
    }

    // tmux is generally the most capable connector
    if tmux_available {
        return Some(ConnectorRecommendation {
            name: "tmux",
            reason: "Full-featured multiplexer with complete API support",
            priority: 90,
        });
    }

    // iTerm2 native API is fastest iTerm2 option (Unix socket + Protobuf)
    #[cfg(target_os = "macos")]
    if iterm2_native_available {
        return Some(ConnectorRecommendation {
            name: "iterm2-native",
            reason: "iTerm2 via native Unix socket API - fastest macOS option",
            priority: 88,
        });
    }

    // it2 CLI is preferred over AppleScript for iTerm2 (faster, more features)
    #[cfg(target_os = "macos")]
    if it2_available {
        return Some(ConnectorRecommendation {
            name: "it2",
            reason: "iTerm2 via it2 CLI - fast Python API-based control",
            priority: 85,
        });
    }

    // iTerm2 AppleScript fallback (limited capabilities)
    #[cfg(target_os = "macos")]
    if iterm2_applescript_available {
        return Some(ConnectorRecommendation {
            name: "iTerm2",
            reason: "Native macOS terminal with AppleScript support (limited)",
            priority: 70,
        });
    }

    // No connector available
    None
}

/// Find and create the best available connector
pub fn find_best_connector() -> Option<DetectedConnector> {
    let availability = connector_availability();

    #[allow(clippy::option_if_let_else)]
    match availability.recommended {
        Some(ref rec) => match rec.name {
            "tmux" => {
                let connector = TmuxConnector::default();
                Some(DetectedConnector {
                    connector_type: TerminalType::Tmux,
                    connector: Box::new(connector),
                })
            }
            "iterm2-native" => {
                let connector = ITerm2NativeConnector::new();
                Some(DetectedConnector {
                    connector_type: TerminalType::ITerm2,
                    connector: Box::new(connector),
                })
            }
            "it2" => {
                let connector = It2Connector::new();
                Some(DetectedConnector {
                    connector_type: TerminalType::ITerm2,
                    connector: Box::new(connector),
                })
            }
            "iTerm2" => {
                let connector = ITermConnector::new();
                Some(DetectedConnector {
                    connector_type: TerminalType::ITerm2,
                    connector: Box::new(connector),
                })
            }
            _ => None,
        },
        None => None,
    }
}

/// Get a human-readable summary of terminal capabilities
pub fn capability_summary() -> String {
    let avail = connector_availability();

    let mut summary = String::new();
    summary.push_str(&format!("Terminal: {}\n", avail.terminal_type));
    summary.push_str(&format!("tmux available: {}\n", avail.tmux_available));
    summary.push_str(&format!(
        "iterm2-native available: {}\n",
        avail.iterm2_native_available
    ));
    summary.push_str(&format!("it2 CLI available: {}\n", avail.it2_available));
    summary.push_str(&format!(
        "iTerm2 (AppleScript) available: {}\n",
        avail.iterm2_applescript_available
    ));
    summary.push_str(&format!(
        "Inside multiplexer: {}\n",
        avail.inside_multiplexer
    ));

    if let Some(rec) = &avail.recommended {
        summary.push_str(&format!("Recommended: {} - {}\n", rec.name, rec.reason));
    } else {
        summary.push_str("No connector available - limited terminal automation\n");
        summary.push_str(&installation_help());
    }

    summary
}

/// Get installation help for connectors
pub fn installation_help() -> String {
    let mut help = String::new();
    help.push_str("\n--- Installation Help ---\n");
    help.push_str("To enable terminal automation, install one of the following:\n\n");

    // Check tmux
    match crate::install::check_connector("tmux") {
        Some(crate::install::InstallStatus::NotInstalled {
            install_command, ..
        }) => {
            help.push_str(&format!("• tmux (recommended): {install_command}\n"));
        }
        Some(crate::install::InstallStatus::Installed) => {
            help.push_str("• tmux: Already installed\n");
        }
        _ => {}
    }

    // Check iTerm2 native
    #[cfg(target_os = "macos")]
    match crate::install::check_connector("iterm2-native") {
        Some(crate::install::InstallStatus::NotInstalled {
            install_command, ..
        }) => {
            help.push_str(&format!("• iTerm2 Native: {install_command}\n"));
        }
        Some(crate::install::InstallStatus::ServiceUnavailable { reason, .. }) => {
            help.push_str(&format!(
                "• iTerm2 Native: {reason} - see connector_detect for setup\n"
            ));
        }
        Some(crate::install::InstallStatus::Installed) => {
            help.push_str("• iTerm2 Native: Already configured\n");
        }
        _ => {}
    }

    // Check it2 CLI
    #[cfg(target_os = "macos")]
    match crate::install::check_connector("it2") {
        Some(crate::install::InstallStatus::NotInstalled {
            install_command, ..
        }) => {
            help.push_str(&format!("• it2 CLI: {install_command}\n"));
        }
        Some(crate::install::InstallStatus::Installed) => {
            help.push_str("• it2 CLI: Already installed\n");
        }
        _ => {}
    }

    help.push_str("\nRun 'cargo run --package rustycode-connector --example connector_detect' for detailed status.\n");
    help
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_terminal() {
        let terminal = detect_terminal();
        println!("Detected terminal: {terminal}");
        // Just verify it returns something
        assert!(!format!("{terminal}").is_empty());
    }

    #[test]
    fn test_inside_multiplexer() {
        let inside = inside_multiplexer();
        println!("Inside multiplexer: {inside}");
        // This will be true if running inside tmux during tests
    }

    #[test]
    fn test_connector_availability() {
        let avail = connector_availability();
        println!("Connector availability: {avail:?}");

        // Basic sanity checks
        if avail.tmux_available {
            assert!(avail.recommended.is_some());
        }
    }

    #[test]
    fn test_capability_summary() {
        let summary = capability_summary();
        println!("Capability summary:\n{summary}");
        assert!(summary.contains("Terminal:"));
    }

    #[test]
    fn test_terminal_type_display_all_variants() {
        let variants = [
            (TerminalType::AppleTerminal, "Apple Terminal"),
            (TerminalType::ITerm2, "iTerm2"),
            (TerminalType::Tmux, "tmux"),
            (TerminalType::WezTerm, "WezTerm"),
            (TerminalType::Alacritty, "Alacritty"),
            (TerminalType::Kitty, "Kitty"),
            (TerminalType::Ghostty, "Ghostty"),
            (TerminalType::WindowsTerminal, "Windows Terminal"),
            (TerminalType::VSCodeTerminal, "VS Code Terminal"),
            (TerminalType::JetBrainsTerminal, "JetBrains Terminal"),
            (TerminalType::Warp, "Warp"),
            (TerminalType::Hyper, "Hyper"),
            (TerminalType::Unknown, "Unknown"),
        ];
        for (variant, expected) in &variants {
            assert_eq!(variant.to_string(), *expected);
        }
    }

    #[test]
    fn test_terminal_type_equality() {
        assert_eq!(TerminalType::Tmux, TerminalType::Tmux);
        assert_ne!(TerminalType::Tmux, TerminalType::ITerm2);
        assert_ne!(TerminalType::Unknown, TerminalType::Warp);
    }

    #[test]
    fn test_terminal_type_copy() {
        let a = TerminalType::Tmux;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_terminal_type_clone() {
        let a = TerminalType::ITerm2;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_terminal_type_debug() {
        let tt = TerminalType::Alacritty;
        let debug = format!("{tt:?}");
        assert!(debug.contains("Alacritty"));
    }

    #[test]
    fn test_connector_availability_construction() {
        let avail = ConnectorAvailability {
            terminal_type: TerminalType::Tmux,
            tmux_available: true,
            iterm2_applescript_available: false,
            it2_available: false,
            iterm2_native_available: false,
            inside_multiplexer: true,
            recommended: Some(ConnectorRecommendation {
                name: "tmux",
                reason: "test",
                priority: 90,
            }),
        };
        assert_eq!(avail.terminal_type, TerminalType::Tmux);
        assert!(avail.tmux_available);
        assert!(avail.inside_multiplexer);
        assert!(avail.recommended.is_some());
    }

    #[test]
    fn test_connector_availability_clone() {
        let avail = connector_availability();
        let cloned = avail.clone();
        assert_eq!(cloned.terminal_type, avail.terminal_type);
        assert_eq!(cloned.tmux_available, avail.tmux_available);
    }

    #[test]
    fn test_connector_availability_debug() {
        let avail = connector_availability();
        let debug = format!("{avail:?}");
        assert!(debug.contains("ConnectorAvailability"));
    }

    #[test]
    fn test_connector_recommendation_fields() {
        let rec = ConnectorRecommendation {
            name: "tmux",
            reason: "best option",
            priority: 100,
        };
        assert_eq!(rec.name, "tmux");
        assert_eq!(rec.reason, "best option");
        assert_eq!(rec.priority, 100);
    }

    #[test]
    fn test_connector_recommendation_clone() {
        let rec = ConnectorRecommendation {
            name: "it2",
            reason: "fast",
            priority: 85,
        };
        let cloned = rec.clone();
        assert_eq!(cloned.name, rec.name);
        assert_eq!(cloned.priority, rec.priority);
    }

    #[test]
    fn test_connector_recommendation_debug() {
        let rec = ConnectorRecommendation {
            name: "test",
            reason: "testing",
            priority: 50,
        };
        let debug = format!("{rec:?}");
        assert!(debug.contains("ConnectorRecommendation"));
    }

    #[test]
    fn test_determine_best_connector_prefers_tmux() {
        // When tmux is available, it should be recommended
        let rec = determine_best_connector(
            TerminalType::Unknown,
            true,  // tmux available
            false, // it2 not available
            false, // iterm2 native not available
            false, // iterm2 applescript not available
            false, // not inside multiplexer
        );
        assert!(rec.is_some());
        assert_eq!(rec.unwrap().name, "tmux");
    }

    #[test]
    fn test_determine_best_connector_inside_tmux() {
        let rec = determine_best_connector(
            TerminalType::Tmux,
            true, // tmux available
            false,
            false,
            false,
            true, // inside multiplexer
        );
        assert!(rec.is_some());
        let rec = rec.unwrap();
        assert_eq!(rec.name, "tmux");
        assert_eq!(rec.priority, 100);
    }

    #[test]
    fn test_determine_best_connector_no_connectors() {
        let rec = determine_best_connector(
            TerminalType::Unknown,
            false, // no tmux
            false, // no it2
            false, // no iterm2 native
            false, // no applescript
            false,
        );
        assert!(rec.is_none());
    }

    #[test]
    fn test_installation_help() {
        let help = installation_help();
        assert!(help.contains("Installation Help"));
    }

    #[test]
    fn test_find_best_connector_returns_none_without_connectors() {
        // This test just exercises the function; result depends on environment
        let _ = find_best_connector();
    }
}
