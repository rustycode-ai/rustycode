//! Native environment — runs tasks directly on the host without Docker.
//!
//! Uses temporary directories for isolation. Avoids QEMU/Docker overhead
//! on macOS arm64. Commands run via the local shell.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Context;

use super::{BenchEnvironment, ExecResult};

/// Paths used by the native environment (mirrors Docker container paths).
pub mod native_paths {
    pub const LOGS_DIR: &str = "logs";
    pub const VERIFIER_DIR: &str = "logs/verifier";
    pub const AGENT_DIR: &str = "logs/agent";
    pub const ARTIFACTS_DIR: &str = "artifacts";
}

/// Native execution environment — runs tasks on the host filesystem.
///
/// Creates a workspace directory structure that mirrors Docker container paths:
/// ```text
/// workspace/
///   logs/
///     verifier/    # Verifier output (reward.txt)
///     agent/       # Agent logs
///   artifacts/     # Task artifacts
///   tests/         # Test scripts
///   task/          # Task source files
/// ```
pub struct NativeEnvironment {
    /// Workspace root directory.
    workspace: PathBuf,
    /// Task source directory (contains instruction.md, tests/, solution/).
    task_dir: PathBuf,
    /// Whether setup has been run.
    started: bool,
}

impl NativeEnvironment {
    pub fn new(workspace: PathBuf, task_dir: PathBuf) -> Self {
        Self {
            workspace,
            task_dir,
            started: false,
        }
    }

    /// Path to the workspace verifier directory.
    pub fn verifier_dir(&self) -> PathBuf {
        self.workspace.join(native_paths::VERIFIER_DIR)
    }

    /// Path to the workspace agent directory.
    pub fn agent_dir(&self) -> PathBuf {
        self.workspace.join(native_paths::AGENT_DIR)
    }

    /// Path to the workspace artifacts directory.
    pub fn artifacts_dir(&self) -> PathBuf {
        self.workspace.join(native_paths::ARTIFACTS_DIR)
    }

    /// Path to the workspace tests directory.
    pub fn tests_dir(&self) -> PathBuf {
        self.workspace.join("tests")
    }

    /// Execute a script file in the workspace, rewriting container-style paths.
    ///
    /// Reads the script, replaces hardcoded container paths (`/logs/`, `/tests/`,
    /// `/artifacts/`, `/app/`) with `$WORKSPACE/`-prefixed equivalents, strips
    /// Linux-only commands (`apt-get`, `dpkg`), ensures `uv` is available,
    /// writes the modified script to a temp location, and executes it.
    async fn exec_script_inner(
        &self,
        script_path: &Path,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult> {
        let content = std::fs::read_to_string(script_path)
            .with_context(|| format!("reading script {}", script_path.display()))?;

        let adapted = adapt_script_for_native(&content);
        let rewritten = rewrite_container_paths(&adapted, &self.workspace);

        // Write rewritten script to workspace
        let tmp_script = self.workspace.join(".native_run.sh");
        std::fs::write(&tmp_script, rewritten)?;

        let result = self
            .exec_with_timeout(
                &format!(
                    "bash '{}'",
                    tmp_script.display().to_string().replace('\'', "'\\''")
                ),
                timeout_secs,
            )
            .await;

        // Clean up temp script
        let _ = std::fs::remove_file(&tmp_script);

        result
    }

    /// Execute a command in the workspace directory.
    ///
    /// Container-style absolute paths like `/tests/`, `/logs/`, `/tmp/`,
    /// `/artifacts/` are automatically rewritten to workspace-relative paths.
    async fn exec_in_workspace(
        &self,
        command: &str,
        timeout_secs: Option<u64>,
    ) -> anyhow::Result<ExecResult> {
        let mut cmd = rustycode_tools::subprocess::new_tokio_shell_command(command);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&self.workspace)
            .env("WORKSPACE", &self.workspace)
            .env("LOGS_DIR", self.workspace.join(native_paths::LOGS_DIR))
            .env(
                "VERIFIER_DIR",
                self.workspace.join(native_paths::VERIFIER_DIR),
            )
            .env("AGENT_DIR", self.workspace.join(native_paths::AGENT_DIR))
            .env(
                "ARTIFACTS_DIR",
                self.workspace.join(native_paths::ARTIFACTS_DIR),
            );

        let output = if let Some(secs) = timeout_secs {
            let duration = std::time::Duration::from_secs(secs);
            tokio::time::timeout(duration, cmd.output()).await??
        } else {
            cmd.output().await?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ExecResult {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Parse Dockerfile COPY instructions and place files correctly in workspace.
    ///
    /// Docker `COPY <src> <dest>` copies from the build context (environment/) to
    /// the container filesystem. We simulate this by copying from task_dir/environment/
    /// to the appropriate workspace location.
    fn apply_dockerfile_copies(&self, dockerfile: &str) -> anyhow::Result<()> {
        let env_dir = self.task_dir.join("environment");
        for line in dockerfile.lines() {
            let trimmed = line.trim();
            // Skip multi-stage COPYs, comments, empty lines
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if !trimmed.starts_with("COPY ") {
                continue;
            }
            // Skip COPY --from=... (multi-stage builds)
            if trimmed.contains("--from=") {
                continue;
            }

            // Parse: COPY <src> [<src>...] <dest>
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let dest = parts.last().expect("dest exists");
            let sources = &parts[1..parts.len() - 1];

            for src in sources {
                let src_path = env_dir.join(src);
                if !src_path.exists() {
                    continue;
                }

                // Resolve destination relative to /app/ (workspace root)
                let dest_resolved =
                    if *dest == "/app" || *dest == "/app/" || *dest == "." || *dest == "./" {
                        self.workspace.clone()
                    } else if let Some(rest) = dest.strip_prefix("/app/") {
                        self.workspace.join(rest)
                    } else if let Some(rest) = dest.strip_prefix('/') {
                        // Other absolute paths (e.g. /etc/nginx/...)
                        self.workspace.join(rest)
                    } else {
                        self.workspace.join(dest)
                    };

                if src_path.is_dir() {
                    // COPY dir/ /app/dir/
                    copy_dir_recursive_filtered(
                        &src_path,
                        &dest_resolved,
                        &[],
                        Some(&self.workspace),
                    )?;
                } else if dest_resolved.is_dir() || dest.ends_with('/') {
                    // COPY file /app/dir/ → file goes into dir
                    let file_name = src_path.file_name().unwrap_or_default();
                    let target = dest_resolved.join(file_name);
                    copy_file_with_rewrite(&src_path, &target, &self.workspace)?;
                } else {
                    // COPY file /app/renamed_file
                    copy_file_with_rewrite(&src_path, &dest_resolved, &self.workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Execute safe RUN commands from the Dockerfile in the workspace.
    ///
    /// Filters to commands that can run on macOS:
    /// - git clone, wget, curl downloads
    /// - mkdir, cp, mv, chmod, ln
    /// - sed (in-place edits)
    /// - pip install (package setup)
    ///
    /// Skips: apt-get, dpkg, useradd, service, systemctl, apt, yum, apk
    async fn exec_dockerfile_runs(&self, dockerfile: &str) -> anyhow::Result<()> {
        let skip_prefixes = [
            "apt-get",
            "dpkg",
            "apt ",
            "apk ",
            "yum ",
            "dnf ",
            "useradd",
            "groupadd",
            "service ",
            "systemctl",
            "update-alternatives",
            "locale-gen",
            "ln -sf /",
        ];

        let allow_prefixes = [
            "git clone",
            "wget ",
            "curl ",
            "mkdir ",
            "cp ",
            "mv ",
            "chmod ",
            "ln -s",
            "sed ",
            "pip install",
            "pip3 install",
            "python3 -m pip",
            "FLIT_ROOT_INSTALL",
            "env ",
            "flit ",
            "git reset",
            "git remote",
            "git tag",
            "git reflog",
            "git gc",
            "cd ",
            "pip3 ",
        ];

        // First pass: join multi-line RUN commands
        let mut run_commands: Vec<String> = Vec::new();
        let mut current_run = String::new();
        for line in dockerfile.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with("RUN ") {
                if !current_run.is_empty() {
                    run_commands.push(current_run.clone());
                }
                current_run = trimmed
                    .strip_prefix("RUN ")
                    .unwrap_or("")
                    .trim()
                    .to_string();
                // Strip trailing backslash + trailing &&
                if current_run.ends_with('\\') {
                    current_run.pop();
                    current_run = current_run.trim_end().to_string();
                    if current_run.ends_with("&&") {
                        current_run = current_run[..current_run.len() - 2].trim_end().to_string();
                    }
                }
            } else if !current_run.is_empty() {
                // Continuation of previous RUN
                let is_continuation = line.trim().ends_with('\\');
                let mut cont = trimmed.to_string();
                if cont.ends_with('\\') {
                    cont.pop();
                    cont = cont.trim_end().to_string();
                    if cont.ends_with("&&") {
                        cont = cont[..cont.len() - 2].trim_end().to_string();
                    }
                }
                if !current_run.is_empty() {
                    current_run.push_str(" && ");
                }
                current_run.push_str(&cont);
                if !is_continuation {
                    run_commands.push(current_run.clone());
                    current_run.clear();
                }
            }
        }
        if !current_run.is_empty() {
            run_commands.push(current_run);
        }

        // Second pass: filter and execute
        for cmd in &run_commands {
            if cmd.is_empty() {
                continue;
            }

            // Skip unsafe commands
            let first_word = cmd.split_whitespace().next().unwrap_or("");
            if skip_prefixes
                .iter()
                .any(|p| cmd.starts_with(p) || first_word == p.trim())
            {
                continue;
            }

            // Only allow known-safe commands
            if !allow_prefixes.iter().any(|p| cmd.starts_with(p)) {
                continue;
            }

            // Adapt command for macOS
            let adapted = adapt_script_for_native(cmd);
            let rewritten = rewrite_container_paths(&adapted, &self.workspace);

            // Handle git clone into non-empty workspace: clone to tmp, then move.
            // Multi-line chains like "git clone ... /ws && cd /ws && git reset ..."
            // need the whole chain to run against the tmp dir, then we move.
            let ws_str = self.workspace.to_string_lossy().to_string();
            if rewritten.starts_with("git clone") && rewritten.contains(&ws_str) {
                let tmp_dir = format!("{}.git_tmp", ws_str);
                let tmp_rewritten = rewritten.replace(&ws_str, &tmp_dir);
                tracing::debug!("Executing Dockerfile RUN (clone via tmp): {tmp_rewritten}");
                let result = self.exec_in_workspace(&tmp_rewritten, Some(120)).await;
                if let Ok(r) = &result {
                    if r.exit_code == 0 {
                        // Move cloned contents (including dotfiles) into workspace
                        let mv_cmd = format!(
                            "shopt -s dotglob && mv '{tmp_dir}'/* '{ws_str}/' 2>/dev/null; rm -rf '{tmp_dir}'"
                        );
                        let _ = self.exec_in_workspace(&mv_cmd, Some(30)).await;
                    } else {
                        tracing::warn!(
                            "git clone chain exited {}: {}",
                            r.exit_code,
                            r.stderr.trim()
                        );
                    }
                }
                continue;
            }

            tracing::debug!("Executing Dockerfile RUN: {rewritten}");
            let result = self.exec_in_workspace(&rewritten, Some(120)).await;

            match result {
                Ok(r) if r.exit_code == 0 => {}
                Ok(r) => {
                    tracing::debug!(
                        "Dockerfile RUN exited {} (non-fatal): {}",
                        r.exit_code,
                        r.stderr.trim()
                    );
                }
                Err(e) => {
                    tracing::debug!("Dockerfile RUN failed (non-fatal): {e}");
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl BenchEnvironment for NativeEnvironment {
    async fn start(&mut self, _force_build: bool) -> anyhow::Result<()> {
        // Create workspace directories
        std::fs::create_dir_all(self.verifier_dir())?;
        std::fs::create_dir_all(self.agent_dir())?;
        std::fs::create_dir_all(self.artifacts_dir())?;
        std::fs::create_dir_all(self.tests_dir())?;

        // Copy task source files into workspace (excluding _jobs and hidden dirs)
        // Rewrite container paths in text files (tests reference /app/, etc.)
        let task_src = self.workspace.join("task");
        if !self.task_dir.as_os_str().is_empty() {
            copy_dir_recursive_filtered(
                &self.task_dir,
                &task_src,
                &["_jobs"],
                Some(&self.workspace),
            )?;
        }

        // Copy environment source if it exists (e.g. src/ directory)
        let env_src = self.task_dir.join("environment").join("src");
        if env_src.exists() {
            let dest = self.workspace.join("src");
            copy_dir_recursive(&env_src, &dest)?;
        }

        // Copy all non-Dockerfile files from environment/ into workspace root
        // (mirrors Docker COPY and WORKDIR behavior)
        let env_dir = self.task_dir.join("environment");
        if env_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&env_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    // Skip Dockerfile, Dockerfile.*, docker-compose*
                    if name_str.starts_with("Dockerfile")
                        || name_str.starts_with("docker-compose")
                        || name_str == ".dockerignore"
                    {
                        continue;
                    }
                    let src_path = entry.path();
                    let dest_path = self.workspace.join(&*name);
                    if src_path.is_dir() {
                        copy_dir_recursive(&src_path, &dest_path)?;
                    } else {
                        // Copy file, rewriting container paths for text files
                        copy_file_with_rewrite(&src_path, &dest_path, &self.workspace)?;
                    }
                }
            }
        }

        // Pre-install pip packages from the Dockerfile (for Python-based tasks)
        let dockerfile_path = self.task_dir.join("environment").join("Dockerfile");
        if dockerfile_path.exists() {
            if let Ok(dockerfile) = std::fs::read_to_string(&dockerfile_path) {
                let pip_pkgs = extract_pip_packages(&dockerfile);
                if !pip_pkgs.is_empty() {
                    tracing::info!(
                        "Pre-installing {} pip packages from Dockerfile",
                        pip_pkgs.len()
                    );
                    let install_cmd = format!(
                        "pip3 install --break-system-packages {}",
                        pip_pkgs.join(" ")
                    );
                    match self.exec_in_workspace(&install_cmd, Some(300)).await {
                        Ok(result) => {
                            if result.exit_code != 0 {
                                tracing::warn!(
                                    "pip install failed (exit {}): {}",
                                    result.exit_code,
                                    result.stderr.trim()
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!("pip install error: {e}");
                        }
                    }
                }
            }
        }

        // Apply Dockerfile COPY instructions for proper file placement.
        // Docker COPY <src> <dest> copies from environment/ to /app/ (workspace root).
        // Our naive "copy everything from environment/" approach above may place
        // files in wrong subdirectories. COPY instructions give exact placement.
        if dockerfile_path.exists() {
            if let Ok(dockerfile) = std::fs::read_to_string(&dockerfile_path) {
                self.apply_dockerfile_copies(&dockerfile)?;
            }
        }

        // Execute safe RUN commands from Dockerfile.
        // Many tasks use RUN for git clone, wget, chmod, mkdir, sed — all
        // executable on macOS. Skip package managers and root-only commands.
        if dockerfile_path.exists() {
            if let Ok(dockerfile) = std::fs::read_to_string(&dockerfile_path) {
                let _ = self.exec_dockerfile_runs(&dockerfile).await;
            }
        }

        self.started = true;
        tracing::info!("Native environment started: {}", self.workspace.display());
        Ok(())
    }

    async fn stop(&mut self, _delete: bool) -> anyhow::Result<()> {
        // Nothing to stop for native execution
        self.started = false;
        Ok(())
    }

    async fn exec(&self, command: &str) -> anyhow::Result<ExecResult> {
        self.exec_in_workspace(command, None).await
    }

    async fn exec_with_timeout(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult> {
        self.exec_in_workspace(command, Some(timeout_secs)).await
    }

    async fn upload_file(&self, src: &Path, dest: &str) -> anyhow::Result<()> {
        let dest_path = self.workspace.join(dest.trim_start_matches('/'));
        copy_file_with_rewrite(src, &dest_path, &self.workspace)
    }

    async fn download_file(&self, src: &str, dest: &Path) -> anyhow::Result<()> {
        let src_path = self.workspace.join(src.trim_start_matches('/'));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src_path, dest)?;
        Ok(())
    }

    async fn exec_script(
        &self,
        script_path: &Path,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult> {
        self.exec_script_inner(script_path, timeout_secs).await
    }

    fn workspace_path(&self) -> Option<PathBuf> {
        Some(self.workspace.clone())
    }
}

/// Recursively copy a directory, skipping excluded top-level entries.
fn copy_dir_recursive_filtered(
    src: &Path,
    dest: &Path,
    exclude: &[&str],
    workspace: Option<&Path>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    let entries = std::fs::read_dir(src)?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if exclude.contains(&name_str.as_ref()) {
            continue;
        }
        let src_path = entry.path();
        let dest_path = dest.join(&file_name);
        if src_path.is_dir() {
            copy_dir_recursive_filtered(&src_path, &dest_path, &[], workspace)?;
        } else if let Some(wp) = workspace {
            copy_file_with_rewrite(&src_path, &dest_path, wp)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
    copy_dir_recursive_filtered(src, dest, &[], None)
}

/// Extract pip package names from a Dockerfile.
///
/// Parses lines like:
///   `RUN pip install numpy==2.3.0 chess`
///   `RUN pip3 install --no-cache-dir pandas==2.2.3 pyarrow==17.0.0`
///   `RUN python3 -m pip install --upgrade pip==25.2 setuptools==75.6.0`
fn extract_pip_packages(dockerfile: &str) -> Vec<String> {
    let mut packages = Vec::new();

    for line in dockerfile.lines() {
        let trimmed = line.trim();

        // Look for RUN pip/pip3/python3 -m pip install lines
        if !trimmed.starts_with("RUN ") {
            continue;
        }

        let cmd = trimmed.strip_prefix("RUN ").unwrap_or("").trim();

        // Parse: pip install, pip3 install, python3 -m pip install
        let install_args = if cmd.starts_with("pip install ") || cmd.starts_with("pip3 install ") {
            let without_prefix = if cmd.starts_with("pip3 ") {
                cmd.strip_prefix("pip3 install ").unwrap_or("")
            } else {
                cmd.strip_prefix("pip install ").unwrap_or("")
            };
            without_prefix
        } else if cmd.starts_with("python3 -m pip install ") {
            cmd.strip_prefix("python3 -m pip install ").unwrap_or("")
        } else {
            continue;
        };

        // Split on whitespace, skip flags (--no-cache-dir, --upgrade, --break-system-packages)
        for arg in install_args.split_whitespace() {
            if arg.starts_with('-') || arg.starts_with("FLIT_") {
                continue;
            }
            // Keep version pins (numpy==2.3.0) or plain package names,
            // but skip bare commands and meta-packages
            let looks_like_package = arg.contains('.')
                || arg.contains('=')
                || arg.contains('>')
                || arg
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
            let is_meta = arg == "install"
                || arg.starts_with("pip")
                || arg.starts_with("setuptools")
                || arg.starts_with("wheel");
            if looks_like_package && !is_meta {
                packages.push(arg.to_string());
            }
        }
    }

    packages
}

/// Adapt a shell script for native (non-container) execution.
///
/// TB2 test.sh scripts follow a standard pattern:
/// 1. `apt-get update && apt-get install -y curl [extra-pkgs]`
/// 2. `curl -LsSf https://astral.sh/uv/.../install.sh | sh`
/// 3. `source $HOME/.local/bin/env`
/// 4. `uvx ... pytest ...`
///
/// On macOS, steps 1-2 fail because `apt-get` doesn't exist and `uv` may
/// already be installed. This function:
/// - Strips `apt-get` and `dpkg` lines
/// - Replaces the `curl | sh` uv installer with a guard that skips if `uv` is already available
/// - Keeps everything else intact
fn adapt_script_for_native(script: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_apt_block = false;

    for line in script.lines() {
        let trimmed = line.trim();

        // Skip apt-get commands (update, install, and continuations)
        if trimmed.starts_with("apt-get update") {
            // apt-get update has no continuations
            continue;
        }
        if trimmed.starts_with("apt-get install") || trimmed.starts_with("apt-get\tinstall") {
            in_apt_block = true;
            continue;
        }
        if in_apt_block {
            // apt-get install often has continuation lines ending with \
            if trimmed.ends_with('\\') {
                continue;
            }
            // If the line is a package name continuation (no command prefix)
            if !trimmed.starts_with('#')
                && !trimmed.contains('=')
                && !trimmed.starts_with("curl")
                && !trimmed.starts_with("source")
                && !trimmed.starts_with("uvx")
                && !trimmed.starts_with("uv ")
                && !trimmed.starts_with("if ")
                && !trimmed.starts_with("fi")
                && !trimmed.starts_with("echo")
                && !trimmed.contains("pytest")
                && !trimmed.contains("reward")
                && !trimmed.is_empty()
            {
                continue;
            }
            in_apt_block = false;
        }

        // Skip dpkg commands
        if trimmed.starts_with("dpkg ") {
            continue;
        }

        // Replace curl-based uv installer with a guard
        if trimmed.contains("astral.sh/uv") && trimmed.contains("install.sh") {
            lines.push("# [native] skip uv install — use system uv".to_string());
            lines.push("if ! command -v uv &>/dev/null; then".to_string());
            lines.push("  echo 'ERROR: uv not found — please install uv first'".to_string());
            lines.push("  exit 1".to_string());
            lines.push("fi".to_string());
            // If the original line was `curl ... | sh`, skip the `| sh` continuation
            continue;
        }

        // Skip `source $HOME/.local/bin/env` if uv is already system-installed
        // (harmless but noisy if the file doesn't exist)
        if trimmed.starts_with("source ") && trimmed.contains(".local/bin/env") {
            lines.push("# [native] sourcing uv env if present".to_string());
            lines.push(
                "[ -f \"$HOME/.local/bin/env\" ] && source \"$HOME/.local/bin/env\" || true"
                    .to_string(),
            );
            continue;
        }

        lines.push(line.to_string());
    }

    let result = lines.join("\n");

    // Apply macOS compatibility fixes.
    // Rust's regex crate doesn't support lookaround, so we scan byte-by-byte.
    let mut adapted = String::with_capacity(result.len() + 128);
    let bytes = result.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // 1. "python3" — skip as-is
        if i + 7 <= len && &bytes[i..i + 7] == b"python3" {
            adapted.push_str("python3");
            i += 7;
            continue;
        }
        // 2. "python" (standalone, not part of longer word) → "python3"
        if i + 6 <= len && &bytes[i..i + 6] == b"python" {
            let preceded = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            let followed = i + 6 < len && bytes[i + 6].is_ascii_alphanumeric();
            if !preceded && !followed {
                adapted.push_str("python3");
            } else {
                adapted.push_str("python");
            }
            i += 6;
            continue;
        }
        // 3. "pip3 install" — already correct, skip
        if i + 11 <= len && &bytes[i..i + 11] == b"pip3 install" {
            adapted.push_str("pip3 install");
            i += 11;
            continue;
        }
        // 4. "pip install" (standalone) → "pip3 install"
        if i + 11 <= len && &bytes[i..i + 11] == b"pip install" {
            let preceded = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if !preceded {
                adapted.push_str("pip3 install");
                i += 11;
                continue;
            }
        }
        // 5. "sed -i '" → "sed -i '' '" (GNU → macOS in-place edit)
        //    macOS sed requires -i '' instead of bare -i.
        if i + 7 <= len && &bytes[i..i + 7] == b"sed -i " {
            // Check if the next char is a quote (GNU syntax: sed -i '...')
            let next_is_quote = i + 7 < len && (bytes[i + 7] == b'\'' || bytes[i + 7] == b'"');
            if next_is_quote {
                let quote = bytes[i + 7] as char;
                adapted.push_str("sed -i '' ");
                adapted.push(quote);
                i += 8; // skip "sed -i " + quote
                continue;
            }
            // Handle: sed -i" (no space between -i and quote)
            // Already covered by the space check above — no space case handled below
        }
        // 6. "sed -i\"" or "sed -i'" (no space before quote) → "sed -i '' quote"
        if i + 6 <= len && &bytes[i..i + 6] == b"sed -i" {
            if i + 6 < len && (bytes[i + 6] == b'\'' || bytes[i + 6] == b'"') {
                let quote = bytes[i + 6] as char;
                adapted.push_str("sed -i '' ");
                adapted.push(quote);
                i += 7; // skip "sed -i" + quote
                continue;
            }
            // "sed -iE" or "sed -i -E" — macOS sed -i '' -E works, but -iE doesn't
            // Handle "sed -iE" → "sed -i '' -E" or "sed -i -E" → "sed -i '' -E"
            if i + 7 <= len && &bytes[i..i + 7] == b"sed -iE" {
                adapted.push_str("sed -i '' -E");
                i += 7;
                continue;
            }
            if i + 7 <= len
                && &bytes[i..i + 7] == b"sed -i "
                && i + 9 <= len
                && &bytes[i + 7..i + 9] == b"-E"
            {
                adapted.push_str("sed -i '' -E");
                i += 9;
                continue;
            }
        }

        adapted.push(bytes[i] as char);
        i += 1;
    }

    adapted
}

/// Copy a file, rewriting container paths for text files.
fn copy_file_with_rewrite(src: &Path, dest: &Path, workspace: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let ext = src.extension().map(|e| e.to_string_lossy().to_string());
    let is_text = ext
        .as_ref()
        .map(|e| {
            [
                "py", "txt", "json", "yaml", "yml", "toml", "cfg", "sh", "md",
            ]
            .contains(&e.as_str())
        })
        .unwrap_or(false);

    if is_text {
        if let Ok(content) = std::fs::read_to_string(src) {
            let rewritten = rewrite_container_paths(&content, workspace);
            std::fs::write(dest, rewritten)?;
            return Ok(());
        }
    }

    std::fs::copy(src, dest)?;
    Ok(())
}

/// Rewrite container-style absolute paths in a script to workspace-relative paths.
///
/// Only replaces paths that appear as standalone references (preceded by space,
/// `>`, `"`, `'`, `=`, or start of line) to avoid double-rewriting.
fn rewrite_container_paths(script: &str, workspace: &Path) -> String {
    let wp = workspace.to_string_lossy();
    let mut result = script.to_string();

    // Replace container paths — must be at a path boundary (not mid-path)
    // We match patterns like " /logs/", "> /logs/", etc. and the path itself.
    let container_paths = [
        "/logs/verifier/",
        "/logs/agent/",
        "/logs/",
        "/artifacts/",
        "/tests/",
        "/app/",
        "/app ",
        "/app\"",
        "/app'",
        "/app)",
        "/app\n",
        "/tmp/solve.sh",
    ];

    // /app/ and /app map to workspace root (Docker WORKDIR = /app)
    let workspace_paths: Vec<String> = container_paths
        .iter()
        .map(|p| {
            if p.starts_with("/app") && !p.starts_with("/app/") {
                // /app followed by a boundary char: replace just "/app" with workspace
                let suffix = &p[4..];
                format!("{wp}{suffix}")
            } else if *p == "/app/" {
                format!("{wp}/")
            } else {
                format!("{wp}{p}")
            }
        })
        .collect();

    // Single pass: replace all container paths in one go
    // Use a marker approach to avoid double-replacement
    for (i, container_path) in container_paths.iter().enumerate() {
        let workspace_path = &workspace_paths[i];
        // Only replace when the container path appears as a standalone reference
        // (preceded by a non-path character or at the start of the content)
        let mut new_result = String::with_capacity(result.len());
        let mut last_end = 0;
        while let Some(pos) = result[last_end..].find(container_path) {
            let abs_pos = last_end + pos;
            // Check if this is a standalone path reference
            let is_standalone = if abs_pos == 0 {
                true
            } else {
                let prev_char = result.as_bytes()[abs_pos - 1];
                matches!(
                    prev_char,
                    b' ' | b'>'
                        | b'"'
                        | b'\''
                        | b'='
                        | b'\n'
                        | b'\r'
                        | b'\t'
                        | b';'
                        | b'|'
                        | b'('
                        | b')'
                        | b'`'
                        | b'$'
                        | b','
                        | b'{'
                        | b'['
                        | b'&'
                )
            };
            new_result.push_str(&result[last_end..abs_pos]);
            if is_standalone {
                new_result.push_str(workspace_path);
            } else {
                new_result.push_str(container_path);
            }
            last_end = abs_pos + container_path.len();
        }
        new_result.push_str(&result[last_end..]);
        result = new_result;
    }

    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_env_start_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        std::fs::create_dir_all(&task_dir).unwrap();

        let workspace = dir.path().join("workspace");
        let mut env = NativeEnvironment::new(workspace.clone(), task_dir);

        env.start(false).await.unwrap();

        assert!(workspace.join("logs/verifier").exists());
        assert!(workspace.join("logs/agent").exists());
        assert!(workspace.join("artifacts").exists());
        assert!(workspace.join("tests").exists());
    }

    #[tokio::test]
    async fn native_env_exec_runs_command() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        std::fs::create_dir_all(&task_dir).unwrap();

        let workspace = dir.path().join("workspace");
        let mut env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.start(false).await.unwrap();

        let result = env.exec("echo hello").await.unwrap();
        assert!(result.success());
        assert!(result.stdout.trim() == "hello");
    }

    #[tokio::test]
    async fn native_env_exec_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        std::fs::create_dir_all(&task_dir).unwrap();

        let workspace = dir.path().join("workspace");
        let mut env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.start(false).await.unwrap();

        let result = env.exec_with_timeout("sleep 10", 1).await;
        // Should timeout
        assert!(result.is_err() || !result.unwrap().success());
    }

    #[tokio::test]
    async fn native_env_upload_download() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        std::fs::create_dir_all(&task_dir).unwrap();

        // Create a source file
        let src_file = dir.path().join("test.txt");
        std::fs::write(&src_file, "test content").unwrap();

        let workspace = dir.path().join("workspace");
        let mut env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.start(false).await.unwrap();

        // Upload
        env.upload_file(&src_file, "/logs/test.txt").await.unwrap();
        assert!(workspace.join("logs/test.txt").exists());

        // Download
        let dest_file = dir.path().join("downloaded.txt");
        env.download_file("/logs/test.txt", &dest_file)
            .await
            .unwrap();
        let content = std::fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[tokio::test]
    async fn native_env_stop_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        std::fs::create_dir_all(&task_dir).unwrap();

        let workspace = dir.path().join("workspace");
        let mut env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.start(false).await.unwrap();
        env.stop(false).await.unwrap();
    }

    #[test]
    fn copy_dir_recursive_copies_files() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        std::fs::write(src.path().join("a.txt"), "aaa").unwrap();
        let sub = src.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("b.txt"), "bbb").unwrap();

        let dest_path = dest.path().join("copy");
        copy_dir_recursive(src.path(), &dest_path).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest_path.join("a.txt")).unwrap(),
            "aaa"
        );
        assert_eq!(
            std::fs::read_to_string(dest_path.join("sub/b.txt")).unwrap(),
            "bbb"
        );
    }

    #[test]
    fn rewrite_container_paths_basic() {
        let wp = "/tmp/workspace";
        let script = "echo 1 > /logs/verifier/reward.txt\necho 0 > /logs/verifier/reward.txt\n";
        let result = rewrite_container_paths(script, Path::new(wp));
        assert!(result.contains("/tmp/workspace/logs/verifier/reward.txt"));
        assert!(!result.contains(">/logs/"));
    }

    #[test]
    fn rewrite_container_paths_tests() {
        let wp = "/home/user/ws";
        let script = "pytest /tests/test_outputs.py --ctrf /logs/verifier/ctrf.json\n";
        let result = rewrite_container_paths(script, Path::new(wp));
        assert!(result.contains("/home/user/ws/tests/test_outputs.py"));
        assert!(result.contains("/home/user/ws/logs/verifier/ctrf.json"));
    }

    #[test]
    fn rewrite_container_paths_preserves_other() {
        let wp = "/tmp/ws";
        let script = "apt-get update\npip3 install foo\necho done\n";
        let result = rewrite_container_paths(script, Path::new(wp));
        assert_eq!(result, script);
    }

    #[test]
    fn rewrite_container_paths_app_dir() {
        let wp = "/tmp/ws";
        let script = "cd /app && python main.py\n";
        let result = rewrite_container_paths(script, Path::new(wp));
        // /app (no slash) maps to workspace root
        assert!(result.contains("/tmp/ws "));
        assert!(!result.contains("cd /app"));
    }

    #[test]
    fn rewrite_container_paths_app_subpath() {
        let wp = "/tmp/ws";
        let script = "cat > /app/regex.txt << EOF\nhello\nEOF\n";
        let result = rewrite_container_paths(script, Path::new(wp));
        // /app/ should map to workspace root, not workspace/app/
        assert!(result.contains("/tmp/ws/regex.txt"));
        assert!(!result.contains("/app/"));
    }

    #[test]
    fn rewrite_container_paths_python_sys_path() {
        let wp = "/tmp/ws";
        let script = r#"sys.path.append("/app")"#;
        let result = rewrite_container_paths(script, Path::new(wp));
        assert!(result.contains("/tmp/ws\")"), "got: {result}");
        assert!(!result.contains("/app"));
    }

    #[test]
    fn rewrite_container_paths_python_path_object() {
        let wp = "/tmp/ws";
        let script = r#"report_path = Path("/app/report.jsonl")"#;
        let result = rewrite_container_paths(script, Path::new(wp));
        assert!(result.contains("/tmp/ws/report.jsonl"));
        assert!(!result.contains("/app/"));
    }

    #[test]
    fn rewrite_container_paths_python_assert() {
        let wp = "/tmp/ws";
        let script = r#"assert "/app/bottle.py" in file_paths"#;
        let result = rewrite_container_paths(script, Path::new(wp));
        assert!(result.contains("/tmp/ws/bottle.py"));
    }

    #[tokio::test]
    async fn native_env_exec_script_rewrites_paths() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        std::fs::create_dir_all(&task_dir).unwrap();

        let workspace = dir.path().join("workspace");
        let mut env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.start(false).await.unwrap();

        // Create a script that writes to a container path
        let script = dir.path().join("test_script.sh");
        std::fs::write(
            &script,
            "#!/bin/bash\necho test_reward > /logs/verifier/reward.txt\n",
        )
        .unwrap();

        let result = env.exec_script(&script, 10).await.unwrap();
        assert!(result.success(), "Script failed: {}", result.stderr);

        // Verify the file was written to the workspace path
        let reward_path = workspace.join("logs/verifier/reward.txt");
        assert!(
            reward_path.exists(),
            "Reward file not found at {reward_path:?}"
        );
        let reward = std::fs::read_to_string(&reward_path).unwrap();
        assert_eq!(reward.trim(), "test_reward");
    }

    // --- adapt_script_for_native tests ---

    #[test]
    fn adapt_strips_apt_get() {
        let script = "#!/bin/bash\n\
apt-get update\n\
apt-get install -y curl\n\
curl -LsSf https://astral.sh/uv/0.9.5/install.sh | sh\n\
source $HOME/.local/bin/env\n\
uvx -p 3.13 pytest /tests/test_outputs.py\n";
        let adapted = adapt_script_for_native(script);
        eprintln!("ADAPTED:\n{adapted}");
        assert!(!adapted.contains("apt-get"), "apt-get should be stripped");
        assert!(
            !adapted.contains("astral.sh"),
            "curl uv installer should be replaced"
        );
        assert!(adapted.contains("uvx"), "uvx commands should be preserved");
        assert!(adapted.contains("pytest"), "pytest should be preserved");
        assert!(
            adapted.contains("command -v uv"),
            "uv guard should be added"
        );
    }

    #[test]
    fn adapt_preserves_non_apt() {
        let script = "#!/bin/bash\necho hello\nls -la\n";
        let adapted = adapt_script_for_native(script);
        // adapt_script_for_native uses lines() which drops the trailing newline
        assert!(adapted.contains("echo hello"));
        assert!(adapted.contains("ls -la"));
        assert!(!adapted.contains("apt-get"));
    }

    #[test]
    fn adapt_tb2_style_test_sh() {
        let script = [
            "#!/bin/bash",
            "",
            "# Install curl",
            "apt-get update",
            "apt-get install -y curl primer3",
            "",
            "# Install uv",
            "curl -LsSf https://astral.sh/uv/0.9.5/install.sh | sh",
            "",
            "source $HOME/.local/bin/env",
            "",
            "uvx \\",
            "  -p 3.13 \\",
            "  -w pytest==8.4.1 \\",
            "  -w pytest-json-ctrf==0.3.5 \\",
            "  pytest --ctrf /logs/verifier/ctrf.json /tests/test_outputs.py -rA",
            "",
            "",
            "if [ $? -eq 0 ]; then",
            "  echo 1 > /logs/verifier/reward.txt",
            "else",
            "  echo 0 > /logs/verifier/reward.txt",
            "fi",
        ]
        .join("\n");
        let adapted = adapt_script_for_native(&script);

        // apt-get should be stripped
        assert!(!adapted.contains("apt-get update"));
        assert!(!adapted.contains("apt-get install"));

        // curl uv installer should be replaced with guard
        assert!(!adapted.contains("astral.sh"));
        assert!(adapted.contains("command -v uv"));

        // source should be guarded
        assert!(adapted.contains(".local/bin/env"));

        // uvx and pytest should be preserved
        assert!(adapted.contains("uvx"));
        assert!(adapted.contains("pytest"));
        assert!(adapted.contains("reward.txt"));
        assert!(adapted.contains("echo 1"));
        assert!(adapted.contains("echo 0"));
    }

    #[test]
    fn adapt_source_env_guarded() {
        let script = "source $HOME/.local/bin/env\n";
        let adapted = adapt_script_for_native(script);
        assert!(adapted.contains("[ -f \"$HOME/.local/bin/env\" ]"));
    }

    #[test]
    fn adapt_skips_dpkg() {
        let script = "dpkg --configure -a\napt-get update\necho done\n";
        let adapted = adapt_script_for_native(script);
        assert!(!adapted.contains("dpkg"));
        assert!(!adapted.contains("apt-get"));
        assert!(adapted.contains("echo done"));
    }

    #[test]
    fn extract_pip_packages_basic() {
        let dockerfile = "FROM python:3.13-slim\nRUN pip install chess\n";
        let pkgs = extract_pip_packages(dockerfile);
        assert_eq!(pkgs, vec!["chess"]);
    }

    #[test]
    fn extract_pip_packages_with_versions() {
        let dockerfile = "FROM python:3.13-slim\nRUN pip install numpy==2.3.0 pandas==2.2.3\n";
        let pkgs = extract_pip_packages(dockerfile);
        assert_eq!(pkgs, vec!["numpy==2.3.0", "pandas==2.2.3"]);
    }

    #[test]
    fn extract_pip_packages_skips_flags() {
        let dockerfile =
            "FROM python:3.13-slim\nRUN pip install --no-cache-dir --break-system-packages pillow==11.2.1\n";
        let pkgs = extract_pip_packages(dockerfile);
        assert_eq!(pkgs, vec!["pillow==11.2.1"]);
    }

    #[test]
    fn extract_pip_packages_python3_m_pip() {
        let dockerfile =
            "FROM python:3.13-slim\nRUN python3 -m pip install --upgrade pip==25.2 setuptools==75.6.0 wheel==0.45.1\n";
        let pkgs = extract_pip_packages(dockerfile);
        // pip, setuptools, wheel are skipped
        assert!(pkgs.is_empty());
    }

    #[test]
    fn extract_pip_packages_empty_dockerfile() {
        let dockerfile = "FROM ubuntu:22.04\nRUN apt-get update\n";
        let pkgs = extract_pip_packages(dockerfile);
        assert!(pkgs.is_empty());
    }

    #[test]
    fn extract_pip_packages_pip3() {
        let dockerfile = "FROM python:3.13\nRUN pip3 install numpy==2.3.1 pillow==11.2.1 --break-system-packages\n";
        let pkgs = extract_pip_packages(dockerfile);
        assert_eq!(pkgs, vec!["numpy==2.3.1", "pillow==11.2.1"]);
    }

    // --- macOS compatibility adaptation tests ---

    #[test]
    fn adapt_pip_to_pip3() {
        let script = "pip install numpy==2.3.0\npip install --no-cache-dir chess\n";
        let adapted = adapt_script_for_native(script);
        assert!(adapted.contains("pip3 install numpy"));
        assert!(adapted.contains("pip3 install --no-cache-dir chess"));
        assert!(!adapted.contains("pip install"));
    }

    #[test]
    fn adapt_pip3_unchanged() {
        let script = "pip3 install numpy\n";
        let adapted = adapt_script_for_native(script);
        assert!(adapted.contains("pip3 install numpy"));
    }

    #[test]
    fn adapt_python_to_python3() {
        let script = "python main.py\npython3 other.py\n";
        let adapted = adapt_script_for_native(script);
        assert!(adapted.contains("python3 main.py"));
        assert!(adapted.contains("python3 other.py"));
        // Should not double-replace python3 to python33
        assert!(!adapted.contains("python33"));
    }

    #[test]
    fn adapt_sed_i_gnu_to_macos() {
        let script = "sed -i 's/foo/bar/g' file.txt\n";
        let adapted = adapt_script_for_native(script);
        assert!(adapted.contains("sed -i '' 's/foo/bar/g' file.txt"));
        assert!(!adapted.contains("sed -i 's/foo"));
    }

    #[test]
    fn adapt_sed_i_multiple_patterns() {
        let script = "sed -i 's/#PermitRootLogin/PermitRootLogin yes/' config\nsed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' config\n";
        let adapted = adapt_script_for_native(script);
        assert!(!adapted.contains("sed -i 's/#"));
        assert!(adapted.contains("sed -i '' 's/#PermitRootLogin"));
        assert!(adapted.contains("sed -i '' 's/#PasswordAuthentication"));
    }

    #[test]
    fn adapt_sed_i_with_find_xargs() {
        let script = "find . -type f | xargs sed -i -E \"s/AKIA[0-9A-Z]{16}/REDACTED/g\"\n";
        let adapted = adapt_script_for_native(script);
        // sed -i" (no space) variant - should get '' inserted
        assert!(adapted.contains("sed -i ''"));
    }

    #[test]
    fn adapt_combined_fixes() {
        let script =
            "apt-get update\npip install numpy\nsed -i 's/old/new/g' file.txt\npython script.py\n";
        let adapted = adapt_script_for_native(script);
        assert!(!adapted.contains("apt-get"));
        assert!(adapted.contains("pip3 install numpy"));
        assert!(adapted.contains("sed -i '' 's/old/new/g'"));
        assert!(adapted.contains("python3 script.py"));
    }

    #[test]
    fn dockerfile_copy_parsing() {
        let dockerfile = "\
FROM python:3.13-slim
WORKDIR /app
COPY filter.py /app/
COPY tests/test_outputs.py /app
RUN pip install pytest
";
        let dir = tempfile::tempdir().unwrap();
        // Task dir structure: task/environment/
        let task_dir = dir.path().join("task");
        let env_dir = task_dir.join("environment");
        std::fs::create_dir_all(env_dir.join("tests")).unwrap();
        std::fs::write(env_dir.join("filter.py"), "# filter").unwrap();
        std::fs::write(env_dir.join("tests").join("test_outputs.py"), "# test").unwrap();

        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.apply_dockerfile_copies(dockerfile).unwrap();

        assert!(
            workspace.join("filter.py").exists(),
            "filter.py not at workspace root"
        );
        assert!(
            workspace.join("test_outputs.py").exists(),
            "test_outputs.py not at workspace root"
        );
    }

    #[test]
    fn dockerfile_copy_dir() {
        let dockerfile = "FROM python:3.13\nCOPY src/ /app/src/\n";
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        let env_dir = task_dir.join("environment");
        std::fs::create_dir_all(env_dir.join("src")).unwrap();
        std::fs::write(env_dir.join("src").join("main.py"), "print('hi')").unwrap();

        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.apply_dockerfile_copies(dockerfile).unwrap();

        assert!(
            workspace.join("src").join("main.py").exists(),
            "src/main.py not copied"
        );
    }

    #[test]
    fn dockerfile_copy_skips_multistage() {
        let dockerfile =
            "FROM python:3.13\nCOPY --from=build /app/dist /app/dist\nCOPY data.csv /app/\n";
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("task");
        let env_dir = task_dir.join("environment");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(env_dir.join("data.csv"), "a,b,c").unwrap();

        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let env = NativeEnvironment::new(workspace.clone(), task_dir);
        env.apply_dockerfile_copies(dockerfile).unwrap();

        assert!(workspace.join("data.csv").exists(), "data.csv not copied");
        assert!(
            !workspace.join("dist").exists(),
            "multistage COPY should be skipped"
        );
    }
}
