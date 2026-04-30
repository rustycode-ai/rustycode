//! Multi-step task support for sequential benchmark tasks.

use serde::{Deserialize, Serialize};

/// A single step in a multi-step task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    /// Step description / instruction.
    pub instruction: String,
    /// Minimum reward to proceed to the next step (0.0 to 1.0).
    #[serde(default = "default_min_reward")]
    pub min_reward: f64,
}

const fn default_min_reward() -> f64 {
    1.0
}

/// Parsed multi-step task configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiStepConfig {
    /// Ordered list of steps to execute.
    #[serde(default)]
    pub steps: Vec<TaskStep>,
}

impl MultiStepConfig {
    /// Parse multi-step config from a TOML string.
    pub fn from_toml(content: &str) -> anyhow::Result<Self> {
        let mut config: Self = toml::from_str(content)?;
        for step in &mut config.steps {
            step.min_reward = step.min_reward.clamp(0.0, 1.0);
        }
        Ok(config)
    }

    /// Total number of steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether there are no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Whether a step's reward allows proceeding to the next.
    pub fn can_proceed(&self, step_index: usize, reward: f64) -> bool {
        if step_index >= self.steps.len() {
            return false;
        }
        reward >= self.steps[step_index].min_reward
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_step() {
        let toml = r#"
[[steps]]
instruction = "Write a hello world program"
min_reward = 1.0
"#;
        let config = MultiStepConfig::from_toml(toml).unwrap();
        assert_eq!(config.len(), 1);
        assert_eq!(config.steps[0].instruction, "Write a hello world program");
        assert!((config.steps[0].min_reward - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_multi_step() {
        let toml = r#"
[[steps]]
instruction = "Install dependencies"
min_reward = 1.0

[[steps]]
instruction = "Implement the algorithm"
min_reward = 0.8

[[steps]]
instruction = "Write tests"
"#;
        let config = MultiStepConfig::from_toml(toml).unwrap();
        assert_eq!(config.len(), 3);
        assert!((config.steps[1].min_reward - 0.8).abs() < f64::EPSILON);
        assert!((config.steps[2].min_reward - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_empty_steps() {
        let toml = "";
        let config = MultiStepConfig::from_toml(toml).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn can_proceed_sufficient_reward() {
        let config = MultiStepConfig {
            steps: vec![
                TaskStep {
                    instruction: "step1".into(),
                    min_reward: 1.0,
                },
                TaskStep {
                    instruction: "step2".into(),
                    min_reward: 0.8,
                },
            ],
        };
        assert!(config.can_proceed(0, 1.0));
        assert!(!config.can_proceed(0, 0.9));
    }

    #[test]
    fn can_proceed_insufficient_reward() {
        let config = MultiStepConfig {
            steps: vec![TaskStep {
                instruction: "step1".into(),
                min_reward: 1.0,
            }],
        };
        assert!(!config.can_proceed(0, 0.5));
    }

    #[test]
    fn can_proceed_out_of_bounds() {
        let config = MultiStepConfig { steps: vec![] };
        assert!(!config.can_proceed(0, 1.0));
    }

    #[test]
    fn serde_roundtrip() {
        let config = MultiStepConfig {
            steps: vec![TaskStep {
                instruction: "do thing".into(),
                min_reward: 0.5,
            }],
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: MultiStepConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert!((back.steps[0].min_reward - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn can_proceed_partial_reward_threshold() {
        let config = MultiStepConfig {
            steps: vec![TaskStep {
                instruction: "step1".into(),
                min_reward: 0.8,
            }],
        };
        assert!(config.can_proceed(0, 0.8));
        assert!(config.can_proceed(0, 0.9));
        assert!(!config.can_proceed(0, 0.7));
    }
}
