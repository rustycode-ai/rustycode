//! Heuristic command risk classification.
//!
//! Categorizes shell commands as Safe / Ask / Blocked based on pattern matching,
//! and caches user decisions to reduce prompting over time.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Risk level for a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PermissionRiskLevel {
    /// Safe to execute without asking (e.g., ls, cat, git status).
    Safe,
    /// Should ask user before executing (e.g., rm, curl, pip install).
    Ask,
    /// Should be blocked entirely (e.g., rm -rf /, mkfs).
    Blocked,
}

/// Semantic category of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CommandCategory {
    FileRead,
    FileWrite,
    FileDelete,
    PackageManage,
    NetworkFetch,
    NetworkUpload,
    Build,
    Test,
    GitRead,
    GitWrite,
    SystemInfo,
    ProcessManagement,
    Destructive,
    Unknown,
}

impl std::fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Result of classifying a command.
#[derive(Debug, Clone)]
pub struct Classification {
    pub level: PermissionRiskLevel,
    pub category: CommandCategory,
    pub explanation: String,
    pub matched_pattern: Option<String>,
}

/// Decision cache key (command hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DecisionKey(u64);

impl DecisionKey {
    fn new(command: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        command.hash(&mut hasher);
        DecisionKey(hasher.finish())
    }
}

/// A cached user decision.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CachedDecision {
    level: PermissionRiskLevel,
    timestamp_secs: u64,
    ttl_secs: u64,
}

/// Heuristic permission classifier with decision caching.
pub struct PermissionClassifier {
    cache: HashMap<DecisionKey, CachedDecision>,
    cache_path: Option<PathBuf>,
    default_ttl_secs: u64,
}

impl PermissionClassifier {
    /// Create a new classifier with optional disk-backed decision cache.
    pub fn new(cache_dir: Option<&Path>) -> Self {
        let cache_path = cache_dir.map(|d| d.join("permission_cache.json"));
        let mut classifier = Self {
            cache: HashMap::new(),
            cache_path,
            default_ttl_secs: 3600, // 1 hour default
        };
        let path_clone = classifier.cache_path.clone();
        if let Some(path) = path_clone {
            classifier.load_cache(&path);
        }
        classifier
    }

    /// Classify a command string.
    pub fn classify(&self, command: &str) -> Classification {
        let trimmed = command.trim();
        let base_cmd = Self::base_command(trimmed);

        // Check blocked patterns first
        if let Some(matched) = Self::check_blocked(trimmed, &base_cmd) {
            return Classification {
                level: PermissionRiskLevel::Blocked,
                category: CommandCategory::Destructive,
                explanation: format!("Destructive command blocked: {matched}"),
                matched_pattern: Some(matched.to_string()),
            };
        }

        // Check safe patterns
        if let Some((category, pattern)) = Self::check_safe(&base_cmd, trimmed) {
            return Classification {
                level: PermissionRiskLevel::Safe,
                category,
                explanation: format!("Read-only command: {pattern}"),
                matched_pattern: Some(pattern.to_string()),
            };
        }

        // Everything else requires asking
        let category = Self::infer_category(&base_cmd);
        Classification {
            level: PermissionRiskLevel::Ask,
            category,
            explanation: format!("Command requires approval: {base_cmd}"),
            matched_pattern: Some(base_cmd),
        }
    }

    /// Classify and check the decision cache. Returns the cached decision if
    /// the user previously approved/denied this exact command and the entry
    /// hasn't expired.
    pub fn classify_with_cache(&mut self, command: &str) -> Classification {
        let key = DecisionKey::new(command);

        // Check cache first
        if let Some(cached) = self.cache.get(&key) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now.saturating_sub(cached.timestamp_secs) < cached.ttl_secs {
                return Classification {
                    level: cached.level,
                    category: CommandCategory::Unknown,
                    explanation: "Cached decision".to_string(),
                    matched_pattern: None,
                };
            }
            // Expired entry
            self.cache.remove(&key);
        }

        self.classify(command)
    }

    /// Store a user decision for future use.
    pub fn cache_decision(&mut self, command: &str, level: PermissionRiskLevel) {
        let key = DecisionKey::new(command);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.cache.insert(
            key,
            CachedDecision {
                level,
                timestamp_secs: now,
                ttl_secs: self.default_ttl_secs,
            },
        );
        if let Some(ref path) = self.cache_path {
            self.save_cache(path);
        }
    }

    /// Clear the decision cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        if let Some(ref path) = self.cache_path {
            let _ = std::fs::remove_file(path);
        }
    }

    // ── Pattern tables ──────────────────────────────────────────────────

    fn base_command(command: &str) -> String {
        command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }

    fn check_blocked(command: &str, base: &str) -> Option<&'static str> {
        // Absolute blocks
        let blocks: &[(&str, &[&str])] = &[
            ("rm -rf /", &["rm -rf /", "rm -rf /*", "rm -fr /"]),
            ("mkfs", &["mkfs"]),
            ("dd if=", &["dd if="]),
            ("> /dev/sd", &["> /dev/sd", "dd of=/dev/sd"]),
            (":(){:|:&};:", &[":(){:|:&};:"]),
            ("chmod -R 777 /", &["chmod -R 777 /"]),
            ("npm publish", &["npm publish"]),
            ("cargo publish", &["cargo publish"]),
            ("git push --force", &["git push --force", "git push -f"]),
        ];

        for (name, patterns) in blocks {
            if patterns.iter().any(|p| command.contains(p)) {
                return Some(name);
            }
        }

        // Block commands that redirect to system devices
        if command.contains("/dev/sd") || command.contains("/dev/hd") {
            return Some("device write");
        }

        let _ = base; // base not needed for blocked check currently
        None
    }

    fn check_safe<'a>(base: &'a str, command: &str) -> Option<(CommandCategory, &'static str)> {
        let safe_commands: &[(CommandCategory, &[&str])] = &[
            (CommandCategory::FileRead, &["ls", "cat", "head", "tail", "less", "more", "file", "stat", "wc", "md5sum", "sha256sum", "cksum"]),
            (CommandCategory::FileRead, &["grep", "egrep", "fgrep", "rg", "ag", "ack"]),
            (CommandCategory::FileRead, &["find", "locate", "which", "whereis", "type"]),
            (CommandCategory::FileRead, &["diff", "comm", "cmp"]),
            (CommandCategory::FileRead, &["echo", "printf", "seq", "yes"]),
            (CommandCategory::SystemInfo, &["uname", "hostname", "uptime", "whoami", "id", "env", "printenv", "date", "cal"]),
            (CommandCategory::SystemInfo, &["df", "du", "free", "top", "ps", "lsof", "ss", "netstat", "ifconfig", "ip"]),
            (CommandCategory::SystemInfo, &["systemctl status", "journalctl"]),
            (CommandCategory::GitRead, &["git status", "git log", "git diff", "git branch", "git show", "git remote", "git tag", "git stash list", "git describe"]),
            (CommandCategory::Build, &["cargo build", "cargo check", "cargo clippy", "cargo test", "cargo bench", "cargo doc", "cargo metadata"]),
            (CommandCategory::Build, &["npm run build", "npm run test", "npm run lint", "npm run check", "npm run typecheck"]),
            (CommandCategory::Build, &["make", "cmake", "ninja"]),
            (CommandCategory::Test, &["pytest", "jest", "vitest", "mocha", "cargo test", "go test", "npm test"]),
            (CommandCategory::PackageManage, &["pip list", "pip show", "pip freeze", "npm list", "cargo tree", "cargo search"]),
            (CommandCategory::FileRead, &["tree", "exa", "eza", "fd"]),
            (CommandCategory::FileRead, &["jq", "yq", "xq", "hx"]),
        ];

        for (category, commands) in safe_commands {
            if commands.iter().any(|cmd| {
                if cmd.contains(' ') {
                    command.starts_with(cmd)
                } else {
                    base == *cmd
                }
            }) {
                return Some((*category, commands[0]));
            }
        }

        // git subcommand check
        if base == "git" {
            let git_args: Vec<&str> = command.split_whitespace().collect();
            if git_args.len() >= 2 {
                let subcmd = git_args[1];
                let safe_git = ["status", "log", "diff", "show", "branch", "remote", "tag", "stash", "describe", "rev-parse", "config", "blame", "shortlog", "reflog", "ls-files", "ls-tree"];
                if safe_git.contains(&subcmd) {
                    return Some((CommandCategory::GitRead, "git (read)"));
                }
            }
        }

        None
    }

    fn infer_category(base: &str) -> CommandCategory {
        match base {
            "rm" | "rmdir" => CommandCategory::FileDelete,
            "curl" | "wget" => CommandCategory::NetworkFetch,
            "scp" | "rsync" | "sftp" => CommandCategory::NetworkUpload,
            "pip" | "pip3" | "npm" | "yarn" | "pnpm" | "cargo" | "brew" | "apt" | "yum" | "dnf" => CommandCategory::PackageManage,
            "touch" | "mkdir" | "cp" | "mv" | "chmod" | "chown" | "ln" => CommandCategory::FileWrite,
            "git" => CommandCategory::GitWrite,
            "kill" | "killall" | "pkill" => CommandCategory::ProcessManagement,
            _ => CommandCategory::Unknown,
        }
    }

    // ── Cache persistence ───────────────────────────────────────────────

    fn load_cache(&mut self, path: &Path) {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(cache) = serde_json::from_str::<HashMap<String, CachedDecision>>(&data) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                for (cmd_str, decision) in cache {
                    if now.saturating_sub(decision.timestamp_secs) < decision.ttl_secs {
                        self.cache.insert(DecisionKey::new(&cmd_str), decision);
                    }
                }
            }
        }
    }

    fn save_cache(&self, path: &Path) {
        let serializable: HashMap<String, &CachedDecision> = self
            .cache
            .iter()
            .map(|(_, v)| (format!("{:016x}", v.timestamp_secs), v))
            .collect();
        if let Ok(json) = serde_json::to_string(&serializable) {
            let _ = std::fs::write(path, json);
        }
    }
}

impl Default for PermissionClassifier {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        let classifier = PermissionClassifier::new(None);
        let safe = ["ls", "cat file.txt", "grep pattern file", "git status", "git log --oneline", "cargo build", "echo hello", "wc -l file"];
        for cmd in safe {
            let result = classifier.classify(cmd);
            assert_eq!(result.level, PermissionRiskLevel::Safe, "Expected safe for: {cmd}");
        }
    }

    #[test]
    fn test_blocked_commands() {
        let classifier = PermissionClassifier::new(None);
        let blocked = ["rm -rf /", "mkfs /dev/sda1", "npm publish", "cargo publish", "git push --force"];
        for cmd in blocked {
            let result = classifier.classify(cmd);
            assert_eq!(result.level, PermissionRiskLevel::Blocked, "Expected blocked for: {cmd}");
        }
    }

    #[test]
    fn test_ask_commands() {
        let classifier = PermissionClassifier::new(None);
        let ask = ["rm file.txt", "curl http://example.com", "pip install flask", "touch newfile"];
        for cmd in ask {
            let result = classifier.classify(cmd);
            assert_eq!(result.level, PermissionRiskLevel::Ask, "Expected ask for: {cmd}");
        }
    }

    #[test]
    fn test_category_inference() {
        let classifier = PermissionClassifier::new(None);
        assert_eq!(classifier.classify("rm file").category, CommandCategory::FileDelete);
        assert_eq!(classifier.classify("curl url").category, CommandCategory::NetworkFetch);
        assert_eq!(classifier.classify("pip install x").category, CommandCategory::PackageManage);
    }

    #[test]
    fn test_cache_decision() {
        let mut classifier = PermissionClassifier::new(None);
        classifier.cache_decision("dangerous-cmd", PermissionRiskLevel::Ask);
        let result = classifier.classify_with_cache("dangerous-cmd");
        assert_eq!(result.level, PermissionRiskLevel::Ask);
        assert_eq!(result.explanation, "Cached decision");
    }

    #[test]
    fn test_cache_clear() {
        let mut classifier = PermissionClassifier::new(None);
        classifier.cache_decision("cmd", PermissionRiskLevel::Safe);
        assert!(classifier.cache.len() == 1);
        classifier.clear_cache();
        assert!(classifier.cache.is_empty());
    }
}
