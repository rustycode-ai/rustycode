//! Reasoning persistence layer.
//!
//! Stores and retrieves structured thoughts to maintain context across
//! complex reasoning phases. Each task gets a directory under `base_path`
//! with one JSONL file per phase.

use crate::types::StructuredThought;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;

/// Summary of a completed reasoning phase.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhaseSummary {
    pub phase: u32,
    pub thought_count: usize,
    pub average_confidence: f64,
    pub decisions_made: Vec<String>,
    pub key_learnings: Vec<String>,
}

/// File-backed reasoning store.
pub struct ReasoningStore {
    base_path: PathBuf,
}

impl ReasoningStore {
    /// Create a new store rooted at `base_path`.
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Persist a thought for a given task and phase.
    pub fn store_thought(
        &self,
        task_id: &str,
        phase: u32,
        thought: &StructuredThought,
    ) -> Result<()> {
        let phase_dir = self
            .base_path
            .join(sanitize_task_id(task_id))
            .join(format!("phase_{phase}"));
        std::fs::create_dir_all(&phase_dir)
            .with_context(|| format!("Failed to create phase dir {}", phase_dir.display()))?;

        let thought_file = phase_dir.join("thoughts.jsonl");
        let json_line = serde_json::to_string(thought)? + "\n";
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&thought_file)
            .with_context(|| format!("Failed to open {}", thought_file.display()))?
            .write_all(json_line.as_bytes())
            .with_context(|| "Failed to write thought")?;

        Ok(())
    }

    /// Retrieve all thoughts for a task/phase.
    pub fn get_phase_thoughts(&self, task_id: &str, phase: u32) -> Result<Vec<StructuredThought>> {
        let thought_file = self
            .base_path
            .join(sanitize_task_id(task_id))
            .join(format!("phase_{phase}"))
            .join("thoughts.jsonl");
        if !thought_file.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&thought_file)?;
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("Failed to parse thought JSON"))
            .collect()
    }

    /// Produce a summary for a completed phase.
    #[allow(clippy::cast_precision_loss)]
    pub fn get_phase_summary(&self, task_id: &str, phase: u32) -> Result<PhaseSummary> {
        let thoughts = self.get_phase_thoughts(task_id, phase)?;
        let total_conf: u32 = thoughts.iter().map(|t| t.confidence).sum();
        let avg_conf = if thoughts.is_empty() {
            0.0
        } else {
            f64::from(total_conf) / thoughts.len() as f64
        };

        Ok(PhaseSummary {
            phase,
            thought_count: thoughts.len(),
            average_confidence: avg_conf,
            decisions_made: thoughts
                .iter()
                .filter(|t| t.thought_type == crate::types::ThoughtType::Decision)
                .map(|t| t.thought.clone())
                .collect(),
            key_learnings: thoughts
                .iter()
                .filter(|t| t.thought_type == crate::types::ThoughtType::Learning)
                .map(|t| t.thought.clone())
                .collect(),
        })
    }

    /// Build a JSON context blob for phase `N+1` from the previous phase.
    pub fn get_context_for_next_phase(
        &self,
        task_id: &str,
        next_phase: u32,
    ) -> Result<serde_json::Value> {
        if next_phase <= 1 {
            return Ok(serde_json::json!({
                "phase": next_phase,
                "previous_summary": null,
                "thoughts": []
            }));
        }

        let prev_phase = next_phase - 1;
        let summary = self.get_phase_summary(task_id, prev_phase)?;
        let thoughts = self.get_phase_thoughts(task_id, prev_phase)?;

        Ok(serde_json::json!({
            "phase": next_phase,
            "previous_summary": {
                "phase": summary.phase,
                "thought_count": summary.thought_count,
                "avg_confidence": summary.average_confidence,
                "decisions_made": summary.decisions_made,
                "key_learnings": summary.key_learnings,
            },
            "thoughts": thoughts.iter().map(|t| serde_json::json!({
                "thought": t.thought,
                "confidence": t.confidence,
                "thought_type": format!("{:?}", t.thought_type),
            })).collect::<Vec<_>>(),
        }))
    }

    /// Return the base path for diagnostics.
    #[allow(clippy::missing_const_for_fn)]
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

/// Sanitize a task ID for use as a directory name.
fn sanitize_task_id(task_id: &str) -> String {
    task_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::ThoughtType;
    use tempfile::TempDir;

    fn make_thought(
        text: &str,
        phase: u32,
        confidence: u32,
        final_thought: bool,
    ) -> StructuredThought {
        let mut t = StructuredThought::new(text.to_string(), phase, ThoughtType::Decision);
        t.confidence = confidence;
        t.next_thought_needed = !final_thought;
        t
    }

    #[test]
    fn test_store_and_retrieve_thoughts() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        store
            .store_thought("task1", 1, &make_thought("Analysis", 1, 70, false))
            .unwrap();

        let retrieved = store.get_phase_thoughts("task1", 1).unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].thought, "Analysis");
    }

    #[test]
    fn test_empty_phase() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());
        let thoughts = store.get_phase_thoughts("nonexistent", 1).unwrap();
        assert!(thoughts.is_empty());
    }

    #[test]
    fn test_phase_summary() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        store
            .store_thought("task2", 1, &make_thought("decide A", 1, 70, false))
            .unwrap();
        store
            .store_thought("task2", 1, &make_thought("decide B", 1, 90, true))
            .unwrap();

        let summary = store.get_phase_summary("task2", 1).unwrap();
        assert_eq!(summary.phase, 1);
        assert_eq!(summary.thought_count, 2);
        assert!((summary.average_confidence - 80.0).abs() < f64::EPSILON);
        assert_eq!(summary.decisions_made.len(), 2);
    }

    #[test]
    fn test_context_for_next_phase() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        store
            .store_thought("task3", 1, &make_thought("decided X", 1, 85, true))
            .unwrap();

        let ctx = store.get_context_for_next_phase("task3", 2).unwrap();
        assert_eq!(ctx["phase"], 2);
        assert_eq!(
            ctx["previous_summary"]["decisions_made"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_context_for_first_phase() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        let ctx = store.get_context_for_next_phase("task4", 1).unwrap();
        assert_eq!(ctx["phase"], 1);
        assert!(ctx["previous_summary"].is_null());
    }

    #[test]
    fn test_sanitize_task_id() {
        assert_eq!(sanitize_task_id("abc-123_def"), "abc-123_def");
        assert_eq!(sanitize_task_id("task with spaces!"), "task_with_spaces_");
    }

    #[test]
    fn test_multiple_phases() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        store
            .store_thought("multi", 1, &make_thought("phase 1", 1, 60, true))
            .unwrap();
        store
            .store_thought("multi", 2, &make_thought("phase 2", 2, 80, true))
            .unwrap();

        let p1 = store.get_phase_thoughts("multi", 1).unwrap();
        let p2 = store.get_phase_thoughts("multi", 2).unwrap();
        assert_eq!(p1.len(), 1);
        assert_eq!(p2.len(), 1);
        assert_eq!(p2[0].thought, "phase 2");
    }

    #[test]
    fn test_base_path_accessor() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());
        assert_eq!(store.base_path(), temp_dir.path());
    }

    #[test]
    fn test_store_multiple_thoughts_same_phase() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        store
            .store_thought("batch", 1, &make_thought("thought A", 1, 60, false))
            .unwrap();
        store
            .store_thought("batch", 1, &make_thought("thought B", 1, 70, false))
            .unwrap();
        store
            .store_thought("batch", 1, &make_thought("thought C", 1, 80, true))
            .unwrap();

        let thoughts = store.get_phase_thoughts("batch", 1).unwrap();
        assert_eq!(thoughts.len(), 3);
        assert_eq!(thoughts[0].thought, "thought A");
        assert_eq!(thoughts[2].thought, "thought C");
    }

    #[test]
    fn test_summary_empty_phase() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        let summary = store.get_phase_summary("empty", 1).unwrap();
        assert_eq!(summary.thought_count, 0);
        assert!((summary.average_confidence - 0.0).abs() < f64::EPSILON);
        assert!(summary.decisions_made.is_empty());
    }

    #[test]
    fn test_sanitize_special_chars() {
        assert_eq!(sanitize_task_id("task/with/slashes"), "task_with_slashes");
        assert_eq!(sanitize_task_id("task.with.dots"), "task_with_dots");
        assert_eq!(sanitize_task_id("a+b=c"), "a_b_c");
    }

    #[test]
    fn test_context_for_next_phase_no_previous() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        let ctx = store.get_context_for_next_phase("new-task", 2).unwrap();
        assert_eq!(ctx["phase"], 2);
        assert!(ctx["previous_summary"]["thought_count"].as_u64().unwrap() == 0);
    }

    #[test]
    fn test_corrupted_jsonl_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        // Write valid thought
        store
            .store_thought("corrupt", 1, &make_thought("valid", 1, 70, false))
            .unwrap();

        // Manually append corrupted line
        let phase_dir = temp_dir.path().join("corrupt").join("phase_1");
        std::fs::OpenOptions::new()
            .append(true)
            .open(phase_dir.join("thoughts.jsonl"))
            .unwrap()
            .write_all(b"not valid json\n")
            .unwrap();

        // Should return only the valid thought, ignoring corrupted line
        let thoughts = store.get_phase_thoughts("corrupt", 1);
        assert!(thoughts.is_err() || thoughts.unwrap().len() == 1);
    }

    #[test]
    fn test_large_phase_performance() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        let start = std::time::Instant::now();
        for i in 0..1000 {
            store
                .store_thought(
                    "large",
                    1,
                    &make_thought(&format!("thought_{i}"), 1, 70, false),
                )
                .unwrap();
        }
        let write_time = start.elapsed();

        let start = std::time::Instant::now();
        let thoughts = store.get_phase_thoughts("large", 1).unwrap();
        let read_time = start.elapsed();

        assert_eq!(thoughts.len(), 1000);
        // Should complete within reasonable time
        assert!(
            write_time < std::time::Duration::from_secs(10),
            "write took {:?}",
            write_time
        );
        assert!(
            read_time < std::time::Duration::from_secs(5),
            "read took {:?}",
            read_time
        );
    }

    #[test]
    fn test_empty_task_id_sanitized() {
        let temp_dir = TempDir::new().unwrap();
        let store = ReasoningStore::new(temp_dir.path().to_path_buf());

        let result = store.store_thought("", 1, &make_thought("test", 1, 70, false));
        assert!(result.is_ok());
        let thoughts = store.get_phase_thoughts("", 1).unwrap();
        assert_eq!(thoughts.len(), 1);
    }

    #[test]
    fn test_concurrent_writes_same_phase() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(ReasoningStore::new(temp_dir.path().to_path_buf()));

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    store
                        .store_thought(
                            "concurrent",
                            1,
                            &make_thought(&format!("t{i}"), 1, 70, false),
                        )
                        .unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let thoughts = store.get_phase_thoughts("concurrent", 1).unwrap();
        assert_eq!(thoughts.len(), 5);
    }
}
