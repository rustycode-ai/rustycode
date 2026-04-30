//! High-level phase model used by orchestration and state derivation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Phase {
    Research,
    Plan,
    #[default]
    Execute,
    Validate,
    Complete,
}
