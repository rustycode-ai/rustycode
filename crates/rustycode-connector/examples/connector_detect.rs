//! Connector Detection and Installation Tool
//!
//! Checks for available terminal connectors and provides installation instructions.

use rustycode_connector::{
    check_connector, print_connector_status, ITerm2NativeConnector, ITermConnector, InstallStatus,
    It2Connector, TmuxConnector,
};

fn print_usage() {
    println!("CONNECTOR DETECTION AND INSTALLATION TOOL");
    println!();
    println!("Usage:");
    println!("  connector_detect              - Check status of all connectors");
    println!("  connector_detect tmux         - Check tmux specifically");
    println!("  connector_detect it2          - Check it2 CLI specifically");
    println!("  connector_detect iterm2       - Check iTerm2 (AppleScript) specifically");
    println!("  connector_detect iterm2-native - Check iTerm2 native API specifically");
    println!();
    println!("Environment Variables:");
    println!("  ITERM2_COOKIE   - iTerm2 API authentication cookie");
    println!("  ITERM2_KEY      - iTerm2 API key (alternative to cookie)");
    println!();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 2 {
        print_usage();
        std::process::exit(1);
    }

    if args.len() == 1 {
        // Show status for all connectors
        print_connector_status();
    } else {
        // Check specific connector
        let connector_name = &args[1];

        println!("Checking connector: {connector_name}");
        println!("{}", "=".repeat(50));

        if let Some(status) = check_connector(connector_name) {
            match &status {
                InstallStatus::Installed => {
                    println!("✅ {connector_name} is installed and ready to use");

                    // Try to create the connector and verify it works
                    let connector_available = match connector_name.as_str() {
                        "tmux" => TmuxConnector::check_available(),
                        "it2" => It2Connector::check_available(),
                        "iterm2" | "iterm2-applescript" => ITermConnector::is_available(),
                        "iterm2-native" => ITerm2NativeConnector::check_available(),
                        _ => false,
                    };

                    if connector_available {
                        println!("   Connector is functional and ready to use.");
                    } else {
                        println!("   Note: Binary is installed but connector may not be fully available.");
                        println!("   - For tmux: Start a tmux session with 'tmux'");
                        println!("   - For it2: Ensure iTerm2 is running");
                        println!("   - For iTerm2: Open iTerm2 application");
                    }
                }
                InstallStatus::NotInstalled {
                    install_command,
                    install_instructions,
                } => {
                    println!("❌ {connector_name} is not installed");
                    println!();
                    println!("Installation instructions:");
                    println!("{install_instructions}");
                    println!();
                    println!("Quick install:");
                    println!("  {install_command}");
                }
                InstallStatus::ServiceUnavailable {
                    reason,
                    setup_instructions,
                } => {
                    println!("⏸️ {connector_name} - {reason}");
                    println!();
                    println!("Setup instructions:");
                    println!("{setup_instructions}");
                }
                InstallStatus::Unsupported { reason } => {
                    println!("⚠️ {connector_name} is not supported on this platform");
                    println!("   Reason: {reason}");
                }
                #[allow(unreachable_patterns)]
                _ => {
                    println!("❓ {connector_name} has unknown status");
                }
            }
        } else {
            println!("Unknown connector: {connector_name}");
            println!();
            println!("Available connectors:");
            println!("  - tmux");
            println!("  - it2");
            println!("  - iterm2 (AppleScript)");
            println!("  - iterm2-native");
            std::process::exit(1);
        }
    }

    // Show current environment
    println!();
    println!("{}", "=".repeat(50));
    println!("ENVIRONMENT");
    println!("{}", "=".repeat(50));

    let env_vars = ["ITERM2_COOKIE", "ITERM2_KEY", "TMUX", "TERM_PROGRAM"];
    for var in &env_vars {
        match std::env::var(var) {
            Ok(val) => println!("  {} = {}", var, val.chars().take(50).collect::<String>()),
            Err(_) => println!("  {var} = (not set)"),
        }
    }
}
