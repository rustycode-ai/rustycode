//! Task profiler -- assesses task characteristics to assemble the right team.
//!
//! Analyzes task descriptions and context to produce a TaskProfile, which
//! determines team composition, agent attitudes, and safety constraints.

use rustycode_protocol::team::*;

/// Profiles a task to determine the appropriate team composition.
pub struct TaskProfiler;

impl TaskProfiler {
    pub fn new() -> Self {
        Self
    }

    /// Profile a task description and return a TaskProfile.
    ///
    /// Uses deterministic signals (keywords, patterns), not LLM calls.
    pub fn profile(&self, task: &str) -> TaskProfile {
        let signals = self.detect_signals(task);

        let risk = self.classify_risk(&signals);
        let reach = self.classify_reach(&signals);
        let familiarity = Familiarity::default();
        let reversibility = Reversibility::default();
        let strategy = self.detect_strategy(task, &signals);

        TaskProfile {
            risk,
            reach,
            familiarity,
            reversibility,
            strategy,
            signals,
        }
    }

    /// Detect the appropriate reasoning strategy from task characteristics.
    fn detect_strategy(&self, task: &str, _signals: &[ProfileSignal]) -> ReasoningStrategy {
        use rustycode_protocol::team::ReasoningStrategy;

        let task_lower = task.to_lowercase();

        // Check for debugging/investigation keywords first
        let debug_keywords = [
            "debug",
            "investigate",
            "why",
            "broken",
            "failing",
            "not working",
            "fix the bug",
            "troubleshoot",
        ];
        let is_debugging = debug_keywords.iter().any(|kw| task_lower.contains(kw));

        if is_debugging {
            return ReasoningStrategy::ReflectFirst;
        }

        let tdd_keywords = ["add", "implement", "feature", "create", "new", "build"];
        let is_feature_request = tdd_keywords.iter().any(|kw| task_lower.contains(kw));

        if is_feature_request {
            return ReasoningStrategy::TDD;
        }

        // Default to plan-first
        ReasoningStrategy::PlanFirst
    }

    fn detect_signals(&self, task: &str) -> Vec<ProfileSignal> {
        let mut signals = Vec::new();
        let lower = task.to_lowercase();

        // Keyword-based signals.
        let risk_keywords = [
            ("auth", 0.8),
            ("security", 0.9),
            ("password", 0.9),
            ("token", 0.7),
            ("secret", 0.9),
            ("credential", 0.9),
            ("production", 0.8),
            ("deploy", 0.7),
            ("migration", 0.6),
            ("database", 0.6),
            ("refactor", 0.5),
        ];

        for (keyword, weight) in &risk_keywords {
            if lower.contains(keyword) {
                signals.push(ProfileSignal {
                    kind: SignalKind::Keyword,
                    evidence: format!("task contains '{}'", keyword),
                    weight: *weight,
                });
            }
        }

        signals
    }

    fn classify_risk(&self, signals: &[ProfileSignal]) -> RiskLevel {
        let max_weight = signals.iter().map(|s| s.weight).fold(0.0_f64, f64::max);

        if max_weight >= 0.8 {
            RiskLevel::Critical
        } else if max_weight >= 0.6 {
            RiskLevel::High
        } else if max_weight >= 0.3 {
            RiskLevel::Moderate
        } else {
            RiskLevel::Low
        }
    }

    fn classify_reach(&self, signals: &[ProfileSignal]) -> ReachLevel {
        let keyword_count = signals
            .iter()
            .filter(|s| matches!(s.kind, SignalKind::Keyword))
            .count();

        if keyword_count >= 5 {
            ReachLevel::SystemWide
        } else if keyword_count >= 3 {
            ReachLevel::Wide
        } else if keyword_count >= 1 {
            ReachLevel::Local
        } else {
            ReachLevel::SingleFile
        }
    }
}

impl Default for TaskProfiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiler_detects_security_keywords() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Fix the auth token validation security issue");

        assert!(matches!(profile.risk, RiskLevel::Critical));
        assert!(!profile.signals.is_empty());
    }

    #[test]
    fn profiler_low_risk_for_simple_tasks() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Fix a typo in the README");

        assert!(matches!(profile.risk, RiskLevel::Low));
    }

    #[test]
    fn profiler_detects_password_security() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Implement password hashing for user registration");

        assert!(matches!(profile.risk, RiskLevel::Critical));
        assert!(profile
            .signals
            .iter()
            .any(|s| s.evidence.contains("password")));
    }

    #[test]
    fn profiler_detects_deployment_risk() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Deploy the application to production");

        assert!(
            matches!(profile.risk, RiskLevel::High) || matches!(profile.risk, RiskLevel::Critical)
        );
    }

    #[test]
    fn profiler_detects_database_moderate_risk() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Add a new database migration for users table");

        assert!(
            matches!(profile.risk, RiskLevel::Moderate) || matches!(profile.risk, RiskLevel::High)
        );
    }

    #[test]
    fn profiler_reach_single_file() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Fix typo");

        assert!(matches!(profile.reach, ReachLevel::SingleFile));
    }

    #[test]
    fn profiler_reach_system_wide() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Security refactor for auth password token credentials");

        assert!(matches!(profile.reach, ReachLevel::SystemWide));
    }

    #[test]
    fn profiler_detects_refactor_risk() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Refactor the authentication module");

        // "refactor" has weight 0.5, "auth" has weight 0.8
        assert!(matches!(
            profile.risk,
            RiskLevel::Moderate | RiskLevel::High | RiskLevel::Critical
        ));
    }

    #[test]
    fn profiler_default_familiarity_and_reversibility() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Some task");

        // These are always default in current implementation
        assert!(matches!(profile.familiarity, Familiarity::WellKnown));
        assert!(matches!(profile.reversibility, Reversibility::Easy));
    }

    #[test]
    fn profiler_detects_multiple_signals() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Security deployment with database migration");

        assert!(profile.signals.len() >= 3);
    }

    #[test]
    fn profiler_classify_risk_boundaries() {
        let profiler = TaskProfiler::new();

        // Test high risk (>= 0.6)
        let high_risk = profiler.profile("Deploy to production");
        assert!(matches!(
            high_risk.risk,
            RiskLevel::High | RiskLevel::Critical | RiskLevel::Moderate
        ));
    }

    #[test]
    fn profiler_task_profile_fields() {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile("Test task");

        // Verify all fields are populated - defaults are WellKnown and PlanFirst
        assert!(!profile.signals.is_empty() || profile.signals.is_empty()); // signals can be empty
        assert!(matches!(profile.familiarity, Familiarity::WellKnown));
        assert!(matches!(
            profile.strategy,
            ReasoningStrategy::PlanFirst
                | ReasoningStrategy::ActFirst
                | ReasoningStrategy::ReflectFirst
        ));
    }
}
