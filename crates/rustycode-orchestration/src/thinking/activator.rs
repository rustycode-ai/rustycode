//! Automatic deep-thinking activation based on complexity signals.
//!
//! This module provides a policy-driven approach to deciding when the
//! deep-thinking engine should be invoked. The orchestra (or any caller)
//! gathers environmental signals and asks the policy whether deep thinking
//! is warranted.

/// Approximation of the task's computational/complexity weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignalTier {
    /// Simple, well-understood tasks — no deep thinking needed.
    Light,
    /// Multi-step tasks that benefit from structured reasoning.
    #[default]
    Standard,
    /// Highly complex tasks that warrant full Graph-of-Thoughts treatment.
    Heavy,
}

/// Risk level of the operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignalRisk {
    /// Routine operation with low blast radius.
    #[default]
    Low,
    /// Operation that could affect correctness if wrong.
    Medium,
    /// Critical operation — mistakes are costly or irreversible.
    High,
}

/// Environmental signals collected by the caller to inform the activation decision.
///
/// Multiple boolean flags are intentional here: each represents an independent
/// activation factor that contributes to the scoring policy.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ActivationSignals {
    /// How complex is the current task?
    pub tier: SignalTier,
    /// How risky is the operation?
    pub risk: SignalRisk,
    /// Is the task strategic/architectural (vs. tactical/implementation)?
    pub is_strategic: bool,
    /// Has the task encountered ambiguity or confusion?
    pub is_ambiguous: bool,
    /// Is the budget under pressure (token cost sensitivity)?
    pub budget_pressure: bool,
    /// Did the caller explicitly request deep thinking?
    pub explicit_hint: bool,
}

impl ActivationSignals {
    /// Create signals with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the complexity tier.
    #[must_use]
    pub const fn with_tier(mut self, tier: SignalTier) -> Self {
        self.tier = tier;
        self
    }

    /// Set the risk level.
    #[must_use]
    pub const fn with_risk(mut self, risk: SignalRisk) -> Self {
        self.risk = risk;
        self
    }

    /// Mark as strategic/architectural.
    #[must_use]
    pub const fn strategic(mut self) -> Self {
        self.is_strategic = true;
        self
    }

    /// Mark as ambiguous.
    #[must_use]
    pub const fn ambiguous(mut self) -> Self {
        self.is_ambiguous = true;
        self
    }

    /// Enable budget pressure mode.
    #[must_use]
    pub const fn with_budget_pressure(mut self) -> Self {
        self.budget_pressure = true;
        self
    }

    /// Explicitly request deep thinking.
    #[must_use]
    pub const fn with_explicit_hint(mut self) -> Self {
        self.explicit_hint = true;
        self
    }
}

impl Default for ActivationSignals {
    fn default() -> Self {
        Self {
            tier: SignalTier::Standard,
            risk: SignalRisk::Low,
            is_strategic: false,
            is_ambiguous: false,
            budget_pressure: false,
            explicit_hint: false,
        }
    }
}

/// Policy for deciding whether to activate deep thinking.
///
/// Implementations can use any combination of signals to make the decision.
pub trait ThinkingActivationPolicy: Send + Sync {
    /// Returns `true` if deep thinking should be activated for the given signals.
    fn should_activate(&self, signals: &ActivationSignals) -> bool;

    /// Returns a human-readable reason for the activation decision.
    fn reason(&self, signals: &ActivationSignals) -> String;
}

/// Default activation policy using a scoring approach.
///
/// Each positive signal contributes points. If the total meets the threshold,
/// deep thinking is activated.
///
/// | Signal | Points |
/// |--------|--------|
/// | Heavy tier | 3 |
/// | Standard tier | 1 |
/// | High risk | 3 |
/// | Medium risk | 1 |
/// | Strategic | 2 |
/// | Ambiguous | 2 |
/// | Explicit hint | 10 |
/// | Budget pressure | -2 |
pub struct DefaultActivationPolicy {
    /// Minimum score to trigger deep thinking (default: 4).
    pub threshold: u32,
}

impl Default for DefaultActivationPolicy {
    fn default() -> Self {
        Self { threshold: 4 }
    }
}

impl DefaultActivationPolicy {
    /// Create with a custom activation threshold.
    #[must_use]
    pub const fn with_threshold(threshold: u32) -> Self {
        Self { threshold }
    }

    /// Compute the activation score for the given signals.
    #[must_use]
    #[allow(clippy::unused_self)]
    const fn score(&self, signals: &ActivationSignals) -> i32 {
        let mut score: i32 = 0;

        // Tier
        match signals.tier {
            SignalTier::Heavy => score += 3,
            SignalTier::Standard => score += 1,
            SignalTier::Light => {}
        }

        // Risk
        match signals.risk {
            SignalRisk::High => score += 3,
            SignalRisk::Medium => score += 1,
            SignalRisk::Low => {}
        }

        // Binary signals
        if signals.is_strategic {
            score += 2;
        }
        if signals.is_ambiguous {
            score += 2;
        }
        if signals.explicit_hint {
            score += 10;
        }

        // Budget pressure suppresses activation
        if signals.budget_pressure {
            score -= 2;
        }

        score
    }

    /// Describe which factors contributed to the score.
    #[must_use]
    #[allow(clippy::unused_self)]
    fn factors(&self, signals: &ActivationSignals) -> Vec<&'static str> {
        let mut factors = Vec::new();

        match signals.tier {
            SignalTier::Heavy => factors.push("heavy tier"),
            SignalTier::Standard => factors.push("standard tier"),
            SignalTier::Light => factors.push("light tier"),
        }

        match signals.risk {
            SignalRisk::High => factors.push("high risk"),
            SignalRisk::Medium => factors.push("medium risk"),
            SignalRisk::Low => {}
        }

        if signals.is_strategic {
            factors.push("strategic");
        }
        if signals.is_ambiguous {
            factors.push("ambiguous");
        }
        if signals.explicit_hint {
            factors.push("explicit hint");
        }
        if signals.budget_pressure {
            factors.push("budget pressure");
        }

        factors
    }
}

impl ThinkingActivationPolicy for DefaultActivationPolicy {
    fn should_activate(&self, signals: &ActivationSignals) -> bool {
        // Explicit hint always activates, regardless of budget
        if signals.explicit_hint {
            return true;
        }

        let score = self.score(signals);
        score >= i32::try_from(self.threshold).unwrap_or(i32::MAX)
    }

    fn reason(&self, signals: &ActivationSignals) -> String {
        let score = self.score(signals);
        let activated = self.should_activate(signals);
        let factors = self.factors(signals);

        if activated {
            format!(
                "Activated (score {} >= threshold {}): [{}]",
                score,
                self.threshold,
                factors.join(", ")
            )
        } else {
            format!(
                "Skipped (score {} < threshold {}): [{}]",
                score,
                self.threshold,
                factors.join(", ")
            )
        }
    }
}

/// A conservative policy that only activates for heavy+high-risk or explicit hints.
pub struct ConservativeActivationPolicy;

impl ThinkingActivationPolicy for ConservativeActivationPolicy {
    fn should_activate(&self, signals: &ActivationSignals) -> bool {
        if signals.explicit_hint {
            return true;
        }
        signals.tier == SignalTier::Heavy && signals.risk == SignalRisk::High
    }

    fn reason(&self, signals: &ActivationSignals) -> String {
        if self.should_activate(signals) {
            "Activated: conservative policy triggered".to_string()
        } else {
            "Skipped: conservative policy requires heavy+high-risk or explicit hint".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_light_low_risk() {
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Light)
            .with_risk(SignalRisk::Low);

        assert!(!policy.should_activate(&signals));
        assert!(policy.reason(&signals).contains("Skipped"));
    }

    #[test]
    fn test_default_policy_heavy_high_risk() {
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High);

        assert!(policy.should_activate(&signals));
        assert!(policy.reason(&signals).contains("Activated"));
    }

    #[test]
    fn test_default_policy_explicit_hint() {
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Light)
            .with_risk(SignalRisk::Low)
            .with_explicit_hint();

        // Explicit hint overrides everything
        assert!(policy.should_activate(&signals));
    }

    #[test]
    fn test_default_policy_budget_pressure_suppresses() {
        let policy = DefaultActivationPolicy::default();
        // Standard + Medium = 1 + 1 = 2, budget pressure = -2, total = 0
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Standard)
            .with_risk(SignalRisk::Medium)
            .with_budget_pressure();

        assert!(!policy.should_activate(&signals));
    }

    #[test]
    fn test_default_policy_strategic_ambiguous() {
        let policy = DefaultActivationPolicy::default();
        // Standard + Low + strategic + ambiguous = 1 + 0 + 2 + 2 = 5 >= 4
        let signals = ActivationSignals::new().strategic().ambiguous();

        assert!(policy.should_activate(&signals));
    }

    #[test]
    fn test_custom_threshold() {
        let policy = DefaultActivationPolicy::with_threshold(8);
        // Heavy + High = 3 + 3 = 6 < 8
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High);

        assert!(!policy.should_activate(&signals));
    }

    #[test]
    fn test_conservative_policy() {
        let policy = ConservativeActivationPolicy;

        // Heavy + High → activated
        let heavy_high = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High);
        assert!(policy.should_activate(&heavy_high));

        // Heavy + Medium → not activated
        let heavy_medium = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::Medium);
        assert!(!policy.should_activate(&heavy_medium));

        // Light + Explicit → activated
        let light_explicit = ActivationSignals::new()
            .with_tier(SignalTier::Light)
            .with_explicit_hint();
        assert!(policy.should_activate(&light_explicit));
    }

    #[test]
    fn test_explicit_hint_overrides_budget() {
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_explicit_hint()
            .with_budget_pressure();

        assert!(policy.should_activate(&signals));
    }

    #[test]
    fn test_score_calculation() {
        let policy = DefaultActivationPolicy::default();

        // All signals off, Light + Low = 0
        let none = ActivationSignals::new()
            .with_tier(SignalTier::Light)
            .with_risk(SignalRisk::Low);
        assert_eq!(policy.score(&none), 0);

        // Everything max except explicit and budget: Heavy + High + strategic + ambiguous = 3+3+2+2 = 10
        let all = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High)
            .strategic()
            .ambiguous();
        assert_eq!(policy.score(&all), 10);
    }

    #[test]
    fn test_default_signals_values() {
        let signals = ActivationSignals::default();
        assert_eq!(signals.tier, SignalTier::Standard);
        assert_eq!(signals.risk, SignalRisk::Low);
        assert!(!signals.is_strategic);
        assert!(!signals.is_ambiguous);
        assert!(!signals.budget_pressure);
        assert!(!signals.explicit_hint);
    }

    #[test]
    fn test_default_policy_exact_threshold() {
        // Standard + High = 1 + 3 = 4, exactly at threshold
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Standard)
            .with_risk(SignalRisk::High);
        assert!(policy.should_activate(&signals));
    }

    #[test]
    fn test_default_policy_below_threshold() {
        // Standard + Medium = 1 + 1 = 2 < 4
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Standard)
            .with_risk(SignalRisk::Medium);
        assert!(!policy.should_activate(&signals));
    }

    #[test]
    fn test_budget_pressure_negative_score() {
        // Light + Low + budget_pressure = 0 - 2 = -2
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Light)
            .with_risk(SignalRisk::Low)
            .with_budget_pressure();
        assert_eq!(policy.score(&signals), -2);
    }

    #[test]
    fn test_factors_list() {
        let policy = DefaultActivationPolicy::default();
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High)
            .strategic()
            .ambiguous()
            .with_budget_pressure();

        let factors = policy.factors(&signals);
        assert!(factors.contains(&"heavy tier"));
        assert!(factors.contains(&"high risk"));
        assert!(factors.contains(&"strategic"));
        assert!(factors.contains(&"ambiguous"));
        assert!(factors.contains(&"budget pressure"));
    }

    #[test]
    fn test_conservative_policy_reason_messages() {
        let policy = ConservativeActivationPolicy;

        let activated = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High);
        assert!(policy.reason(&activated).contains("Activated"));

        let skipped = ActivationSignals::new()
            .with_tier(SignalTier::Light)
            .with_risk(SignalRisk::Low);
        assert!(policy.reason(&skipped).contains("Skipped"));
    }

    #[test]
    fn test_signal_tier_default() {
        assert_eq!(SignalTier::default(), SignalTier::Standard);
    }

    #[test]
    fn test_signal_risk_default() {
        assert_eq!(SignalRisk::default(), SignalRisk::Low);
    }

    #[test]
    fn test_builder_pattern_chaining() {
        let signals = ActivationSignals::new()
            .with_tier(SignalTier::Heavy)
            .with_risk(SignalRisk::High)
            .strategic()
            .ambiguous()
            .with_budget_pressure()
            .with_explicit_hint();

        assert_eq!(signals.tier, SignalTier::Heavy);
        assert_eq!(signals.risk, SignalRisk::High);
        assert!(signals.is_strategic);
        assert!(signals.is_ambiguous);
        assert!(signals.budget_pressure);
        assert!(signals.explicit_hint);
    }
}
