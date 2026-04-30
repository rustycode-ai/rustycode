//! Diagnostics Collection
//!
//! Collects system information and application state for bug reporting
//! and debugging. Exports a structured snapshot to a directory or
//! JSON file without requiring a ZIP dependency.
//!
//! Inspired by goose's `session/diagnostics.rs`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::diagnostics::DiagnosticsCollector;
//!
//! let snapshot = DiagnosticsCollector::collect();
//!
//! // Write to a directory
//! snapshot.export_to_dir("/tmp/rustycode-diagnostics").unwrap();
//!
//! // Or get as JSON
//! let json = snapshot.to_json_pretty().unwrap();
//! ```

use crate::app_paths::AppPaths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of log files to include in diagnostics.
const MAX_LOG_FILES: usize = 5;

/// System and application information snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Application version
    pub app_version: String,
    /// Operating system name
    pub os: String,
    /// OS version/release
    pub os_version: String,
    /// CPU architecture
    pub architecture: String,
    /// Number of CPU cores
    pub num_cpus: usize,
    /// Total system memory in bytes (if available)
    pub total_memory_bytes: Option<u64>,
    /// Configured LLM provider (if any)
    pub provider: Option<String>,
    /// Configured model (if any)
    pub model: Option<String>,
    /// Timestamp when the snapshot was collected
    pub collected_at: String,
}

impl SystemInfo {
    /// Collect current system information.
    pub fn collect() -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            os_version: get_os_version(),
            architecture: std::env::consts::ARCH.to_string(),
            num_cpus: num_cpus(),
            total_memory_bytes: get_total_memory(),
            provider: None,
            model: None,
            collected_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Render as human-readable text.
    pub fn to_text(&self) -> String {
        format!(
            "App Version: {}\n\
             OS: {}\n\
             OS Version: {}\n\
             Architecture: {}\n\
             CPUs: {}\n\
             Total Memory: {}\n\
             Provider: {}\n\
             Model: {}\n\
             Collected At: {}\n",
            self.app_version,
            self.os,
            self.os_version,
            self.architecture,
            self.num_cpus,
            self.total_memory_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "unknown".to_string()),
            self.provider.as_deref().unwrap_or("unknown"),
            self.model.as_deref().unwrap_or("unknown"),
            self.collected_at,
        )
    }
}

/// A diagnostics snapshot containing system info and collected files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    /// System information
    pub system_info: SystemInfo,
    /// Files included in the snapshot (relative_path → content)
    pub files: Vec<DiagnosticsFile>,
    /// Total size in bytes
    pub total_size_bytes: usize,
}

/// A single file included in diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsFile {
    /// Relative path within the diagnostics bundle
    pub path: String,
    /// Size in bytes
    pub size_bytes: usize,
    /// Whether the file was successfully read
    pub read_success: bool,
    /// Error message if read failed
    pub error: Option<String>,
}

/// Diagnostics collector that gathers system state.
pub struct DiagnosticsCollector;

impl DiagnosticsCollector {
    /// Collect a full diagnostics snapshot.
    pub fn collect() -> DiagnosticsSnapshot {
        let system_info = SystemInfo::collect();
        let mut files = Vec::new();
        let mut total_size = 0usize;

        // Collect log files
        let logs_dir = AppPaths::in_state_dir("logs");
        if logs_dir.exists() {
            let dir_entries = match fs::read_dir(&logs_dir) {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!("Failed to read logs dir: {}", e);
                    return DiagnosticsSnapshot {
                        system_info,
                        files,
                        total_size_bytes: total_size,
                    };
                }
            };
            let mut log_entries: Vec<_> = dir_entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "log" || ext == "jsonl")
                })
                .collect();

            log_entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

            for entry in log_entries.iter().rev().take(MAX_LOG_FILES) {
                let path = entry.path();
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => continue,
                };
                let file = collect_file(&format!("logs/{}", name), &path);
                total_size += file.size_bytes;
                files.push(file);
            }
        }

        // Collect config if present
        let config_path = AppPaths::in_config_dir("config.yaml");
        if config_path.exists() {
            let file = collect_file("config/config.yaml", &config_path);
            total_size += file.size_bytes;
            files.push(file);
        }

        // Collect permissions if present
        let perm_path = AppPaths::in_config_dir("tool_permissions.yaml");
        if perm_path.exists() {
            let file = collect_file("config/tool_permissions.yaml", &perm_path);
            total_size += file.size_bytes;
            files.push(file);
        }

        // Collect recent session files
        let sessions_dir = AppPaths::in_data_dir("sessions");
        if sessions_dir.exists() {
            if let Ok(entries) = fs::read_dir(&sessions_dir) {
                let mut session_entries: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                    .collect();

                session_entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

                for entry in session_entries.iter().rev().take(3) {
                    let path = entry.path();
                    let name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n,
                        None => continue,
                    };
                    let file = collect_file(&format!("sessions/{}", name), &path);
                    total_size += file.size_bytes;
                    files.push(file);
                }
            }
        }

        DiagnosticsSnapshot {
            system_info,
            files,
            total_size_bytes: total_size,
        }
    }

    /// Collect and export diagnostics to a directory.
    ///
    /// Creates the directory structure and writes all collected files.
    pub fn export_to_dir(output_dir: &str) -> Result<PathBuf> {
        let snapshot = Self::collect();
        let dir = PathBuf::from(output_dir);
        fs::create_dir_all(&dir)?;

        // Write system info
        let sysinfo_path = dir.join("system.txt");
        fs::write(&sysinfo_path, snapshot.system_info.to_text())?;

        // Write full snapshot as JSON
        let snapshot_path = dir.join("snapshot.json");
        fs::write(&snapshot_path, snapshot.to_json_pretty()?)?;

        // Write collected files
        for file in &snapshot.files {
            let file_path = dir.join(&file.path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Only write if the file was successfully read
            if file.read_success {
                let source = find_source_for_diagnostics_file(&file.path);
                if let Ok(content) = fs::read(&source) {
                    fs::write(&file_path, content)?;
                }
            }
        }

        Ok(dir)
    }
}

impl DiagnosticsSnapshot {
    /// Serialize to pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Serialize to compact JSON.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// Number of files in the snapshot.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Number of files that were successfully read.
    pub fn successful_file_count(&self) -> usize {
        self.files.iter().filter(|f| f.read_success).count()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Collect metadata about a single file for diagnostics.
fn collect_file(relative_path: &str, full_path: &PathBuf) -> DiagnosticsFile {
    match fs::metadata(full_path) {
        Ok(meta) => DiagnosticsFile {
            path: relative_path.to_string(),
            size_bytes: meta.len() as usize,
            read_success: true,
            error: None,
        },
        Err(e) => DiagnosticsFile {
            path: relative_path.to_string(),
            size_bytes: 0,
            read_success: false,
            error: Some(e.to_string()),
        },
    }
}

/// Map a diagnostics relative path back to a source file path.
fn find_source_for_diagnostics_file(relative_path: &str) -> PathBuf {
    if relative_path.starts_with("logs/") {
        AppPaths::in_state_dir(relative_path)
    } else if relative_path.starts_with("config/") {
        let name = relative_path.strip_prefix("config/").unwrap();
        AppPaths::in_config_dir(name)
    } else if relative_path.starts_with("sessions/") {
        let name = relative_path.strip_prefix("sessions/").unwrap();
        AppPaths::in_data_dir(name)
    } else {
        PathBuf::from(relative_path)
    }
}

/// Get OS version string (cross-platform best-effort).
fn get_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    #[cfg(target_os = "linux")]
    {
        // Try /etc/os-release first
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return line
                        .strip_prefix("PRETTY_NAME=")
                        .unwrap()
                        .trim_matches('"')
                        .to_string();
                }
            }
        }
        "unknown".to_string()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        "unknown".to_string()
    }
}

/// Get the number of CPUs.
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Get total system memory in bytes (best-effort).
fn get_total_memory() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()
            .ok()?;
        let size_str = String::from_utf8_lossy(&output.stdout);
        size_str.trim().parse::<u64>().ok()
    }

    #[cfg(target_os = "linux")]
    {
        let content = fs::read_to_string("/proc/meminfo").ok()?;
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb: u64 = parts[1].parse().ok()?;
                    return Some(kb * 1024);
                }
            }
        }
        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Format bytes as human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info_collect() {
        let info = SystemInfo::collect();
        assert!(!info.app_version.is_empty());
        assert!(!info.os.is_empty());
        assert!(!info.architecture.is_empty());
        assert!(info.num_cpus > 0);
        assert!(!info.collected_at.is_empty());
    }

    #[test]
    fn test_system_info_to_text() {
        let info = SystemInfo::collect();
        let text = info.to_text();
        assert!(text.contains("App Version:"));
        assert!(text.contains("OS:"));
        assert!(text.contains("Architecture:"));
        assert!(text.contains("CPUs:"));
    }

    #[test]
    fn test_system_info_with_provider() {
        let mut info = SystemInfo::collect();
        info.provider = Some("anthropic".to_string());
        info.model = Some("claude-sonnet-4-6".to_string());
        let text = info.to_text();
        assert!(text.contains("anthropic"));
        assert!(text.contains("claude-sonnet"));
    }

    #[test]
    fn test_system_info_serialization() {
        let info = SystemInfo::collect();
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: SystemInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.os, deserialized.os);
        assert_eq!(info.architecture, deserialized.architecture);
    }

    #[test]
    fn test_diagnostics_collect() {
        let snapshot = DiagnosticsCollector::collect();
        assert!(!snapshot.system_info.os.is_empty());
        // Files may or may not exist depending on test environment
        // total_size_bytes is u64, always >= 0 by definition
    }

    #[test]
    fn test_diagnostics_snapshot_json() {
        let snapshot = DiagnosticsCollector::collect();
        let json = snapshot.to_json().unwrap();
        assert!(json.contains("system_info"));
        assert!(json.contains("files"));

        let pretty = snapshot.to_json_pretty().unwrap();
        assert!(pretty.contains("system_info"));
    }

    #[test]
    fn test_diagnostics_snapshot_counts() {
        let snapshot = DiagnosticsCollector::collect();
        // file_count() and successful_file_count() return usize, always >= 0
        assert!(snapshot.successful_file_count() <= snapshot.file_count());
    }

    #[test]
    fn test_diagnostics_file_collect_success() {
        let temp = tempfile::tempdir().unwrap();
        let test_file = temp.path().join("test.log");
        fs::write(&test_file, "hello world").unwrap();

        let file = collect_file("logs/test.log", &test_file);
        assert!(file.read_success);
        assert_eq!(file.size_bytes, 11);
        assert!(file.error.is_none());
    }

    #[test]
    fn test_diagnostics_file_collect_missing() {
        let file = collect_file("logs/missing.log", &PathBuf::from("/nonexistent/file.log"));
        assert!(!file.read_success);
        assert!(file.error.is_some());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn test_get_os_version() {
        let version = get_os_version();
        // Should return something on all platforms
        assert!(!version.is_empty());
    }

    #[test]
    fn test_num_cpus() {
        let cpus = num_cpus();
        assert!(cpus >= 1);
    }

    #[test]
    fn test_export_to_dir() {
        let original = std::env::var("RUSTYCODE_PATH_ROOT").ok();
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("RUSTYCODE_PATH_ROOT", temp.path().as_os_str());

        let output_dir = temp.path().join("diagnostics-output");
        let result = DiagnosticsCollector::export_to_dir(output_dir.to_str().unwrap());

        assert!(result.is_ok());
        let exported_dir = result.unwrap();
        assert!(exported_dir.join("system.txt").exists());
        assert!(exported_dir.join("snapshot.json").exists());

        // Verify system.txt is readable
        let sys_text = fs::read_to_string(exported_dir.join("system.txt")).unwrap();
        assert!(sys_text.contains("App Version:"));

        // Verify snapshot.json is valid JSON
        let snapshot_json = fs::read_to_string(exported_dir.join("snapshot.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
        assert!(parsed["system_info"].is_object());

        match original {
            Some(v) => std::env::set_var("RUSTYCODE_PATH_ROOT", v),
            None => std::env::remove_var("RUSTYCODE_PATH_ROOT"),
        }
    }

    #[test]
    fn test_diagnostics_snapshot_roundtrip() {
        let snapshot = DiagnosticsCollector::collect();
        let json = snapshot.to_json_pretty().unwrap();
        let restored: DiagnosticsSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(snapshot.system_info.os, restored.system_info.os);
        assert_eq!(snapshot.files.len(), restored.files.len());
        assert_eq!(snapshot.total_size_bytes, restored.total_size_bytes);
    }
}
