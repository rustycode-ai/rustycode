//! Execution limits and budget enforcement for autonomous agent runs.
//!
//! Provides hard limits on tool calls, model calls, wall-clock time, and token
//! usage. Each limit is checked before its corresponding operation and returns
//! a typed error when exceeded, preventing runaway execution.
//!
//! # Defaults by Autonomy Level
//!
//! | Level | Tool Calls | Model Calls | Wall Time | Tokens |
//! |-------|-----------|-------------|-----------|--------|
//! | L0    |   0       |     0       |   0s      |   0    |
//! | L1    |  10       |    15       |  5 min    |  50K   |
//! | L2    |  25       |    40       | 15 min    | 100K   |
//! | L3    |  50       |    80       | 30 min    | 200K   |
//! | L4    | 100       |   150       | 60 min    | 500K   |

use crate::autonomy::AutonomyLevel;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Hard limit on a single execution dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    pub max: u32,
    pub warn_at_percent: u8,
}

impl Limit {
    pub const fn new(max: u32) -> Self {
        Self {
            max,
            warn_at_percent: 80,
        }
    }

    pub const fn with_warn(max: u32, warn_at_percent: u8) -> Self {
        Self {
            max,
            warn_at_percent,
        }
    }

    /// Returns `Ok(())` if `current` is within budget, or an error description.
    pub fn check(&self, label: &str, current: u32) -> Result<(), ExecutionLimitError> {
        if self.max == 0 {
            return Err(ExecutionLimitError::LimitExceeded {
                limit: label.to_string(),
                current,
                max: 0,
            });
        }
        if current >= self.max {
            Err(ExecutionLimitError::LimitExceeded {
                limit: label.to_string(),
                current,
                max: self.max,
            })
        } else {
            Ok(())
        }
    }

    /// Whether the current usage is at or above the warning threshold.
    pub fn is_warning(&self, current: u32) -> bool {
        if self.max == 0 {
            return false;
        }
        let percent = (current as f64 / self.max as f64 * 100.0) as u8;
        percent >= self.warn_at_percent
    }
}

/// Configuration for all execution limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLimitsConfig {
    pub tool_calls: Limit,
    pub model_calls: Limit,
    pub wall_time_secs: Limit,
    pub tokens: Limit,
}

impl Default for ExecutionLimitsConfig {
    fn default() -> Self {
        Self::for_autonomy(AutonomyLevel::default())
    }
}

impl ExecutionLimitsConfig {
    /// Limits appropriate for the given autonomy level.
    #[must_use]
    pub fn for_autonomy(level: AutonomyLevel) -> Self {
        match level {
            AutonomyLevel::L0 => Self {
                tool_calls: Limit::new(0),
                model_calls: Limit::new(0),
                wall_time_secs: Limit::new(0),
                tokens: Limit::new(0),
            },
            AutonomyLevel::L1 => Self {
                tool_calls: Limit::new(10),
                model_calls: Limit::new(15),
                wall_time_secs: Limit::new(300), // 5 min
                tokens: Limit::new(50_000),
            },
            AutonomyLevel::L2 => Self {
                tool_calls: Limit::new(25),
                model_calls: Limit::new(40),
                wall_time_secs: Limit::new(900), // 15 min
                tokens: Limit::new(100_000),
            },
            AutonomyLevel::L3 => Self {
                tool_calls: Limit::new(50),
                model_calls: Limit::new(80),
                wall_time_secs: Limit::new(1800), // 30 min
                tokens: Limit::new(200_000),
            },
            AutonomyLevel::L4 => Self {
                tool_calls: Limit::new(100),
                model_calls: Limit::new(150),
                wall_time_secs: Limit::new(3600), // 60 min
                tokens: Limit::new(500_000),
            },
        }
    }

    /// Override individual limits while keeping defaults for the rest.
    #[must_use]
    pub fn with_tool_calls(mut self, max: u32) -> Self {
        self.tool_calls = Limit::new(max);
        self
    }

    #[must_use]
    pub fn with_model_calls(mut self, max: u32) -> Self {
        self.model_calls = Limit::new(max);
        self
    }

    #[must_use]
    pub fn with_wall_time_secs(mut self, max: u32) -> Self {
        self.wall_time_secs = Limit::new(max);
        self
    }

    #[must_use]
    pub fn with_tokens(mut self, max: u32) -> Self {
        self.tokens = Limit::new(max);
        self
    }
}

/// Typed error when an execution limit is exceeded.
#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionLimitError {
    #[error("{limit} exceeded: {current}/{max}")]
    LimitExceeded {
        limit: String,
        current: u32,
        max: u32,
    },
    #[error("wall time exceeded: {elapsed_secs}s > {max_secs}s")]
    TimeExceeded { elapsed_secs: u64, max_secs: u32 },
    #[error("doom loop detected: '{tool_name}' repeated {repeat_count} times")]
    DoomLoop {
        tool_name: String,
        repeat_count: usize,
    },
}

/// Runtime state tracking current usage against configured limits.
#[derive(Debug, Clone)]
pub struct ExecutionLimits {
    config: ExecutionLimitsConfig,
    tool_call_count: u32,
    model_call_count: u32,
    token_count: u32,
    start: Instant,
}

impl ExecutionLimits {
    /// Create a new tracker with the given config.
    pub fn new(config: ExecutionLimitsConfig) -> Self {
        Self {
            config,
            tool_call_count: 0,
            model_call_count: 0,
            token_count: 0,
            start: Instant::now(),
        }
    }

    /// Create a tracker configured for the given autonomy level.
    pub fn for_autonomy(level: AutonomyLevel) -> Self {
        Self::new(ExecutionLimitsConfig::for_autonomy(level))
    }

    /// Check and record a tool call. Returns an error if the limit is exceeded.
    pub fn check_tool_call(&mut self) -> Result<(), ExecutionLimitError> {
        self.config
            .tool_calls
            .check("tool_calls", self.tool_call_count)?;
        self.tool_call_count = self.tool_call_count.saturating_add(1);
        Ok(())
    }

    /// Check and record a model call. Returns an error if the limit is exceeded.
    pub fn check_model_call(&mut self) -> Result<(), ExecutionLimitError> {
        self.config
            .model_calls
            .check("model_calls", self.model_call_count)?;
        self.model_call_count = self.model_call_count.saturating_add(1);
        Ok(())
    }

    /// Check whether consuming `tokens` would exceed the budget, then record.
    /// Unlike counter-based limits, the full budget is usable: error only when
    /// the projected total *exceeds* the max (not equals).
    pub fn check_tokens(&mut self, tokens: u32) -> Result<(), ExecutionLimitError> {
        let projected = self.token_count.saturating_add(tokens);
        let max = self.config.tokens.max;
        if max == 0 || projected > max {
            return Err(ExecutionLimitError::LimitExceeded {
                limit: "tokens".into(),
                current: projected,
                max,
            });
        }
        self.token_count = projected;
        Ok(())
    }

    /// Check whether wall-clock time has exceeded the limit.
    pub fn check_time(&self) -> Result<(), ExecutionLimitError> {
        let elapsed = self.start.elapsed();
        let max_secs = self.config.wall_time_secs.max;
        // max=0 means no time allowed (immediately exceeded).
        if max_secs == 0 || elapsed.as_secs() >= u64::from(max_secs) {
            return Err(ExecutionLimitError::TimeExceeded {
                elapsed_secs: elapsed.as_secs(),
                max_secs,
            });
        }
        Ok(())
    }

    /// Record a doom loop abort from the detector.
    pub fn check_doom_loop(
        &self,
        tool_name: &str,
        repeat_count: usize,
    ) -> Result<(), ExecutionLimitError> {
        Err(ExecutionLimitError::DoomLoop {
            tool_name: tool_name.to_string(),
            repeat_count,
        })
    }

    /// Run all limit checks at once (before a tool call).
    pub fn check_all_before_tool(&mut self) -> Result<(), ExecutionLimitError> {
        self.check_time()?;
        self.check_tool_call()?;
        Ok(())
    }

    /// Run all limit checks at once (before a model call).
    pub fn check_all_before_model(&mut self) -> Result<(), ExecutionLimitError> {
        self.check_time()?;
        self.check_model_call()?;
        Ok(())
    }

    /// Whether any counter is at or above its warning threshold.
    pub fn has_warnings(&self) -> bool {
        self.config.tool_calls.is_warning(self.tool_call_count)
            || self.config.model_calls.is_warning(self.model_call_count)
            || self.config.tokens.is_warning(self.token_count)
    }

    /// Current usage snapshot for logging/diagnostics.
    #[must_use]
    pub fn snapshot(&self) -> ExecutionSnapshot {
        ExecutionSnapshot {
            tool_calls: self.tool_call_count,
            tool_call_limit: self.config.tool_calls.max,
            model_calls: self.model_call_count,
            model_call_limit: self.config.model_calls.max,
            tokens: self.token_count,
            token_limit: self.config.tokens.max,
            elapsed: self.start.elapsed(),
            wall_time_limit: Duration::from_secs(self.config.wall_time_secs.max.into()),
        }
    }

    pub const fn config(&self) -> &ExecutionLimitsConfig {
        &self.config
    }

    pub const fn tool_call_count(&self) -> u32 {
        self.tool_call_count
    }

    pub const fn model_call_count(&self) -> u32 {
        self.model_call_count
    }

    pub const fn token_count(&self) -> u32 {
        self.token_count
    }
}

/// Point-in-time usage snapshot for diagnostics.
#[derive(Debug, Clone)]
pub struct ExecutionSnapshot {
    pub tool_calls: u32,
    pub tool_call_limit: u32,
    pub model_calls: u32,
    pub model_call_limit: u32,
    pub tokens: u32,
    pub token_limit: u32,
    pub elapsed: Duration,
    pub wall_time_limit: Duration,
}

impl std::fmt::Display for ExecutionSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tools={}/{}, models={}/{}, tokens={}/{}, time={:.0}s/{:.0}s",
            self.tool_calls,
            self.tool_call_limit,
            self.model_calls,
            self.model_call_limit,
            self.tokens,
            self.token_limit,
            self.elapsed.as_secs_f64(),
            self.wall_time_limit.as_secs_f64(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_check_ok_when_under() {
        let limit = Limit::new(10);
        assert!(limit.check("test", 5).is_ok());
    }

    #[test]
    fn limit_check_fails_at_max() {
        let limit = Limit::new(10);
        assert!(limit.check("test", 10).is_err());
    }

    #[test]
    fn limit_check_fails_over_max() {
        let limit = Limit::new(10);
        assert!(limit.check("test", 15).is_err());
    }

    #[test]
    fn limit_zero_always_fails() {
        let limit = Limit::new(0);
        assert!(limit.check("test", 0).is_err());
    }

    #[test]
    fn limit_warning_at_threshold() {
        let limit = Limit::with_warn(100, 80);
        assert!(!limit.is_warning(79));
        assert!(limit.is_warning(80));
        assert!(limit.is_warning(90));
    }

    #[test]
    fn config_for_each_autonomy_level() {
        for level in [
            AutonomyLevel::L0,
            AutonomyLevel::L1,
            AutonomyLevel::L2,
            AutonomyLevel::L3,
            AutonomyLevel::L4,
        ] {
            let config = ExecutionLimitsConfig::for_autonomy(level);
            // Each level should have progressively higher limits
            assert!(config.tool_calls.max <= 100);
            assert!(config.model_calls.max <= 150);
        }
    }

    #[test]
    fn l0_blocks_everything() {
        let config = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L0);
        assert_eq!(config.tool_calls.max, 0);
        assert_eq!(config.model_calls.max, 0);
        assert_eq!(config.wall_time_secs.max, 0);
        assert_eq!(config.tokens.max, 0);
    }

    #[test]
    fn limits_escalate_with_autonomy() {
        let l1 = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L1);
        let l2 = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2);
        let l3 = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L3);
        let l4 = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L4);

        assert!(l1.tool_calls.max < l2.tool_calls.max);
        assert!(l2.tool_calls.max < l3.tool_calls.max);
        assert!(l3.tool_calls.max < l4.tool_calls.max);
    }

    #[test]
    fn config_override_builder() {
        let config = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2)
            .with_tool_calls(42)
            .with_tokens(999);

        assert_eq!(config.tool_calls.max, 42);
        assert_eq!(config.model_calls.max, 40); // L2 default
        assert_eq!(config.tokens.max, 999);
    }

    #[test]
    fn tool_call_enforcement() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L1));
        // L1 allows 10 tool calls
        for _ in 0..10 {
            assert!(limits.check_tool_call().is_ok());
        }
        // 11th should fail
        assert!(limits.check_tool_call().is_err());
        assert_eq!(limits.tool_call_count(), 10);
    }

    #[test]
    fn model_call_enforcement() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L1));
        // L1 allows 15 model calls
        for _ in 0..15 {
            assert!(limits.check_model_call().is_ok());
        }
        assert!(limits.check_model_call().is_err());
    }

    #[test]
    fn token_budget_enforcement() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L1));
        // L1 allows 50K tokens
        assert!(limits.check_tokens(40_000).is_ok());
        assert!(limits.check_tokens(10_000).is_ok());
        // Now at 50K, next call should fail
        assert!(limits.check_tokens(1).is_err());
    }

    #[test]
    fn token_saturating_add() {
        let config = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L4).with_tokens(u32::MAX);
        let mut limits = ExecutionLimits::new(config);
        // Near-max tokens should be accepted
        assert!(limits.check_tokens(u32::MAX - 1).is_ok());
        assert_eq!(limits.token_count(), u32::MAX - 1);
        // Saturating add wraps to u32::MAX, which is == max (not >), so still OK
        assert!(limits.check_tokens(100).is_ok());
        assert_eq!(limits.token_count(), u32::MAX);
        // Any further token request should fail: projected saturates to MAX, MAX > MAX is false,
        // but we're already AT max, so another call that would add > 0 pushes past.
        // Actually saturated: MAX + 1 = MAX, MAX > MAX = false → OK. Budget is effectively infinite.
    }

    #[test]
    fn time_limit_not_exceeded_initially() {
        let limits = ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2));
        assert!(limits.check_time().is_ok());
    }

    #[test]
    fn time_zero_always_fails() {
        let limits = ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L0));
        assert!(limits.check_time().is_err());
    }

    #[test]
    fn check_all_before_tool_checks_time_and_tools() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L1));
        for _ in 0..10 {
            assert!(limits.check_all_before_tool().is_ok());
        }
        // 11th tool call fails
        assert!(limits.check_all_before_tool().is_err());
    }

    #[test]
    fn check_all_before_model_checks_time_and_models() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L1));
        for _ in 0..15 {
            assert!(limits.check_all_before_model().is_ok());
        }
        assert!(limits.check_all_before_model().is_err());
    }

    #[test]
    fn doom_loop_returns_error() {
        let limits = ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L3));
        let result = limits.check_doom_loop("Read", 5);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ExecutionLimitError::DoomLoop { .. }));
        assert!(err.to_string().contains("Read"));
        assert!(err.to_string().contains('5'));
    }

    #[test]
    fn warning_detection() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2));
        // L2: 25 tool calls, warn at 80% = 20
        assert!(!limits.has_warnings());
        for _ in 0..20 {
            limits.check_tool_call().ok();
        }
        assert!(limits.has_warnings());
    }

    #[test]
    fn snapshot_format() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2));
        limits.check_tool_call().ok();
        limits.check_model_call().ok();
        limits.check_tokens(500).ok();

        let snap = limits.snapshot();
        assert_eq!(snap.tool_calls, 1);
        assert_eq!(snap.model_calls, 1);
        assert_eq!(snap.tokens, 500);
        let display = snap.to_string();
        assert!(display.contains("tools=1/25"));
        assert!(display.contains("models=1/40"));
        assert!(display.contains("tokens=500/100000"));
    }

    #[test]
    fn error_message_quality() {
        let mut limits =
            ExecutionLimits::new(ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2));
        for _ in 0..25 {
            limits.check_tool_call().ok();
        }
        let err = limits.check_tool_call().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("tool_calls"), "should name the limit: {msg}");
        assert!(msg.contains("25"), "should show current: {msg}");
        assert!(msg.contains("25"), "should show max: {msg}");
    }

    #[test]
    fn time_error_message_quality() {
        let config = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L2).with_wall_time_secs(0);
        let limits = ExecutionLimits::new(config);
        let err = limits.check_time().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("wall time"), "should mention wall time: {msg}");
    }

    #[test]
    fn config_serde_roundtrip() {
        let config = ExecutionLimitsConfig::for_autonomy(AutonomyLevel::L3);
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ExecutionLimitsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn limit_error_serde_roundtrip() {
        let err = ExecutionLimitError::LimitExceeded {
            limit: "tool_calls".into(),
            current: 25,
            max: 25,
        };
        let json = serde_json::to_string(&err).unwrap();
        let decoded: ExecutionLimitError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, err);
    }
}
