use crate::codec::{HookInput, HookResult};
use crate::rules::{all_rules, GuardAction, GuardRule};
use rustycode_protocol::tool_names;

// PreToolUse evaluation with short-circuit rules
pub fn evaluate(input: &HookInput) -> HookResult {
    for r in all_rules() {
        if matches_rule(r, input) {
            return match r.action {
                GuardAction::Deny => HookResult::deny(format!(
                    "{} triggered {}: {}",
                    input.tool_name, r.id, r.description
                )),
                GuardAction::Ask => HookResult::ask(format!(
                    "{} requires confirmation: {} - {}",
                    input.tool_name, r.id, r.description
                )),
                GuardAction::Warn => HookResult::warn(format!(
                    "{} warning: {} - {}",
                    input.tool_name, r.id, r.description
                )),
            };
        }
    }
    HookResult::allow()
}

/// Check if the tool is a write-capable tool (modifies files or content).
fn is_write_tool(name: &str) -> bool {
    tool_names::is_write_tool(name)
}

/// Check if the tool is a shell/execution tool.
fn is_bash_tool(name: &str) -> bool {
    tool_names::is_bash_tool(name)
}

/// Extract a file path from tool input, checking both `file_path` and `path` keys.
fn extract_path(tf: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    tf.get("file_path")
        .or_else(|| tf.get("path"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

#[allow(clippy::too_many_lines)]
fn matches_rule(rule: &GuardRule, input: &HookInput) -> bool {
    let tool_original = &input.tool_name;
    let tf = input
        .tool_input
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    let cmd = tf
        .get("command")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let path = extract_path(&tf);
    let content = tf
        .get("content")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    match rule.id {
        // R01: Block sudo commands
        "R01" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    return c.contains("sudo");
                }
            }
            false
        }
        // R02: Block writes to protected paths (.git, .env, credentials, keys)
        "R02" => {
            if is_write_tool(tool_original) {
                if let Some(p) = path {
                    return protected_path_contains(&p);
                }
            }
            false
        }
        // R03: Bash writes to protected paths via redirection
        "R03" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    if c.contains('>') || c.contains("=>") {
                        if let Some(p) = path {
                            return protected_path_contains(&p);
                        }
                        return protected_path_contains(c);
                    }
                }
            }
            false
        }
        // R04: Outside of cwd — only apply to write/edit/bash (reading outside cwd is fine)
        "R04" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(p) = path {
                    let path_is_relative = !p.starts_with('/');
                    if path_is_relative {
                        return false;
                    }
                    if let Some(cwd) = &input.cwd {
                        let cwd_with_sep = if cwd.ends_with('/') {
                            cwd.as_str()
                        } else {
                            // Use a temporary to enforce directory boundary
                            // so /home/user/project does NOT match /home/user/projectile
                            return !p.starts_with(&format!("{}/", cwd));
                        };
                        return !p.starts_with(cwd_with_sep);
                    }
                }
            }
            false
        }
        // R05: rm -rf
        "R05" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    return c.contains("rm -rf");
                }
            }
            false
        }
        // R06: git push --force or -f
        "R06" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    return c.contains("git push --force") || c.contains("git push -f");
                }
            }
            false
        }
        // R07: Secrets in content — only check write/edit/bash (reading secrets is fine)
        "R07" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(ct) = &content {
                    let s = ct.to_lowercase();
                    return s.contains("sk-")
                        || s.contains("ghp_")
                        || s.contains("akia")
                        || s.contains("-----begin rsa private key-----")
                        || s.contains("-----begin private key-----");
                }
            }
            false
        }
        // R08: Binary write extensions — only applies to write/edit/bash tools
        "R08" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(p) = &path {
                    let ex = p.rsplit('.').next().unwrap_or("");
                    let blocked = ["exe", "dll", "so", "dylib", "bin", "db", "sqlite"];
                    return blocked.contains(&ex);
                }
            }
            false
        }
        // R09: Path traversal — only apply to write/edit/bash (reads are safe)
        "R09" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(p) = &path {
                    return p.contains("..");
                }
            }
            false
        }
        // R10: no-verify / no-gpg-sign in Bash
        "R10" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    return c.contains("--no-verify") || c.contains("--no-gpg-sign");
                }
            }
            false
        }
        // R11: git reset --hard main/master
        "R11" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    return c.contains("git reset --hard")
                        && (c.contains("main") || c.contains("master"));
                }
            }
            false
        }
        // R12: git push origin main/master
        "R12" => {
            if is_bash_tool(tool_original) {
                if let Some(c) = &cmd {
                    return c.contains("git push origin main")
                        || c.contains("git push origin master");
                }
            }
            false
        }
        // R13: config edits — only apply to write/edit/bash tools
        "R13" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(p) = &path {
                    let lowered = p.to_lowercase();
                    return lowered.contains("settings.json")
                        || lowered.contains("claude/settings")
                        || lowered.contains(".eslintrc")
                        || lowered.contains("tsconfig");
                }
            }
            false
        }
        // R14: symlink in path — only check write/edit/bash (reading through symlinks is fine)
        "R14" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(p) = &path {
                    return is_symlink_in_path(p);
                }
            }
            false
        }
        // R15: content length > 10MB — only check write/edit/bash
        "R15" => {
            if is_write_tool(tool_original) || is_bash_tool(tool_original) {
                if let Some(ct) = &content {
                    return ct.len() > 10_000_000;
                }
            }
            false
        }
        _ => false,
    }
}

fn protected_path_contains(p: &str) -> bool {
    let restricted = [
        ".git/",
        ".env",
        "credentials",
        ".key",
        ".pem",
        "/etc/",
        "/proc/",
        "/sys/",
    ];
    restricted.iter().any(|r| p.contains(r))
}

/// Check if any component in the path is a broken or looping symlink.
/// Resolves valid symlinks (e.g., `/tmp` → `/private/tmp` on macOS) and
/// continues walking the resolved path. Only flags symlinks that cannot be
/// resolved (broken, looping, or permission-denied).
fn is_symlink_in_path(p: &str) -> bool {
    let path = std::path::Path::new(p);
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    // Resolve the symlink and continue from the target
                    match std::fs::canonicalize(&current) {
                        Ok(resolved) => {
                            current = resolved;
                        }
                        Err(_) => {
                            // Broken or looping symlink
                            return true;
                        }
                    }
                }
            }
            Err(_) => {
                // Path doesn't exist yet — can't be a symlink
                return false;
            }
        }
    }
    false
}
