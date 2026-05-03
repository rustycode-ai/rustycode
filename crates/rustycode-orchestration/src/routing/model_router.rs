//! Model router -- maps task complexity to execution tiers.
//!
//! Uses [`ComplexityClassifier`] to score a [`TaskDescriptor`] and a
//! [`RoutingPolicy`] to determine the target [`ExecutionTier`].

use crate::routing::complexity_classifier::{ComplexityClassifier, TaskComplexity, TaskDescriptor};
use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};

// -- RoutingPolicy ------------------------------------------------------------

/// Configurable mapping from [`TaskComplexity`] to [`ExecutionTier`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicy {
    /// Tier for [`Simple`](TaskComplexity::Simple) tasks.
    pub simple_tier: ExecutionTier,
    /// Tier for [`Moderate`](TaskComplexity::Moderate) tasks.
    pub moderate_tier: ExecutionTier,
    /// Tier for [`Complex`](TaskComplexity::Complex) tasks.
    pub complex_tier: ExecutionTier,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            simple_tier: ExecutionTier::Musician,
            moderate_tier: ExecutionTier::Editor,
            complex_tier: ExecutionTier::Composer,
        }
    }
}

// -- ModelRouter --------------------------------------------------------------

/// Routes [`TaskDescriptor`] values to [`ExecutionTier`] values using
/// complexity classification and a configurable policy.
#[derive(Debug, Clone, Default)]
pub struct ModelRouter {
    classifier: ComplexityClassifier,
    policy: RoutingPolicy,
}

impl ModelRouter {
    /// Create a router with the given policy and default classifier.
    pub fn new(policy: RoutingPolicy) -> Self {
        Self {
            classifier: ComplexityClassifier::default(),
            policy,
        }
    }

    /// Create a router with both a custom policy and a custom classifier.
    pub const fn with_classifier(policy: RoutingPolicy, classifier: ComplexityClassifier) -> Self {
        Self { classifier, policy }
    }

    /// Classify a task and map it to the appropriate [`ExecutionTier`].
    pub fn route(&self, task: &TaskDescriptor) -> ExecutionTier {
        let complexity = self.classifier.classify(task);
        match complexity {
            TaskComplexity::Simple => self.policy.simple_tier,
            TaskComplexity::Moderate => self.policy.moderate_tier,
            TaskComplexity::Complex => self.policy.complex_tier,
        }
    }

    /// Delegate classification to the internal classifier.
    pub fn classify(&self, task: &TaskDescriptor) -> TaskComplexity {
        self.classifier.classify(task)
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_task(desc: &str, context: &str, steps: usize) -> TaskDescriptor {
        TaskDescriptor {
            description: desc.to_string(),
            context: context.to_string(),
            step_count: steps,
        }
    }

    #[test]
    fn test_routes_simple_to_musician() {
        let router = ModelRouter::default();
        let task = make_task("List files in /src", "", 1);
        let tier = router.route(&task);
        assert_eq!(tier, ExecutionTier::Musician);
    }

    #[test]
    fn test_routes_moderate_to_editor() {
        let router = ModelRouter::default();
        // "refactor" keyword + 5 steps => Moderate
        let task = make_task("Refactor authentication module", "", 5);
        let tier = router.route(&task);
        assert_eq!(tier, ExecutionTier::Editor);
    }

    #[test]
    fn test_routes_complex_to_composer() {
        let router = ModelRouter::default();
        // 20 steps + "design"/"algorithm" keywords => Complex
        let task = make_task("Design distributed consensus algorithm", "", 20);
        let tier = router.route(&task);
        assert_eq!(tier, ExecutionTier::Composer);
    }

    #[test]
    fn test_custom_policy() {
        // Route everything to Composer
        let policy = RoutingPolicy {
            simple_tier: ExecutionTier::Composer,
            moderate_tier: ExecutionTier::Composer,
            complex_tier: ExecutionTier::Composer,
        };
        let router = ModelRouter::new(policy);
        let task = make_task("List files", "", 1);
        let tier = router.route(&task);
        assert_eq!(tier, ExecutionTier::Composer);
    }

    #[test]
    fn test_custom_classifier_thresholds() {
        // Raise thresholds so that most tasks are Simple
        let classifier = ComplexityClassifier::new(100, 200);
        let router = ModelRouter::with_classifier(RoutingPolicy::default(), classifier);

        // This would be Complex with defaults, but with high thresholds it stays Simple
        let task = make_task("Design the architecture", "", 10);
        let tier = router.route(&task);
        assert_eq!(tier, ExecutionTier::Musician);
    }

    #[test]
    fn test_classify_delegates_to_classifier() {
        let router = ModelRouter::default();
        let task = make_task("List files", "", 1);
        let complexity = router.classify(&task);
        assert_eq!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn test_default_policy_serialization() {
        let policy = RoutingPolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: RoutingPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy.simple_tier, deserialized.simple_tier);
        assert_eq!(policy.moderate_tier, deserialized.moderate_tier);
        assert_eq!(policy.complex_tier, deserialized.complex_tier);
    }

    #[test]
    fn test_policy_maps_each_complexity_independently() {
        // Swap simple and complex tiers
        let policy = RoutingPolicy {
            simple_tier: ExecutionTier::Composer,
            moderate_tier: ExecutionTier::Editor,
            complex_tier: ExecutionTier::Musician,
        };
        let router = ModelRouter::new(policy);

        // Simple task -> Composer (not Musician)
        let simple_task = make_task("List files", "", 1);
        assert_eq!(router.route(&simple_task), ExecutionTier::Composer);

        // Complex task -> Musician (not Composer)
        let complex_task = make_task("Design distributed consensus algorithm", "", 20);
        assert_eq!(router.route(&complex_task), ExecutionTier::Musician);
    }
}
