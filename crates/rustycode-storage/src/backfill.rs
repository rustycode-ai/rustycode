use anyhow::{Result, Context, anyhow};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::fs;
use crate::state_runtime::StateRuntime;
use rustycode_protocol::SessionId;

/// Watermark for tracking backfill progress.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackfillWatermark {
    pub last_processed_timestamp: i64,
    pub last_processed_path: Option<String>,
    pub items_processed: u64,
    pub threads_updated: u64,
}

impl BackfillWatermark {
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join(".watermark");
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self {
                last_processed_timestamp: 0,
                last_processed_path: None,
                items_processed: 0,
                threads_updated: 0,
            })
        }
    }

    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = base_dir.join(".watermark");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

/// Worker that syncs historical rollout files into the SQLite index.
pub struct BackfillWorker {
    base_dir: PathBuf,
    state_runtime: StateRuntime,
    batch_size: usize,
}

impl BackfillWorker {
    pub fn new(base_dir: PathBuf, state_runtime: StateRuntime) -> Self {
        Self {
            base_dir,
            state_runtime,
            batch_size: 200,
        }
    }

    /// Run a backfill pass to index any new or updated rollout files.
    pub fn run(&mut self) -> Result<()> {
        let mut watermark = BackfillWatermark::load(&self.base_dir)?;
        
        // Scan for .jsonl files in the sessions directory
        let mut rollout_files = Vec::new();
        self.scan_dir(&self.base_dir, &mut rollout_files)?;
        
        // Sort by modification time
        rollout_files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());

        let mut processed = 0;
        for path in rollout_files {
            if processed >= self.batch_size {
                break;
            }

            let path_str = path.to_string_lossy().to_string();
            
            // Extract session_id from filename (expected: {session_id}.jsonl)
            let stem = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("Invalid filename: {:?}", path))?;
            
            let session_id = SessionId::parse(stem)
                .with_context(|| format!("Failed to parse session ID from filename: {stem}"))?;

            watermark.items_processed += 1;

            if self.state_runtime.get_thread(&session_id)?.is_none() {
                // Replay and index (simplified for now: just create row)
                // In a full implementation, we would use SessionReplayer::replay(path)
                self.state_runtime.create_thread(
                    &session_id,
                    &path_str,
                    "Recovered Task", // We don't have the task without replaying
                    "executing",
                    None,
                    None,
                )?;
                watermark.threads_updated += 1;
            }

            let mtime = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            watermark.last_processed_timestamp = mtime;
            watermark.last_processed_path = Some(path_str);
            processed += 1;
        }

        watermark.save(&self.base_dir)?;
        Ok(())
    }

    fn scan_dir(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir(&path, files)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
        Ok(())
    }
}
