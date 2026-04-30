pub mod instance;
pub mod predictor;
pub mod report;

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

/// SWE-bench prediction produced by the compatibility runner.
#[derive(Debug, Clone, Default)]
pub struct SweBenchPrediction {
    pub instance_id: String,
    pub model_patch: String,
}

/// Lightweight compatibility runner for the CLI swebench command.
pub struct SweBenchRunner {
    pub instances: PathBuf,
    pub output: PathBuf,
    pub budget: f64,
    pub parallel: usize,
    pub instance_ids: Option<Vec<String>>,
    pub format: String,
}

impl SweBenchRunner {
    pub fn new(
        instances: PathBuf,
        output: PathBuf,
        budget: f64,
        parallel: usize,
        instance_ids: Option<Vec<String>>,
    ) -> Self {
        Self {
            instances,
            output,
            budget,
            parallel,
            instance_ids,
            format: "json".to_string(),
        }
    }

    #[allow(clippy::unused_async)]
    pub async fn run_all(&mut self) -> Result<Vec<SweBenchPrediction>> {
        let predictions = Vec::new();

        if let Some(parent) = self.output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.output, "[]")?;

        Ok(predictions)
    }
}
