//! Ramp-up strategies for load tests

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Strategy for ramping up concurrent users
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RampUpStrategy {
    /// Start all users immediately
    Immediate,

    /// Linear ramp-up over a duration
    Linear {
        /// Duration to ramp up all users
        duration: Duration,
    },

    /// Stepped ramp-up with discrete steps
    Stepped {
        /// Number of steps
        steps: usize,
        /// Duration of each step
        step_duration: Duration,
    },
}

impl RampUpStrategy {
    /// Calculate how many users should be active at a given time
    pub fn active_users(&self, total_users: usize, elapsed: Duration) -> usize {
        match self {
            Self::Immediate => total_users,

            Self::Linear { duration } => {
                if elapsed >= *duration {
                    total_users
                } else {
                    let ratio = elapsed.as_secs_f64() / duration.as_secs_f64();
                    #[allow(clippy::cast_precision_loss)]
                    let users = total_users as f64;
                    std::cmp::min((ratio * users) as usize, total_users)
                }
            }

            Self::Stepped {
                steps,
                step_duration,
            } => {
                let current_step = (elapsed.as_secs_f64() / step_duration.as_secs_f64()) as usize;
                let users_per_step = total_users.div_ceil(*steps); // Ceiling division
                std::cmp::min(users_per_step * (current_step + 1), total_users)
            }
        }
    }

    /// Get the total ramp-up duration
    pub const fn duration(&self, _total_users: usize) -> Option<Duration> {
        match self {
            Self::Immediate => None,
            Self::Linear { duration } => Some(*duration),
            Self::Stepped {
                steps,
                step_duration,
            } => {
                // Use nanosecond arithmetic to avoid Duration * u32 panic on overflow
                let total_ns = step_duration.as_nanos() as u64;
                let total_ns = total_ns.saturating_mul(*steps as u64);
                Some(Duration::from_nanos(total_ns))
            }
        }
    }

    /// Create a linear ramp-up strategy
    pub const fn linear(duration: Duration) -> Self {
        Self::Linear { duration }
    }

    /// Create a stepped ramp-up strategy
    pub const fn stepped(steps: usize, step_duration: Duration) -> Self {
        Self::Stepped {
            steps,
            step_duration,
        }
    }
}

impl Default for RampUpStrategy {
    fn default() -> Self {
        Self::Linear {
            duration: crate::defaults::DEFAULT_RAMP_UP_DURATION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_immediate_ramp_up() {
        let strategy = RampUpStrategy::Immediate;
        assert_eq!(strategy.active_users(100, Duration::ZERO), 100);
        assert_eq!(strategy.active_users(100, Duration::from_secs(1)), 100);
    }

    #[test]
    fn test_linear_ramp_up() {
        let strategy = RampUpStrategy::Linear {
            duration: Duration::from_secs(10),
        };

        // At 0 seconds, no users should be active
        assert_eq!(strategy.active_users(100, Duration::ZERO), 0);

        // At 5 seconds (halfway), 50% of users should be active
        assert_eq!(strategy.active_users(100, Duration::from_secs(5)), 50);

        // At 10 seconds (complete), all users should be active
        assert_eq!(strategy.active_users(100, Duration::from_secs(10)), 100);

        // After ramp-up, all users should be active
        assert_eq!(strategy.active_users(100, Duration::from_secs(20)), 100);
    }

    #[test]
    fn test_stepped_ramp_up() {
        let strategy = RampUpStrategy::Stepped {
            steps: 4,
            step_duration: Duration::from_secs(5),
        };

        // At 0 seconds, first step (25 users)
        assert_eq!(strategy.active_users(100, Duration::ZERO), 25);

        // At 5 seconds, second step (50 users)
        assert_eq!(strategy.active_users(100, Duration::from_secs(5)), 50);

        // At 10 seconds, third step (75 users)
        assert_eq!(strategy.active_users(100, Duration::from_secs(10)), 75);

        // At 15 seconds, fourth step (100 users)
        assert_eq!(strategy.active_users(100, Duration::from_secs(15)), 100);
    }

    #[test]
    fn test_ramp_up_duration() {
        let immediate = RampUpStrategy::Immediate;
        assert!(immediate.duration(100).is_none());

        let linear = RampUpStrategy::linear(Duration::from_secs(30));
        assert_eq!(linear.duration(100), Some(Duration::from_secs(30)));

        let stepped = RampUpStrategy::stepped(5, Duration::from_secs(10));
        assert_eq!(stepped.duration(100), Some(Duration::from_secs(50)));
    }

    #[test]
    fn test_default_ramp_up() {
        let strategy = RampUpStrategy::default();
        match strategy {
            RampUpStrategy::Linear { duration } => {
                assert_eq!(duration, crate::defaults::DEFAULT_RAMP_UP_DURATION);
            }
            _ => panic!("Expected default to be Linear"),
        }
    }

    #[test]
    fn test_linear_ramp_up_constructors() {
        let strategy = RampUpStrategy::linear(Duration::from_secs(20));
        assert_eq!(strategy.active_users(100, Duration::from_secs(10)), 50);
        assert_eq!(strategy.active_users(100, Duration::from_secs(20)), 100);
    }

    #[test]
    fn test_stepped_ramp_up_constructor() {
        let strategy = RampUpStrategy::stepped(5, Duration::from_secs(2));
        assert_eq!(strategy.active_users(100, Duration::from_secs(0)), 20);
    }

    #[test]
    fn test_ramp_up_duration_stepped() {
        let strategy = RampUpStrategy::stepped(3, Duration::from_secs(10));
        assert_eq!(strategy.duration(100), Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_ramp_up_duration_immediate() {
        let strategy = RampUpStrategy::Immediate;
        assert!(strategy.duration(50).is_none());
    }

    #[test]
    fn test_ramp_up_serialization_roundtrip() {
        for strategy in &[
            RampUpStrategy::Immediate,
            RampUpStrategy::linear(Duration::from_secs(15)),
            RampUpStrategy::stepped(4, Duration::from_secs(5)),
        ] {
            let json = serde_json::to_string(strategy).unwrap();
            let decoded: RampUpStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(strategy.duration(100), decoded.duration(100));
        }
    }

    #[test]
    fn test_linear_ramp_up_beyond_duration() {
        let strategy = RampUpStrategy::linear(Duration::from_secs(5));
        // Well past ramp-up should still return total
        assert_eq!(strategy.active_users(50, Duration::from_secs(100)), 50);
    }

    #[test]
    fn test_stepped_ramp_up_beyond_duration() {
        let strategy = RampUpStrategy::stepped(2, Duration::from_secs(5));
        // Past all steps, should be capped at total
        assert_eq!(strategy.active_users(10, Duration::from_secs(100)), 10);
    }

    #[test]
    fn test_stepped_duration_does_not_overflow() {
        // Previously: step_duration * steps as u32 could panic on overflow
        // Now uses saturating nanosecond arithmetic
        let strategy = RampUpStrategy::stepped(1_000_000, Duration::from_hours(1));
        let duration = strategy.duration(100);
        assert!(duration.is_some());
        // Should not panic — saturates instead
        assert!(duration.unwrap() > Duration::from_hours(1));
    }

    #[test]
    fn test_stepped_duration_normal() {
        let strategy = RampUpStrategy::stepped(5, Duration::from_secs(10));
        assert_eq!(strategy.duration(100), Some(Duration::from_secs(50)));
    }
}
