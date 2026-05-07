//! Orchestra State Derivation
//!
//! Reconstructs the complete Orchestra project state by parsing files on disk.
//! This is the **source of truth** — `STATE.md` is just a cached snapshot.
//!
//! # State Hierarchy
//!
//! Orchestra organizes work in a three-level hierarchy:
//!
//! ```text
//! Milestones (M01, M02, ...)
//!   └─ Slices (S01, S02, ...)
//!       └─ Tasks (T01, T02, ...)
//! ```
//!
//! # Derivation Algorithm
//!
//! 1. Scan `.orchestra/milestones/*/` for ROADMAP.md files
//! 2. Parse each milestone to find incomplete slices
//! 3. For each incomplete slice, parse PLAN.md for tasks
//! 4. Return the **first incomplete task** at the deepest level
//!
//! # Finding the Active Task
//!
//! The algorithm prioritizes depth over breadth:
//! - Complete M01/S01/T01 before M01/S01/T02
//! - Complete M01/S01 before M01/S02
//! - Complete M01 before M02
//!
//! # Caching
//!
//! Derived state is cached to `STATE.md` for fast reads without
//! file parsing. Call `write_state_cache()` to update the cache.
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestration::state_derivation::StateDeriver;
//!
//! let deriver = StateDeriver::new(project_root);
//! let state = deriver.derive()?;
//!
//! match state.active_task {
//!     Some(task) => println!("Executing: {}", task.id),
//!     None => println!("All tasks complete!"),
//! }
//! ```
//!
//! # Error Handling
//!
//! State derivation is **fault-tolerant**:
//! - Missing milestones are skipped
//! - Malformed ROADMAP.md/PLAN.md files log warnings
//! - Empty projects return `Ok` with no active task
//!
//! Only critical errors (permissions, disk full) return `Err`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;
use walkdir::WalkDir;

use crate::phase::Phase;

/// Orchestra state derived from files on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestraState {
    /// Active milestone ID (e.g., "M01")
    pub active_milestone: Option<MilestoneRef>,
    /// Active slice ID (e.g., "S01")
    pub active_slice: Option<SliceRef>,
    /// Active task ID (e.g., "T01")
    pub active_task: Option<TaskRef>,
    /// All milestones
    pub milestones: Vec<MilestoneState>,
    /// Current phase (like orchestra-2)
    #[serde(default = "default_phase")]
    pub phase: Phase,
}

/// Default phase is executing
const fn default_phase() -> Phase {
    Phase::Execute
}

/// Milestone reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneRef {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
}

/// Slice reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceRef {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
}

/// Task reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRef {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub done: bool,
}

/// Milestone state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneState {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub complete: bool,
    pub slices: Vec<SliceState>,
}

/// Slice state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceState {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub done: bool,
    pub tasks: Vec<TaskState>,
}

/// Task state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub done: bool,
    pub has_plan: bool,
    pub has_summary: bool,
}

/// Roadmap structure (from ROADMAP.md)
#[derive(Debug, Clone)]
struct Roadmap {
    slices: Vec<RoadmapSlice>,
}

/// Slice in roadmap
#[derive(Debug, Clone)]
struct RoadmapSlice {
    id: String,
    #[allow(dead_code)]
    title: String,
    done: bool,
}

/// Plan structure (from PLAN.md)
#[derive(Debug, Clone)]
struct SlicePlan {
    tasks: Vec<PlanTask>,
}

/// Task in plan
#[derive(Debug, Clone)]
struct PlanTask {
    id: String,
    title: String,
    done: bool,
}

/// State deriver
pub struct StateDeriver {
    project_root: PathBuf,
}

impl StateDeriver {
    pub const fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Derive state from files on disk
    #[allow(clippy::too_many_lines)]
    pub fn derive_state(&self) -> Result<OrchestraState> {
        let milestones_dir = self.project_root.join(".orchestra/milestones");

        if !milestones_dir.exists() {
            return Ok(OrchestraState {
                active_milestone: None,
                active_slice: None,
                active_task: None,
                milestones: Vec::new(),
                phase: Phase::Research,
            });
        }

        let mut milestones = Vec::new();
        for entry in WalkDir::new(&milestones_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let path = entry.path();
            if path.is_dir() {
                if let Some(milestone_state) = self.load_milestone(path)? {
                    milestones.push(milestone_state);
                }
            }
        }

        milestones.sort_by(|a, b| a.id.cmp(&b.id));

        debug!("Total milestones loaded: {}", milestones.len());
        for m in &milestones {
            debug!(
                "  Milestone {} complete={} slices={}",
                m.id,
                m.complete,
                m.slices.len()
            );
        }

        // Single-pass: find active milestone → active slice → active task
        // Previous code re-searched milestones 4 times; now references chain directly.
        let active_milestone_state = milestones.iter().find(|m| !m.complete);

        let active_milestone = active_milestone_state.map(|m| {
            debug!("Found active milestone: {}", m.id);
            MilestoneRef {
                id: m.id.clone(),
                title: m.title.clone(),
                path: m.path.clone(),
            }
        });

        debug!(
            "Active milestone: {:?}",
            active_milestone.as_ref().map(|m| &m.id)
        );

        let active_slice_state = active_milestone_state.and_then(|m| {
            m.slices.iter().find(|s| !s.done)
        });

        let active_slice = active_slice_state.map(|s| {
            debug!(
                "Found active slice: {} with {} tasks",
                s.id,
                s.tasks.len()
            );
            SliceRef {
                id: s.id.clone(),
                title: s.title.clone(),
                path: s.path.clone(),
            }
        });

        let active_task = active_slice_state.and_then(|s| {
            s.tasks.iter().find(|t| !t.done).map(|t| {
                debug!("Found active task: {}", t.id);
                TaskRef {
                    id: t.id.clone(),
                    title: t.title.clone(),
                    path: t.path.clone(),
                    done: t.done,
                }
            })
        });

        let phase = if milestones.is_empty() {
            Phase::Research
        } else if let Some(ref task) = active_task {
            let plan_path = active_slice
                .as_ref()
                .map_or(PathBuf::new(), |slice| slice.path.join("PLAN.md"));

            if !plan_path.exists() {
                Phase::Plan
            } else if task.done {
                let all_done = active_slice_state
                    .is_some_and(|s| s.tasks.iter().all(|t| t.done));

                if all_done {
                    Phase::Complete
                } else {
                    Phase::Execute
                }
            } else {
                Phase::Execute
            }
        } else if active_slice.is_some() {
            let plan_path = active_slice
                .as_ref()
                .map_or(PathBuf::new(), |s| s.path.join("PLAN.md"));
            if plan_path.exists() {
                Phase::Complete
            } else {
                Phase::Plan
            }
        } else {
            Phase::Validate
        };

        Ok(OrchestraState {
            active_milestone,
            active_slice,
            active_task,
            milestones,
            phase,
        })
    }

    fn load_milestone(&self, milestone_path: &Path) -> Result<Option<MilestoneState>> {
        let id = milestone_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let roadmap_path = milestone_path.join("ROADMAP.md");
        let roadmap = if roadmap_path.exists() {
            self.parse_roadmap(&roadmap_path)?
        } else {
            Roadmap { slices: Vec::new() }
        };

        let complete = !roadmap.slices.is_empty() && roadmap.slices.iter().all(|s| s.done);
        debug!(
            "Milestone {} complete: {} ({} slices)",
            id,
            complete,
            roadmap.slices.len()
        );
        for s in &roadmap.slices {
            debug!("  Slice {} done={}", s.id, s.done);
        }

        let mut slices = Vec::new();
        for roadmap_slice in &roadmap.slices {
            let direct_path = milestone_path.join(&roadmap_slice.id);
            let slices_subdir_path = milestone_path.join("slices").join(&roadmap_slice.id);

            let slice_path = if slices_subdir_path.exists() {
                slices_subdir_path
            } else {
                direct_path
            };

            if let Some(slice_state) = self.load_slice(&slice_path, &roadmap_slice.id)? {
                slices.push(slice_state);
            }
        }

        if roadmap.slices.is_empty() {
            for entry in WalkDir::new(milestone_path)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                let path = entry.path();
                if path.is_dir() {
                    let slice_id = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    if slice_id == "slices" || slice_id == "tasks" || slice_id.starts_with('.') {
                        continue;
                    }

                    if let Some(slice_state) = self.load_slice(path, &slice_id)? {
                        slices.push(slice_state);
                    }
                }
            }
        }

        slices.sort_by(|a, b| a.id.cmp(&b.id));

        let title = format!("Milestone {id}");

        Ok(Some(MilestoneState {
            id,
            title,
            path: milestone_path.to_path_buf(),
            complete,
            slices,
        }))
    }

    fn load_slice(&self, slice_path: &Path, slice_id: &str) -> Result<Option<SliceState>> {
        let plan_path = slice_path.join("PLAN.md");
        debug!("Loading slice from: {:?}", slice_path);
        debug!("Looking for PLAN.md at: {:?}", plan_path);
        debug!("PLAN.md exists: {}", plan_path.exists());
        let plan = if plan_path.exists() {
            self.parse_plan(&plan_path)?
        } else {
            SlicePlan { tasks: Vec::new() }
        };

        let milestone_path = slice_path
            .parent()
            .and_then(|parent| {
                if parent.file_name().and_then(|n| n.to_str()) == Some("slices") {
                    parent.parent()
                } else {
                    Some(parent)
                }
            })
            .unwrap_or(slice_path);
        let roadmap_path = milestone_path.join("ROADMAP.md");
        let roadmap_done = if roadmap_path.exists() {
            let roadmap_content = std::fs::read_to_string(&roadmap_path)?;
            roadmap_content.contains(&format!("- [x] {slice_id}:"))
        } else {
            false
        };

        let all_tasks_done = !plan.tasks.is_empty() && plan.tasks.iter().all(|t| t.done);

        let done = roadmap_done || all_tasks_done;

        let mut tasks = Vec::new();
        for plan_task in &plan.tasks {
            let task_path = slice_path.join("tasks").join(&plan_task.id);
            let has_plan = task_path.join(format!("{}-PLAN.md", plan_task.id)).exists();
            let has_summary = task_path
                .join(format!("{}-SUMMARY.md", plan_task.id))
                .exists();

            debug!(
                "Loading task: {} has_plan={} has_summary={}",
                plan_task.id, has_plan, has_summary
            );
            tasks.push(TaskState {
                id: plan_task.id.clone(),
                title: plan_task.title.clone(),
                path: task_path,
                done: plan_task.done,
                has_plan,
                has_summary,
            });
        }

        debug!("Total tasks loaded: {}", tasks.len());

        if plan.tasks.is_empty() {
            let tasks_dir = slice_path.join("tasks");
            if tasks_dir.exists() {
                for entry in WalkDir::new(&tasks_dir)
                    .min_depth(1)
                    .max_depth(1)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                {
                    let path = entry.path();
                    if path.is_dir() {
                        let task_id = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let has_plan = path.join(format!("{task_id}-PLAN.md")).exists();
                        let has_summary = path.join(format!("{task_id}-SUMMARY.md")).exists();

                        tasks.push(TaskState {
                            id: task_id.clone(),
                            title: format!("Task {task_id}"),
                            path: path.to_path_buf(),
                            done: has_summary,
                            has_plan,
                            has_summary,
                        });
                    }
                }
            }
        }

        tasks.sort_by(|a, b| a.id.cmp(&b.id));

        let title = format!("Slice {slice_id}");

        Ok(Some(SliceState {
            id: slice_id.to_string(),
            title,
            path: slice_path.to_path_buf(),
            done,
            tasks,
        }))
    }

    #[allow(clippy::unused_self)]
    fn parse_roadmap(&self, path: &Path) -> Result<Roadmap> {
        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read roadmap: {}", path.display()))?;

        let mut slices = Vec::new();

        debug!("Parsing roadmap: {:?}", path);
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("- [") {
                let done = rest.starts_with('x');
                let rest = rest.trim_start();
                if let Some(slice_line) =
                    rest.strip_prefix("x] ").or_else(|| rest.strip_prefix("] "))
                {
                    let parts: Vec<&str> = slice_line.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let id = parts[0].trim().to_string();
                        let title = parts[1].trim().to_string();
                        debug!("  Found slice: {} ({}) done={}", id, title, done);
                        slices.push(RoadmapSlice { id, title, done });
                    }
                }
            }
        }

        debug!("  Total slices parsed: {}", slices.len());
        Ok(Roadmap { slices })
    }

    #[allow(clippy::unused_self)]
    fn parse_plan(&self, path: &Path) -> Result<SlicePlan> {
        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read plan: {}", path.display()))?;

        debug!("Parsing plan: {:?}", path);
        debug!("Plan content:\n{}", content);
        let mut tasks = Vec::new();

        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("- [") {
                let rest = rest.trim_start();
                let done = rest.starts_with('x');
                let rest = if done {
                    rest.strip_prefix('x').unwrap_or(rest)
                } else {
                    rest
                };

                if let Some(task_line) = rest.strip_prefix("] ") {
                    let task_line_cleaned = task_line.replace("**", "");
                    let parts: Vec<&str> = task_line_cleaned.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let id = parts[0].trim().to_string();
                        let title = parts[1].trim().to_string();
                        debug!("  Found task (format 1): {} ({}) done={}", id, title, done);
                        tasks.push(PlanTask { id, title, done });
                        continue;
                    }
                }

                if let Some(link_start) = rest.find('[') {
                    let after_bracket = &rest[link_start + 1..];
                    if let Some(link_end) = after_bracket.find(']') {
                        let id = &after_bracket[..link_end];
                        let rest_after_link = &after_bracket[link_end..];
                        if let Some(url_end) = rest_after_link.find("): ") {
                            let title = rest_after_link[url_end + 3..].trim().to_string();
                            debug!("  Found task (format 2): {} ({}) done={}", id, title, done);
                            tasks.push(PlanTask {
                                id: id.to_string(),
                                title,
                                done,
                            });
                        }
                    }
                }
            }
        }

        Ok(SlicePlan { tasks })
    }

    /// Write STATE.md cache
    #[allow(clippy::items_after_statements)]
    pub fn write_state_cache(&self, state: &OrchestraState) -> Result<()> {
        let state_path = self.project_root.join(".orchestra/STATE.md");

        let mut content = String::from("# Orchestra State\n\n");

        if let Some(ref am) = state.active_milestone {
            use std::fmt::Write;
            let _ = writeln!(content, "**Active Milestone:** {}: {}", am.id, am.title);
        }

        if let Some(ref aslice) = state.active_slice {
            use std::fmt::Write;
            let _ = writeln!(content, "**Active Slice:** {}: {}", aslice.id, aslice.title);
        }

        if let Some(ref atask) = state.active_task {
            use std::fmt::Write;
            let _ = writeln!(content, "**Active Task:** {}: {}", atask.id, atask.title);
            let _ = writeln!(content, "**Next Action:** Execute {}", atask.id);
        }

        use std::fmt::Write;
        let _ = writeln!(
            content,
            "\n**Last Updated:** {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        std::fs::write(&state_path, content).context(format!(
            "Failed to write STATE.md: {}",
            state_path.display()
        ))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_state_derivation() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let milestone_dir = project_root.join(".orchestra/milestones/M01");
        std::fs::create_dir_all(milestone_dir.join("slices/S01/tasks")).unwrap();

        let roadmap = r"# Milestone M01

## Slices
- [ ] S01: First slice
- [ ] S02: Second slice
";
        std::fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        let plan = r"# Slice S01

## Tasks
- [ ] T01: First task
- [ ] T02: Second task
";
        std::fs::write(milestone_dir.join("slices/S01/PLAN.md"), plan).unwrap();

        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().unwrap();

        println!("Milestones: {:?}", state.milestones.len());
        if !state.milestones.is_empty() {
            let m = &state.milestones[0];
            println!("  Milestone {}: {} slices", m.id, m.slices.len());
            if !m.slices.is_empty() {
                let s = &m.slices[0];
                println!(
                    "    Slice {}: {} tasks, done={}",
                    s.id,
                    s.tasks.len(),
                    s.done
                );
            }
        }

        assert_eq!(
            state.active_milestone.as_ref().map(|m| m.id.as_str()),
            Some("M01")
        );
        assert_eq!(
            state.active_slice.as_ref().map(|s| s.id.as_str()),
            Some("S01")
        );
        assert_eq!(
            state.active_task.as_ref().map(|t| t.id.as_str()),
            Some("T01")
        );
    }

    #[test]
    fn orchestra_state_serde_roundtrip() {
        let state = OrchestraState {
            active_milestone: Some(MilestoneRef {
                id: "M01".into(),
                title: "Milestone M01".into(),
                path: PathBuf::from("/tmp/M01"),
            }),
            active_slice: None,
            active_task: None,
            milestones: vec![],
            phase: Phase::Research,
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: OrchestraState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.active_milestone.as_ref().unwrap().id, "M01");
        assert_eq!(decoded.phase, Phase::Research);
    }

    #[test]
    fn orchestra_state_empty_serde() {
        let state = OrchestraState {
            active_milestone: None,
            active_slice: None,
            active_task: None,
            milestones: vec![],
            phase: Phase::Validate,
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: OrchestraState = serde_json::from_str(&json).unwrap();
        assert!(decoded.active_milestone.is_none());
        assert!(decoded.milestones.is_empty());
    }

    #[test]
    fn milestone_ref_serde() {
        let mr = MilestoneRef {
            id: "M02".into(),
            title: "Second".into(),
            path: PathBuf::from("/proj/.orchestra/milestones/M02"),
        };
        let json = serde_json::to_string(&mr).unwrap();
        let decoded: MilestoneRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "M02");
        assert_eq!(decoded.title, "Second");
    }

    #[test]
    fn slice_ref_serde() {
        let sr = SliceRef {
            id: "S01".into(),
            title: "Core".into(),
            path: PathBuf::from("/proj/.orchestra/milestones/M01/slices/S01"),
        };
        let json = serde_json::to_string(&sr).unwrap();
        let decoded: SliceRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "S01");
    }

    #[test]
    fn task_ref_serde() {
        let tr = TaskRef {
            id: "T01".into(),
            title: "Setup".into(),
            path: PathBuf::from("/proj/.orchestra/milestones/M01/slices/S01/tasks/T01"),
            done: false,
        };
        let json = serde_json::to_string(&tr).unwrap();
        let decoded: TaskRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "T01");
        assert!(!decoded.done);
    }

    #[test]
    fn task_ref_done_serde() {
        let tr = TaskRef {
            id: "T03".into(),
            title: "Done task".into(),
            path: PathBuf::from("/proj/T03"),
            done: true,
        };
        let json = serde_json::to_string(&tr).unwrap();
        let decoded: TaskRef = serde_json::from_str(&json).unwrap();
        assert!(decoded.done);
    }

    #[test]
    fn milestone_state_serde() {
        let ms = MilestoneState {
            id: "M01".into(),
            title: "Milestone M01".into(),
            path: PathBuf::from("/proj/M01"),
            complete: false,
            slices: vec![SliceState {
                id: "S01".into(),
                title: "Slice S01".into(),
                path: PathBuf::from("/proj/M01/S01"),
                done: false,
                tasks: vec![TaskState {
                    id: "T01".into(),
                    title: "Task T01".into(),
                    path: PathBuf::from("/proj/M01/S01/tasks/T01"),
                    done: false,
                    has_plan: true,
                    has_summary: false,
                }],
            }],
        };
        let json = serde_json::to_string(&ms).unwrap();
        let decoded: MilestoneState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "M01");
        assert!(!decoded.complete);
        assert_eq!(decoded.slices.len(), 1);
        assert_eq!(decoded.slices[0].tasks.len(), 1);
        assert!(decoded.slices[0].tasks[0].has_plan);
    }

    #[test]
    fn slice_state_serde() {
        let ss = SliceState {
            id: "S02".into(),
            title: "Slice S02".into(),
            path: PathBuf::from("/proj/S02"),
            done: true,
            tasks: vec![],
        };
        let json = serde_json::to_string(&ss).unwrap();
        let decoded: SliceState = serde_json::from_str(&json).unwrap();
        assert!(decoded.done);
        assert!(decoded.tasks.is_empty());
    }

    #[test]
    fn task_state_serde() {
        let ts = TaskState {
            id: "T02".into(),
            title: "Write tests".into(),
            path: PathBuf::from("/proj/T02"),
            done: true,
            has_plan: true,
            has_summary: true,
        };
        let json = serde_json::to_string(&ts).unwrap();
        let decoded: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "T02");
        assert!(decoded.has_plan);
        assert!(decoded.has_summary);
    }

    #[test]
    fn state_deriver_new() {
        let deriver = StateDeriver::new(PathBuf::from("/tmp/nonexistent"));
        let _ = deriver;
    }

    #[test]
    fn derive_state_empty_project() {
        let temp_dir = TempDir::new().unwrap();
        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        let state = deriver.derive_state().unwrap();
        assert!(state.active_milestone.is_none());
        assert!(state.active_slice.is_none());
        assert!(state.active_task.is_none());
        assert!(state.milestones.is_empty());
    }

    #[test]
    fn derive_state_no_milestones_dir() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra");
        std::fs::create_dir_all(&orchestra_dir).unwrap();
        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        let state = deriver.derive_state().unwrap();
        assert!(state.milestones.is_empty());
    }

    #[test]
    fn derive_state_complete_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let milestone_dir = temp_dir.path().join(".orchestra/milestones/M01");
        std::fs::create_dir_all(milestone_dir.join("slices/S01/tasks")).unwrap();

        let roadmap = "- [x] S01: Done slice\n";
        std::fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        let plan = "- [x] T01: Done task\n";
        std::fs::write(milestone_dir.join("slices/S01/PLAN.md"), plan).unwrap();

        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        let state = deriver.derive_state().unwrap();

        assert_eq!(state.milestones.len(), 1);
        assert!(state.milestones[0].complete);
        assert!(state.active_milestone.is_none());
    }

    #[test]
    fn write_state_cache_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".orchestra")).unwrap();

        let state = OrchestraState {
            active_milestone: Some(MilestoneRef {
                id: "M01".into(),
                title: "Milestone M01".into(),
                path: PathBuf::from("/tmp"),
            }),
            active_slice: None,
            active_task: None,
            milestones: vec![],
            phase: Phase::Execute,
        };

        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        deriver.write_state_cache(&state).unwrap();

        let content = std::fs::read_to_string(temp_dir.path().join(".orchestra/STATE.md")).unwrap();
        assert!(content.contains("Active Milestone:** M01:"));
        assert!(content.contains("Milestone M01"));
    }

    #[test]
    fn orchestra_state_default_phase_is_execute() {
        let json =
            r#"{"active_milestone":null,"active_slice":null,"active_task":null,"milestones":[]}"#;
        let decoded: OrchestraState = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.phase, Phase::Execute);
    }
}
