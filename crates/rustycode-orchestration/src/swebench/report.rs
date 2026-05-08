//! SWE-bench prediction result types.

/// A single prediction produced by the SWE-bench runner.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SweBenchPrediction {
    pub instance_id: String,
    pub model_patch: String,
}
