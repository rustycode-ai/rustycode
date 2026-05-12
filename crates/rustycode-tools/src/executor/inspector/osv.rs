use super::{InspectionAction, InspectionResult, ToolCallInfo, ToolInspector};
use crate::ToolContext;
use rustycode_protocol::tool_names as tn;

/// Inspects bash commands for package installation patterns and checks for
/// known malicious packages via the OSV database.
///
/// This is a **synchronous** inspector that extracts package names from
/// install commands (npm, pip, npx, uvx, pipx) and flags them for
/// user approval. The actual OSV API check happens asynchronously -- the
/// inspector provides a first-pass filter that catches obviously suspicious
/// package install commands.
///
/// # Detection Strategy
///
/// 1. Parse the command to extract the binary name and arguments
/// 2. If the binary is a package manager (npm, pip, npx, etc.), extract the package name
/// 3. Flag the call for `RequireApproval` so the a subsequent async
///    OSV check can run before execution
///
/// Inspired by goose's `extension_malware_check.rs`.
pub struct OsvInspector {
    /// Known typosquatting patterns in package names
    suspicious_patterns: Vec<&'static str>,
}

impl OsvInspector {
    pub fn new() -> Self {
        Self {
            suspicious_patterns: vec![
                // Common typosquatting patterns
                "-crypto",
                "-miner",
                "-wallet",
                "-stealer",
                "-grabber",
                "-clipper",
                "-injector",
                "-keylog",
                "-trojan",
                "-backdoor",
                "-rat",
                "-spy",
                "-exfil",
                "-phish",
                "-exploit",
                // Suspicious npm patterns
                "crypto-miner",
                "wallet-drain",
                "token-steal",
                "discord-token",
                "browser-cookie",
                "password-grab",
                "clipboard",
                "screenshot",
                "keylogger",
            ],
        }
    }

    /// Check if a package name matches any suspicious pattern.
    pub fn is_suspicious_name(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        self.suspicious_patterns
            .iter()
            .any(|pat| lower.contains(pat))
    }

    pub fn extract_package_from_args(&self, cmd: &str, args: &[String]) -> Option<String> {
        match cmd {
            c if c.ends_with("npx") => {
                // npx <package> -- first non-flag arg is the package
                args.iter()
                    .find(|a| !a.starts_with('-') && !a.is_empty())
                    .map(|s| strip_npm_version(s))
            }
            c if c.ends_with("npm") => {
                // npm install/add <package> -- skip the subcommand
                // Also handles: npm --save-dev <package> (no subcommand, first non-flag arg is package)
                let subcmds: [&str; 6] = ["install", "i", "add", "ci", "update", "upgrade"];
                let mut found_subcmd = false;
                for arg in args {
                    if subcmds.contains(&arg.as_str()) {
                        found_subcmd = true;
                        continue;
                    }
                    if arg.starts_with('-') || arg.is_empty() {
                        continue;
                    }
                    // If no subcommand found yet, this first non-flag arg could be
                    // the package (e.g., `npm --save-dev eslint`) or a subcommand
                    // we don't recognize -- treat it as the package either way.
                    if found_subcmd || !subcmds.contains(&arg.as_str()) {
                        return Some(strip_npm_version(arg));
                    }
                }
                None
            }
            c if c.ends_with("pip") || c.ends_with("pip3") => {
                // pip install <package> -- skip "install" subcommand
                // Also handles: pip --force <package> (first non-flag arg after flags)
                let subcmds: [&str; 2] = ["install", "install-download"];
                let mut found_subcmd = false;
                for arg in args {
                    if subcmds.contains(&arg.as_str()) {
                        found_subcmd = true;
                        continue;
                    }
                    if arg.starts_with('-') || arg.is_empty() {
                        continue;
                    }
                    if found_subcmd || !subcmds.contains(&arg.as_str()) {
                        return Some(if let Some(idx) = arg.find("==") {
                            arg.get(..idx).unwrap_or(arg).to_string()
                        } else {
                            arg.to_string()
                        });
                    }
                }
                None
            }
            c if c.ends_with("pipx") || c.ends_with("uvx") => {
                // pipx/uvx <package> -- first non-flag arg
                args.iter()
                    .find(|a| !a.starts_with('-') && !a.is_empty())
                    .map(|s| {
                        if let Some(idx) = s.find("==") {
                            s.get(..idx).unwrap_or(s).to_string()
                        } else {
                            s.to_string()
                        }
                    })
            }
            c if c.ends_with("uv") => {
                // uv pip install <package> or uvx <package>
                let skip_words: [&str; 4] = ["pip", "install", "run", "tool"];
                let mut i = 0;
                while i < args.len() {
                    let arg = &args[i];
                    if arg == "pip" && i + 1 < args.len() && args[i + 1] == "install" {
                        // uv pip install <pkg> -- look for package after "install"
                        for a in &args[i + 2..] {
                            if !a.starts_with('-') && !a.is_empty() {
                                return Some(if let Some(idx) = a.find("==") {
                                    a.get(..idx).unwrap_or(a).to_string()
                                } else {
                                    a.to_string()
                                });
                            }
                        }
                        return None;
                    }
                    if !arg.starts_with('-')
                        && !arg.is_empty()
                        && !skip_words.contains(&arg.as_str())
                    {
                        return Some(arg.to_string());
                    }
                    i += 1;
                }
                None
            }
            _ => None,
        }
    }

    /// Parse a command string to extract (`binary_name`, args).
    fn parse_command(command: &str) -> Option<(String, Vec<String>)> {
        let tokens = shell_words::split(command).ok()?;
        let mut tokens = tokens.into_iter();
        let binary = tokens.next()?;
        let args: Vec<String> = tokens.collect();
        Some((binary, args))
    }

    /// Check if a binary is a package manager that should be inspected.
    fn is_package_manager(binary: &str) -> bool {
        matches!(
            binary,
            "npx" | "npm" | "pip" | "pip3" | "pipx" | "uvx" | "uv"
        )
    }
}

impl Default for OsvInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolInspector for OsvInspector {
    fn name(&self) -> &'static str {
        "osv_malware"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        _history: &[ToolCallInfo],
        _ctx: &ToolContext,
    ) -> InspectionResult {
        // Only inspect bash commands
        if call.name != tn::BASH {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "Not a bash command".to_string(),
                confidence: 1.0,
                inspector_name: "osv_malware".to_string(),
                finding_id: None,
            };
        }

        // Extract the command string from arguments
        let command = call
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if command.is_empty() {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "Empty command".to_string(),
                confidence: 1.0,
                inspector_name: "osv_malware".to_string(),
                finding_id: None,
            };
        }

        // Parse the command
        let Some((binary, args)) = Self::parse_command(command) else {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "Could not parse command".to_string(),
                confidence: 1.0,
                inspector_name: "osv_malware".to_string(),
                finding_id: None,
            };
        };

        // Only check package managers
        if !Self::is_package_manager(&binary) {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: format!("Not a package manager command ({binary})"),
                confidence: 1.0,
                inspector_name: "osv_malware".to_string(),
                finding_id: None,
            };
        }

        // Extract the package name
        let Some(pkg_name) = self.extract_package_from_args(&binary, &args) else {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "No package name found in command".to_string(),
                confidence: 1.0,
                inspector_name: "osv_malware".to_string(),
                finding_id: None,
            };
        };

        // Check against suspicious patterns
        if self.is_suspicious_name(&pkg_name) {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Deny,
                reason: format!(
                    "Package '{pkg_name}' matches suspicious pattern (possible malware). \
                     Install blocked -- verify the package source before proceeding."
                ),
                confidence: 0.85,
                inspector_name: "osv_malware".to_string(),
                finding_id: Some("OSV-001".to_string()),
            };
        }

        // For known-good package managers, flag for approval so async OSV check can run
        InspectionResult {
            request_id: call.id.clone(),
            action: InspectionAction::RequireApproval(Some(format!(
                "Package installation detected: '{pkg_name}' via {binary}. OSV malware check recommended."
            ))),
            reason: format!(
                "Package install command requires approval for OSV verification: {binary} {pkg_name}"
            ),
            confidence: 0.5,
            inspector_name: "osv_malware".to_string(),
            finding_id: None,
        }
    }
}

/// Strip the version suffix from an npm package specifier.
///
/// Handles:
/// - `@scope/pkg@1.2.3` -> `@scope/pkg`
/// - `pkg@1.2.3` -> `pkg`
/// - `@scope/pkg` -> `@scope/pkg`
/// - `pkg` -> `pkg`
fn strip_npm_version(s: &str) -> String {
    if let Some(stripped) = s.strip_prefix('@') {
        // Scoped package: find second '@' (version separator)
        if let Some(at_pos) = stripped.find('@') {
            format!("@{}", &stripped[..at_pos])
        } else {
            s.to_string()
        }
    } else if let Some(idx) = s.find('@') {
        s[..idx].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> ToolCallInfo {
        ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_osv_inspector_skips_non_bash() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
        assert_eq!(result.inspector_name, "osv_malware");
    }

    #[test]
    fn test_osv_inspector_allows_non_package_command() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "cargo build --release"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
    }

    #[test]
    fn test_osv_inspector_flags_npm_install() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "npm install react@18.3.1"}));
        let result = inspector.inspect(&call, &[], &ctx);

        // Should require approval for OSV check
        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
        assert!(result.reason.contains("react"));
        assert_eq!(result.inspector_name, "osv_malware");
    }

    #[test]
    fn test_osv_inspector_flags_npx() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "npx create-react-app myapp"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
        assert!(result.reason.contains("create-react-app"));
    }

    #[test]
    fn test_osv_inspector_flags_pip_install() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "pip install requests==2.32.3"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
        assert!(result.reason.contains("requests"));
    }

    #[test]
    fn test_osv_inspector_denies_suspicious_package() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "npx crypto-miner-tool"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Deny);
        assert!(result.reason.contains("suspicious pattern"));
        assert_eq!(result.finding_id, Some("OSV-001".to_string()));
    }

    #[test]
    fn test_osv_inspector_denies_discord_token_stealer() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call(
            "Bash",
            json!({"command": "pip install discord-token-grabber"}),
        );
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Deny);
        assert!(result.reason.contains("suspicious"));
    }

    #[test]
    fn test_osv_inspector_allows_normal_pip_package() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "pip install numpy"}));
        let result = inspector.inspect(&call, &[], &ctx);

        // Normal packages should require approval (for OSV API check), not deny
        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
    }

    #[test]
    fn test_osv_inspector_empty_command() {
        let inspector = OsvInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": ""}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
    }

    #[test]
    fn test_osv_inspector_in_security_pipeline() {
        use crate::executor::manager::ToolInspectionManager;
        let manager = ToolInspectionManager::with_security(5);
        let names = manager.inspector_names();
        assert!(
            names.contains(&"osv_malware"),
            "osv_malware inspector should be in security pipeline"
        );
    }

    #[test]
    fn test_osv_suspicious_name_detection() {
        let inspector = OsvInspector::new();

        // Should detect suspicious names
        assert!(inspector.is_suspicious_name("crypto-miner-tool"));
        assert!(inspector.is_suspicious_name("wallet-drain-helper"));
        assert!(inspector.is_suspicious_name("discord-token-extractor"));
        assert!(inspector.is_suspicious_name("browser-cookie-grabber"));
        assert!(inspector.is_suspicious_name("my-keylogger-lib"));

        // Should not flag normal names
        assert!(!inspector.is_suspicious_name("react"));
        assert!(!inspector.is_suspicious_name("express"));
        assert!(!inspector.is_suspicious_name("numpy"));
        assert!(!inspector.is_suspicious_name("requests"));
    }

    #[test]
    fn test_osv_extract_npm_package() {
        let inspector = OsvInspector::new();

        let args: Vec<String> = vec!["install".to_string(), "react@18.3.1".to_string()];
        let pkg = inspector.extract_package_from_args("npm", &args);
        assert_eq!(pkg, Some("react".to_string()));

        let args2: Vec<String> = vec!["--save-dev".to_string(), "eslint".to_string()];
        let pkg2 = inspector.extract_package_from_args("npm", &args2);
        assert_eq!(pkg2, Some("eslint".to_string()));
    }

    #[test]
    fn test_osv_extract_pip_package() {
        let inspector = OsvInspector::new();

        let args: Vec<String> = vec!["install".to_string(), "requests==2.32.3".to_string()];
        let pkg = inspector.extract_package_from_args("pip", &args);
        assert_eq!(pkg, Some("requests".to_string()));

        let args2: Vec<String> = vec!["--force".to_string(), "numpy".to_string()];
        let pkg2 = inspector.extract_package_from_args("pip", &args2);
        assert_eq!(pkg2, Some("numpy".to_string()));
    }

    #[test]
    fn test_osv_extract_packages_without_panic_on_scopes() {
        let inspector = OsvInspector::new();

        let npm_args: Vec<String> = vec!["@scope/pkg@1.2.3".to_string()];
        assert_eq!(
            inspector.extract_package_from_args("npm", &npm_args),
            Some("@scope/pkg".to_string())
        );

        let pip_args: Vec<String> = vec!["requests==2.32.3".to_string()];
        assert_eq!(
            inspector.extract_package_from_args("pip", &pip_args),
            Some("requests".to_string())
        );
    }

    #[test]
    fn test_strip_npm_version_handles_scoped_package() {
        assert_eq!(strip_npm_version("@scope/pkg@1.2.3"), "@scope/pkg");
        assert_eq!(strip_npm_version("@scope/pkg"), "@scope/pkg");
        assert_eq!(strip_npm_version("pkg@1.2.3"), "pkg");
        assert_eq!(strip_npm_version("pkg"), "pkg");
    }

    #[test]
    fn test_strip_npm_version_edge_cases() {
        // Deep scoped package
        assert_eq!(strip_npm_version("@org/team-pkg@^3.0.0"), "@org/team-pkg");
        // No scope, version with prerelease tag
        assert_eq!(strip_npm_version("typescript@5.0.0-beta.1"), "typescript");
        // Scoped with multiple @ in version (e.g., @latest)
        assert_eq!(strip_npm_version("@scope/pkg@latest"), "@scope/pkg");
        // Just @scope without package name (edge case)
        assert_eq!(strip_npm_version("@scope"), "@scope");
        // Empty string
        assert_eq!(strip_npm_version(""), "");
    }

    #[test]
    fn test_strip_npm_version_npx_scoped() {
        // npx should also handle scoped packages
        let inspector = OsvInspector::new();
        let args: Vec<String> = vec!["@scope/create-app@1.0.0".to_string()];
        assert_eq!(
            inspector.extract_package_from_args("npx", &args),
            Some("@scope/create-app".to_string())
        );
    }

    #[test]
    fn test_parse_command_preserves_quoted_arguments() {
        let (binary, args) =
            OsvInspector::parse_command(r#"npm install eslint --prefix "/tmp/my project""#)
                .expect("command should parse");

        assert_eq!(binary, "npm");
        assert_eq!(
            args,
            vec![
                "install".to_string(),
                "eslint".to_string(),
                "--prefix".to_string(),
                "/tmp/my project".to_string()
            ]
        );
    }
}
