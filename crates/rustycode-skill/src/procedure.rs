use crate::types::{Pipeline, PipelineStage, ProcedureKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StageId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: StageId,
    pub name: String,
    pub description: String,
    pub dependencies: Vec<StageId>,
    pub required_tools: Vec<String>,
    pub parallel: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDag {
    pub tasks: HashMap<StageId, TaskNode>,
    pub entry_points: Vec<StageId>,
    pub terminal_tasks: Vec<StageId>,
}

impl TaskDag {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            entry_points: Vec::new(),
            terminal_tasks: Vec::new(),
        }
    }

    pub fn add_task(&mut self, task: TaskNode) {
        let is_entry = task.dependencies.is_empty();
        let id = task.id.clone();
        self.tasks.insert(id.clone(), task);
        if is_entry && !self.entry_points.contains(&id) {
            self.entry_points.push(id);
        }
        self.recompute_terminals();
    }

    pub fn get(&self, id: &StageId) -> Option<&TaskNode> {
        self.tasks.get(id)
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn topological_order(&self) -> Vec<StageId> {
        let mut in_deg: HashMap<StageId, usize> = HashMap::new();
        for id in self.tasks.keys() {
            in_deg.insert(id.clone(), 0);
        }
        for task in self.tasks.values() {
            in_deg.insert(task.id.clone(), task.dependencies.len());
        }

        let mut queue: Vec<StageId> = in_deg
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();
        while let Some(id) = queue.pop() {
            result.push(id.clone());
            for task in self.tasks.values() {
                if task.dependencies.contains(&id) {
                    if let Some(deg) = in_deg.get_mut(&task.id) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(task.id.clone());
                        }
                    }
                }
            }
        }

        result
    }

    fn recompute_terminals(&mut self) {
        let all_deps: Vec<StageId> = self
            .tasks
            .values()
            .flat_map(|t| t.dependencies.clone())
            .collect();

        self.terminal_tasks = self
            .tasks
            .keys()
            .filter(|id| !all_deps.iter().any(|dep| dep == *id))
            .cloned()
            .collect();
    }
}

impl Default for TaskDag {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a markdown body into a `ProcedureKind`.
/// Detects `### N. Stage Name` headings as pipeline stages.
/// Falls back to `Instruction` if no stages found.
pub fn parse_procedure(body: &str) -> ProcedureKind {
    let stages = parse_stages(body);
    if stages.is_empty() {
        ProcedureKind::Instruction
    } else {
        ProcedureKind::Pipeline(Pipeline { stages })
    }
}

fn parse_stages(body: &str) -> Vec<PipelineStage> {
    let mut stages = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    let mut found_any = false;

    for line in body.lines() {
        let trimmed = line.trim();

        if let Some(stage) = parse_stage_heading(trimmed) {
            if let Some(name) = current_name.take() {
                let description = current_lines.join("\n").trim().to_string();
                stages.push(build_stage(&name, &description));
            }
            current_name = Some(stage);
            current_lines.clear();
            found_any = true;
        } else if found_any && current_name.is_some() {
            current_lines.push(line.to_string());
        }
    }

    if let Some(name) = current_name {
        let description = current_lines.join("\n").trim().to_string();
        stages.push(build_stage(&name, &description));
    }

    stages
}

fn parse_stage_heading(line: &str) -> Option<String> {
    let heading = line.strip_prefix("###")?.trim();
    if heading.is_empty() {
        return None;
    }

    if let Some(dot_pos) = heading.find('.') {
        let prefix = &heading[..dot_pos];
        if prefix
            .chars()
            .all(|c| c.is_ascii_digit() || c == 'a' || c == 'b' || c == 'c')
        {
            let name = heading[dot_pos + 1..].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    Some(heading.to_string())
}

fn build_stage(name: &str, description: &str) -> PipelineStage {
    let parallel = description.lines().any(|l| {
        let lower = l.to_lowercase();
        lower.contains("parallel") || lower.contains("concurrent")
    });

    let required_tools = extract_tools(description);

    PipelineStage {
        name: name.to_string(),
        description: description.to_string(),
        required_tools,
        parallel,
    }
}

fn extract_tools(text: &str) -> Vec<String> {
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("tools:") || lower.starts_with("allowed tools:") {
            if let Some(rest) = line.split(':').nth(1) {
                return rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Convert a `Pipeline` into a `TaskDag` for execution by `TaskScheduler`.
pub fn pipeline_to_dag(pipeline: &Pipeline) -> TaskDag {
    let mut dag = TaskDag::new();
    let mut prev_id: Option<StageId> = None;
    let mut parallel_group: Vec<StageId> = Vec::new();

    for stage in &pipeline.stages {
        let id = StageId(stage.name.replace(' ', "-").to_lowercase());

        let dependencies = if stage.parallel {
            prev_id
                .as_ref()
                .map_or_else(Vec::new, |prev| vec![prev.clone()])
        } else if !parallel_group.is_empty() {
            let deps = parallel_group.clone();
            parallel_group.clear();
            deps
        } else if let Some(ref prev) = prev_id {
            vec![prev.clone()]
        } else {
            vec![]
        };

        if stage.parallel {
            parallel_group.push(id.clone());
        } else {
            if !parallel_group.is_empty() {
                parallel_group.clear();
            }
            prev_id = Some(id.clone());
        }

        dag.add_task(TaskNode {
            id: id.clone(),
            name: stage.name.clone(),
            description: stage.description.clone(),
            dependencies,
            required_tools: stage.required_tools.clone(),
            parallel: stage.parallel,
        });

        if !stage.parallel {
            prev_id = Some(id);
        }
    }

    dag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_instruction_fallback() {
        let body = "This is a simple prompt with no stages.\nJust instructions.";
        let proc = parse_procedure(body);
        assert_eq!(proc, ProcedureKind::Instruction);
    }

    #[test]
    fn parse_single_stage() {
        let body = "### 1. Write Tests\n\nWrite unit tests for the module.\n\n### 2. Implement\n\nWrite the code.";
        let proc = parse_procedure(body);
        match proc {
            ProcedureKind::Pipeline(p) => {
                assert_eq!(p.stages.len(), 2);
                assert_eq!(p.stages[0].name, "Write Tests");
                assert_eq!(p.stages[1].name, "Implement");
            }
            ProcedureKind::Instruction => panic!("Expected Pipeline"),
        }
    }

    #[test]
    fn parse_stage_heading_numbered() {
        assert_eq!(
            parse_stage_heading("### 1. Write Code"),
            Some("Write Code".to_string())
        );
        assert_eq!(
            parse_stage_heading("### 42. Final Review"),
            Some("Final Review".to_string())
        );
    }

    #[test]
    fn parse_stage_heading_lettered() {
        assert_eq!(
            parse_stage_heading("### 3a. Run Lint"),
            Some("Run Lint".to_string())
        );
        assert_eq!(
            parse_stage_heading("### 3b. Run Tests"),
            Some("Run Tests".to_string())
        );
    }

    #[test]
    fn parse_stage_heading_unnumbered() {
        assert_eq!(
            parse_stage_heading("### Setup Environment"),
            Some("Setup Environment".to_string())
        );
    }

    #[test]
    fn parse_stage_heading_invalid() {
        assert_eq!(parse_stage_heading("## Not a stage"), None);
        assert_eq!(parse_stage_heading("Just text"), None);
    }

    #[test]
    fn detect_parallel_stage() {
        let body =
            "### 1. Setup\n\n### 2a. Lint\nRun in parallel.\n\n### 2b. Test\nConcurrent execution.";
        let proc = parse_procedure(body);
        match proc {
            ProcedureKind::Pipeline(p) => {
                assert!(p.stages[1].parallel);
                assert!(p.stages[2].parallel);
            }
            ProcedureKind::Instruction => panic!("Expected Pipeline"),
        }
    }

    #[test]
    fn extract_tools_from_description() {
        let tools = extract_tools("Tools: Read, Write, Bash");
        assert_eq!(tools, vec!["Read", "Write", "Bash"]);
    }

    #[test]
    fn extract_tools_empty() {
        let tools = extract_tools("Just a regular description.");
        assert!(tools.is_empty());
    }

    #[test]
    fn pipeline_to_dag_linear() {
        let pipeline = Pipeline {
            stages: vec![
                PipelineStage {
                    name: "First".to_string(),
                    description: String::new(),
                    required_tools: vec![],
                    parallel: false,
                },
                PipelineStage {
                    name: "Second".to_string(),
                    description: String::new(),
                    required_tools: vec![],
                    parallel: false,
                },
            ],
        };

        let dag = pipeline_to_dag(&pipeline);
        assert_eq!(dag.task_count(), 2);
        assert_eq!(dag.entry_points.len(), 1);
        assert_eq!(dag.entry_points[0].0, "first");

        let second = dag.get(&StageId("second".to_string())).unwrap();
        assert_eq!(second.dependencies.len(), 1);
        assert_eq!(second.dependencies[0].0, "first");
    }

    #[test]
    fn pipeline_to_dag_single_stage() {
        let pipeline = Pipeline {
            stages: vec![PipelineStage {
                name: "Only".to_string(),
                description: String::new(),
                required_tools: vec![],
                parallel: false,
            }],
        };

        let dag = pipeline_to_dag(&pipeline);
        assert_eq!(dag.task_count(), 1);
        assert_eq!(dag.entry_points.len(), 1);
        assert_eq!(dag.terminal_tasks.len(), 1);
    }

    #[test]
    fn topological_order_linear() {
        let pipeline = Pipeline {
            stages: vec![
                PipelineStage {
                    name: "A".to_string(),
                    description: String::new(),
                    required_tools: vec![],
                    parallel: false,
                },
                PipelineStage {
                    name: "B".to_string(),
                    description: String::new(),
                    required_tools: vec![],
                    parallel: false,
                },
                PipelineStage {
                    name: "C".to_string(),
                    description: String::new(),
                    required_tools: vec![],
                    parallel: false,
                },
            ],
        };

        let dag = pipeline_to_dag(&pipeline);
        let order: Vec<String> = dag.topological_order().into_iter().map(|s| s.0).collect();
        let a_pos = order.iter().position(|x| x == "a").unwrap();
        let b_pos = order.iter().position(|x| x == "b").unwrap();
        let c_pos = order.iter().position(|x| x == "c").unwrap();
        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn dag_default_is_empty() {
        let dag = TaskDag::default();
        assert_eq!(dag.task_count(), 0);
    }

    #[test]
    fn dag_add_task_manual() {
        let mut dag = TaskDag::new();
        dag.add_task(TaskNode {
            id: StageId("a".to_string()),
            name: "Task A".to_string(),
            description: String::new(),
            dependencies: vec![],
            required_tools: vec![],
            parallel: false,
        });
        assert_eq!(dag.task_count(), 1);
        assert!(dag.entry_points.contains(&StageId("a".to_string())));
        assert!(dag.terminal_tasks.contains(&StageId("a".to_string())));
    }

    #[test]
    fn dag_terminals_correct() {
        let mut dag = TaskDag::new();
        dag.add_task(TaskNode {
            id: StageId("a".to_string()),
            name: "A".to_string(),
            description: String::new(),
            dependencies: vec![],
            required_tools: vec![],
            parallel: false,
        });
        dag.add_task(TaskNode {
            id: StageId("b".to_string()),
            name: "B".to_string(),
            description: String::new(),
            dependencies: vec![StageId("a".to_string())],
            required_tools: vec![],
            parallel: false,
        });
        assert_eq!(dag.terminal_tasks.len(), 1);
        assert_eq!(dag.terminal_tasks[0].0, "b");
    }
}
