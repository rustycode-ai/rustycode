//! Unified skill management facade.
//!
//! Composes all Phase 1-5 subsystems into a single entry point:
//! [`SkillRegistry`], [`ActivationManager`], [`Discovery`],
//! [`QualityScorer`], [`CapabilityCurator`], [`LifecycleFsm`],
//! [`CapabilityGraph`], [`SkillWatcher`].

use crate::activation::ActivationManager;
use crate::activation::SkillRecommendation;
use crate::budget::BudgetEnforcer;
use crate::curator::CapabilityCurator;
use crate::discovery::Discovery;
use crate::graph::CapabilityGraph;
use crate::improvement::SkillImprover;
use crate::lifecycle::{LifecycleEvent, LifecycleFsm};
use crate::quality::QualityScorer;
use crate::registry::SkillRegistry;
use crate::scoping::resolve_allowed_tools;
use crate::types::{LifecycleState, SkillDefinition, SkillId, SkillQuality, SkillSource};
use crate::watcher::SkillWatcher;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

pub struct SkillManagerV2Builder {
    user_skills_dir: Option<PathBuf>,
    project_skills_dir: Option<PathBuf>,
    bundled_skills: Vec<SkillDefinition>,
    token_budget: u32,
    quality_storage_dir: Option<PathBuf>,
    min_curator_evidence: u32,
    graph_path: Option<PathBuf>,
}

impl Default for SkillManagerV2Builder {
    fn default() -> Self {
        Self {
            user_skills_dir: None,
            project_skills_dir: None,
            bundled_skills: Vec::new(),
            token_budget: 50_000,
            quality_storage_dir: None,
            min_curator_evidence: 3,
            graph_path: None,
        }
    }
}

impl SkillManagerV2Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn user_skills_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.user_skills_dir = Some(dir.into());
        self
    }

    pub fn project_skills_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.project_skills_dir = Some(dir.into());
        self
    }

    pub fn bundled_skill(mut self, skill: SkillDefinition) -> Self {
        self.bundled_skills.push(skill);
        self
    }

    pub const fn token_budget(mut self, budget: u32) -> Self {
        self.token_budget = budget;
        self
    }

    pub fn quality_storage_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.quality_storage_dir = Some(dir.into());
        self
    }

    pub const fn min_curator_evidence(mut self, min: u32) -> Self {
        self.min_curator_evidence = min;
        self
    }

    pub fn graph_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.graph_path = Some(path.into());
        self
    }

    pub fn build(self) -> Result<SkillManager> {
        let mut registry = SkillRegistry::new();

        for skill in &self.bundled_skills {
            registry.register_bundled(skill.clone());
        }

        if let Some(ref dir) = self.user_skills_dir {
            if let Err(e) = registry.load_from_dir(dir, SkillSource::User) {
                debug!("Failed to load user skills from {:?}: {}", dir, e);
            }
        }

        if let Some(ref dir) = self.project_skills_dir {
            if let Err(e) = registry.load_from_dir(dir, SkillSource::Project) {
                debug!("Failed to load project skills from {:?}: {}", dir, e);
            }
        }

        let total_skills = registry.all().len() + registry.conditional().len();
        info!(
            "SkillManagerV2 initialized with {} skills (budget: {} tokens)",
            total_skills, self.token_budget
        );

        let mut quality_scorer = QualityScorer::new();
        if let Some(ref dir) = self.quality_storage_dir {
            quality_scorer = quality_scorer.with_storage_dir(dir.clone());
            if let Err(e) = quality_scorer.load_from_dir(dir) {
                debug!("Failed to load quality records: {}", e);
            }
        }

        let mut graph = CapabilityGraph::new();
        if let Some(ref path) = self.graph_path {
            if path.exists() {
                match load_graph_from_file(&mut graph, path) {
                    Ok(()) => {}
                    Err(e) => debug!("Failed to load capability graph: {}", e),
                }
            }
        }

        Ok(SkillManager {
            registry,
            activation: ActivationManager::new(self.token_budget),
            discovery: Discovery::new(),
            quality_scorer,
            curator: CapabilityCurator::new().with_min_evidence(self.min_curator_evidence),
            lifecycles: HashMap::new(),
            graph,
            improver: SkillImprover::new(5),
            watcher: SkillWatcher::new(),
            user_skills_dir: self.user_skills_dir,
            project_skills_dir: self.project_skills_dir,
            graph_path: self.graph_path,
            session_skill_ids: Vec::new(),
            budget_enforcer: BudgetEnforcer::new(100_000),
        })
    }
}

pub struct SkillManager {
    registry: SkillRegistry,
    activation: ActivationManager,
    discovery: Discovery,
    quality_scorer: QualityScorer,
    curator: CapabilityCurator,
    lifecycles: HashMap<SkillId, LifecycleFsm>,
    graph: CapabilityGraph,
    improver: SkillImprover,
    watcher: SkillWatcher,
    user_skills_dir: Option<PathBuf>,
    project_skills_dir: Option<PathBuf>,
    graph_path: Option<PathBuf>,
    session_skill_ids: Vec<SkillId>,
    budget_enforcer: BudgetEnforcer,
}

impl SkillManager {
    pub fn builder() -> SkillManagerV2Builder {
        SkillManagerV2Builder::new()
    }

    fn remember_active_skill(&mut self, skill_id: &str) {
        if !self.session_skill_ids.iter().any(|id| id == skill_id) {
            self.session_skill_ids.push(skill_id.to_string());
        }
        self.ensure_lifecycle(skill_id, LifecycleState::Active);
    }

    fn active_skills(&self) -> Vec<&crate::activation::ActiveSkill> {
        self.activation.active_skills().into_iter().collect()
    }

    // -- Activation --

    /// Activate skills matching the given file paths (conditional activation).
    pub fn activate_for_paths(&mut self, file_paths: &[&str]) -> Vec<SkillId> {
        let activated = self
            .activation
            .activate_for_paths(&mut self.registry, file_paths);

        for id in &activated {
            self.remember_active_skill(id);
        }

        debug!(
            "Activated {} skills for paths {:?}",
            activated.len(),
            file_paths
        );
        activated
    }

    /// Evaluate and activate skills for a textual context (e.g., user message).
    pub fn activate_for_context(&mut self, context: &str) -> Vec<SkillRecommendation> {
        let recs = self
            .activation
            .evaluate_for_context(&self.registry, context);

        for rec in &recs {
            if let Err(e) = self.activation.activate(
                &mut self.registry,
                &rec.skill_id,
                format!("context:{}", rec.score),
            ) {
                tracing::warn!("Failed to activate skill {}: {e}", rec.skill_id);
            }
            self.remember_active_skill(&rec.skill_id);
        }

        recs
    }

    /// Manually activate a skill by name.
    pub fn activate_skill(&mut self, skill_id: &str, trigger: &str) -> Result<()> {
        let skill = self
            .registry
            .get(skill_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Skill not found: {skill_id}"))?;

        let token_budget = Self::estimate_tokens_for(&skill);

        self.budget_enforcer
            .add_skill(skill_id, u64::from(token_budget), 5);
        let evicted = self.budget_enforcer.enforce_budget();
        for evicted_id in &evicted {
            self.activation.deactivate(evicted_id);
            info!("Evicted skill '{}' due to budget pressure", evicted_id);
        }

        let activated = self
            .activation
            .activate(&mut self.registry, skill_id, trigger.to_string())
            .map_err(|e| anyhow::anyhow!("Failed to activate skill '{skill_id}': {e}"))?;

        if activated.is_some() {
            self.remember_active_skill(skill_id);
            Ok(())
        } else {
            self.budget_enforcer.deactivate_skill(skill_id);
            Err(anyhow::anyhow!(
                "Budget exceeded, cannot activate skill '{skill_id}'"
            ))
        }
    }

    pub fn deactivate_skill(&mut self, skill_id: &str) {
        self.activation.deactivate(skill_id);
        self.budget_enforcer.deactivate_skill(skill_id);
    }

    /// Get all currently active skill definitions.
    pub fn active_definitions(&self) -> Vec<&SkillDefinition> {
        self.active_skills()
            .into_iter()
            .filter_map(|a| self.registry.get(&a.skill_id))
            .collect()
    }

    pub fn active_skill_ids(&self) -> Vec<&SkillId> {
        self.active_skills()
            .into_iter()
            .map(|a| &a.skill_id)
            .collect()
    }

    pub fn is_active(&self, skill_id: &str) -> bool {
        self.activation.is_active(skill_id)
    }

    pub fn active_tool_scope(&self) -> Vec<String> {
        let mut combined = Vec::new();
        // Use active_definitions() which correctly reads from activation.active_skills()
        for def in self.active_definitions() {
            let tools = resolve_allowed_tools(def);
            combined.extend(tools);
        }
        combined.sort();
        combined.dedup();
        combined
    }

    #[allow(clippy::missing_const_for_fn, clippy::cast_precision_loss)]
    fn estimate_tokens_for(skill: &SkillDefinition) -> u32 {
        let base: u32 = match skill.effort {
            crate::types::SkillEffortLevel::Low => 500,
            crate::types::SkillEffortLevel::Medium => 1_000,
            crate::types::SkillEffortLevel::High => 2_000,
            crate::types::SkillEffortLevel::Max => 4_000,
        };
        base + (skill.description.len() as u32 / 4) + (skill.when_to_use.len() as u32 / 4)
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn budget_usage_ratio(&self) -> f64 {
        self.budget_enforcer.budget().utilization()
    }

    // -- Discovery --

    /// Discover skills by walking up from file paths toward root.
    pub fn discover_dynamic(&mut self, file_paths: &[&Path], cwd: &Path) -> Vec<PathBuf> {
        let dirs = self.discovery.discover_for_paths(file_paths, cwd);

        for dir in &dirs {
            if let Err(e) = self.registry.load_from_dir(dir, SkillSource::Dynamic) {
                debug!("Failed to load dynamic skills from {:?}: {}", dir, e);
            }
        }

        dirs
    }

    // -- Registry --

    pub fn all_definitions(&self) -> Vec<&SkillDefinition> {
        self.registry.all()
    }

    pub fn definition(&self, name: &str) -> Option<&SkillDefinition> {
        self.registry.get(name)
    }

    pub fn total_count(&self) -> usize {
        self.registry.all().len() + self.registry.conditional().len()
    }

    // -- Curator --

    /// Feed a tool execution to the curator for signal extraction.
    pub fn observe_tool_use(&mut self, tool_name: &str, tool_input: &str) {
        self.curator.observe_tool_execution(tool_name, tool_input);
    }

    pub fn unmatched_signals(&self) -> Vec<String> {
        self.curator.detect_unmatched_signals(&self.registry)
    }

    pub fn curator_suggestions(&self) -> Vec<String> {
        self.curator.suggest_for_unmatched()
    }

    // -- Quality --

    /// Record a quality assessment for a skill.
    pub fn record_quality(
        &mut self,
        skill_id: &str,
        telemetry: f64,
        graph: f64,
        intake: f64,
        routing: f64,
    ) -> SkillQuality {
        let quality = self
            .quality_scorer
            .compute_score(skill_id, telemetry, graph, intake, routing);
        self.quality_scorer.observe_score(skill_id, quality.clone());
        self.ensure_lifecycle(skill_id, LifecycleState::Active);
        quality
    }

    pub fn quality(&self, skill_id: &str) -> Option<&SkillQuality> {
        self.quality_scorer.quality(skill_id)
    }

    // -- Lifecycle --

    pub fn lifecycle_state(&self, skill_id: &str) -> Option<LifecycleState> {
        self.lifecycles
            .get(skill_id)
            .map(LifecycleFsm::current_state)
    }

    /// Manually promote a skill (e.g., user action).
    pub fn promote_skill(&mut self, skill_id: &str) -> Result<()> {
        let fsm = self
            .lifecycles
            .get_mut(skill_id)
            .ok_or_else(|| anyhow::anyhow!("Skill '{skill_id}' not tracked in lifecycle"))?;
        fsm.transition(LifecycleEvent::Promote)
            .map_err(|s| anyhow::anyhow!("Cannot promote from state {s:?}"))?;
        Ok(())
    }

    /// Archive a skill.
    pub fn archive_skill(&mut self, skill_id: &str) -> Result<()> {
        let fsm = self
            .lifecycles
            .get_mut(skill_id)
            .ok_or_else(|| anyhow::anyhow!("Skill '{skill_id}' not tracked in lifecycle"))?;
        fsm.transition(LifecycleEvent::Archive)
            .map_err(|s| anyhow::anyhow!("Cannot archive from state {s:?}"))?;
        Ok(())
    }

    // -- Graph --

    /// Find related skills by graph traversal.
    pub fn related_skills(&self, skill_id: &str, max_hops: usize) -> Vec<(String, f32)> {
        self.graph.walk_from(skill_id, max_hops)
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn centrality(&self, skill_id: &str) -> f32 {
        self.graph.centrality_score(skill_id)
    }

    // -- Improvement --

    /// Analyze corrections for a skill and return improvement proposals.
    pub fn on_user_turn(
        &mut self,
        skill: &SkillDefinition,
        corrections: &[String],
    ) -> Vec<crate::improvement::SkillUpdateProposal> {
        let result = self.improver.analyze_corrections(skill, corrections);
        result.proposals
    }

    // -- Session --

    /// Call at end of session to persist state and run lifecycle transitions.
    pub fn end_session(&mut self) {
        debug!(
            "Ending session, {} skills used",
            self.session_skill_ids.len()
        );

        if let Err(e) = self.quality_scorer.persist() {
            debug!("Failed to persist quality scores: {}", e);
        }

        if let Some(ref path) = self.graph_path {
            if let Err(e) = persist_graph_to_file(&self.graph, path) {
                debug!("Failed to save capability graph: {}", e);
            }
        }

        for skill_id in &self.session_skill_ids {
            if let Some(fsm) = self.lifecycles.get_mut(skill_id.as_str()) {
                if let Err(e) = fsm.transition(LifecycleEvent::Activate) {
                    tracing::warn!("Failed to activate lifecycle for skill {skill_id}: {:?}", e);
                }
            }
        }

        info!(
            "Session ended. Quality scores persisted, lifecycle transitions applied for {} skills.",
            self.session_skill_ids.len()
        );
    }

    // -- Watching --

    /// Register directories for file watching.
    pub fn watch_dirs(&mut self, dirs: &[PathBuf]) {
        for dir in dirs {
            self.watcher.watch_dir(dir.clone());
        }
    }

    /// Check for file changes and reload affected skills.
    pub fn poll_changes(&mut self) -> Vec<SkillId> {
        let changes = self.watcher.poll_changes();
        let mut reloaded = Vec::new();

        for event in &changes {
            debug!("Skill file {:?}: {:?}", event.path, event.kind);
            if let Some(parent) = event.path.parent() {
                if let Some(grandparent) = parent.parent() {
                    let source = self.infer_source(grandparent);
                    if let Err(e) = self.registry.load_from_dir(grandparent, source) {
                        debug!("Failed to reload skills from {:?}: {}", grandparent, e);
                    }
                }
            }
            if let Some(skill_id) = event.path.parent().and_then(|p| p.file_name()) {
                reloaded.push(SkillId::from(skill_id.to_string_lossy().as_ref()));
            }
        }

        reloaded
    }

    // -- Helpers --

    fn ensure_lifecycle(&mut self, skill_id: &str, initial: LifecycleState) {
        self.lifecycles
            .entry(skill_id.to_string())
            .or_insert_with(|| LifecycleFsm::new(initial));
    }

    fn infer_source(&self, dir: &Path) -> SkillSource {
        match &self.project_skills_dir {
            Some(proj_dir) if dir.starts_with(proj_dir) => SkillSource::Project,
            _ => match &self.user_skills_dir {
                Some(user_dir) if dir.starts_with(user_dir) => SkillSource::User,
                _ => SkillSource::Dynamic,
            },
        }
    }
}

// -- Graph file I/O helpers --

fn persist_graph_to_file(graph: &CapabilityGraph, path: &Path) -> Result<()> {
    let serialized = graph.serialize();
    let json = serde_json::to_string_pretty(&serialized)?;
    std::fs::write(path, &json)
        .map_err(|e| anyhow::anyhow!("Failed to write graph to {}: {e}", path.display()))?;
    Ok(())
}

fn load_graph_from_file(graph: &mut CapabilityGraph, path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read graph from {}: {e}", path.display()))?;
    let data: crate::graph::SerializedGraph = serde_json::from_str(&content)?;
    graph.deserialize(&data);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundled::SkillifyBuilder;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-v2-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn builder_creates_empty_manager() {
        let mgr = SkillManager::builder().build().unwrap();
        assert_eq!(mgr.total_count(), 0);
        assert!(mgr.active_definitions().is_empty());
    }

    #[test]
    fn builder_with_bundled_skills() {
        let skill = SkillifyBuilder::new("test-skill")
            .description("A test skill")
            .build();

        let mgr = SkillManager::builder()
            .bundled_skill(skill)
            .build()
            .unwrap();

        assert_eq!(mgr.total_count(), 1);
        assert!(mgr.definition("test-skill").is_some());
    }

    #[test]
    fn builder_with_skills_dir() {
        let dir = temp_dir();
        let skill_dir = dir.join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\neffort: low\n---\n\n# my-skill\n\nA skill from dir.\n",
        )
        .unwrap();

        let mgr = SkillManager::builder()
            .user_skills_dir(&dir)
            .build()
            .unwrap();

        assert!(mgr.definition("my-skill").is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn activate_for_context() {
        let skill = SkillifyBuilder::new("debugger")
            .description("Debug issues")
            .when_to_use("Use when debugging errors or fixing bugs")
            .build();

        let mut mgr = SkillManager::builder()
            .bundled_skill(skill)
            .build()
            .unwrap();

        let recs = mgr.activate_for_context("I need to debug this error");
        assert!(!recs.is_empty());
        assert!(mgr.is_active("debugger"));
    }

    #[test]
    fn activate_deactivate_cycle() {
        let skill = SkillifyBuilder::new("toggle-skill")
            .description("Toggle test")
            .build();

        let mut mgr = SkillManager::builder()
            .bundled_skill(skill)
            .build()
            .unwrap();

        mgr.activate_skill("toggle-skill", "manual").unwrap();
        assert!(mgr.is_active("toggle-skill"));

        mgr.deactivate_skill("toggle-skill");
        assert!(!mgr.is_active("toggle-skill"));
    }

    #[test]
    fn observe_tool_use_feeds_curator() {
        let mut mgr = SkillManager::builder().build().unwrap();

        for _ in 0..4 {
            mgr.observe_tool_use("Bash", "cargo test");
        }

        let unmatched = mgr.unmatched_signals();
        assert!(!unmatched.is_empty());
    }

    #[test]
    fn record_quality_updates_lifecycle() {
        let mut mgr = SkillManager::builder().build().unwrap();

        let quality = mgr.record_quality("test-skill", 0.9, 0.8, 0.7, 0.6);
        assert!(quality.weighted_total() > 0.0);

        let state = mgr.lifecycle_state("test-skill");
        assert!(state.is_some());
    }

    #[test]
    fn end_session_persists_state() {
        let dir = temp_dir();
        let quality_dir = dir.join("quality");
        let graph_path = dir.join("graph.json");
        fs::create_dir_all(&quality_dir).unwrap();

        let skill = SkillifyBuilder::new("session-skill")
            .description("Session test")
            .build();

        let mut mgr = SkillManager::builder()
            .bundled_skill(skill)
            .quality_storage_dir(&quality_dir)
            .graph_path(&graph_path)
            .build()
            .unwrap();

        mgr.activate_skill("session-skill", "test").unwrap();
        mgr.record_quality("session-skill", 0.8, 0.7, 0.6, 0.5);
        mgr.end_session();

        assert!(graph_path.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn related_skills_returns_empty_for_unknown() {
        let mgr = SkillManager::builder().build().unwrap();
        let related = mgr.related_skills("nonexistent", 3);
        assert!(related.is_empty());
    }

    #[test]
    fn poll_changes_empty_when_no_watched_dirs() {
        let mut mgr = SkillManager::builder().build().unwrap();
        let changes = mgr.poll_changes();
        assert!(changes.is_empty());
    }

    #[test]
    fn full_workflow() {
        let dir = temp_dir();
        let quality_dir = dir.join("quality");
        let graph_path = dir.join("graph.json");
        fs::create_dir_all(&quality_dir).unwrap();

        let skill_dir = dir.join("skills").join("coder");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: coder\neffort: medium\n---\n\n# coder\n\nWrites code.\n",
        )
        .unwrap();

        let mut mgr = SkillManager::builder()
            .user_skills_dir(dir.join("skills"))
            .quality_storage_dir(&quality_dir)
            .graph_path(&graph_path)
            .token_budget(25_000)
            .build()
            .unwrap();

        assert!(mgr.definition("coder").is_some());

        mgr.activate_skill("coder", "manual").unwrap();
        assert!(mgr.is_active("coder"));

        mgr.observe_tool_use("Bash", "cargo build");

        let q = mgr.record_quality("coder", 0.9, 0.8, 0.9, 0.7);
        assert!(q.weighted_total() > 0.5);

        mgr.end_session();
        assert!(graph_path.exists());

        fs::remove_dir_all(&dir).ok();
    }
}
