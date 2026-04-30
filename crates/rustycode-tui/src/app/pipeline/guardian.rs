use super::artifact_registry::ArtifactRegistry;
use crate::app::pipeline::registry::{PipelineContext, PipelineRegistry};
use anyhow::Result;
use std::time::{Duration, Instant};

pub struct PipelineGuardian {
    last_check: Instant,
    check_interval: Duration,
    artifact_cleanup_threshold: usize,
}

impl Default for PipelineGuardian {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineGuardian {
    pub fn new() -> Self {
        Self {
            last_check: Instant::now(),
            check_interval: Duration::from_mins(1),
            artifact_cleanup_threshold: 1000,
        }
    }

    pub fn monitor(&mut self, _registry: &PipelineRegistry, _ctx: &PipelineContext) -> Result<()> {
        if self.last_check.elapsed() < self.check_interval {
            return Ok(());
        }

        tracing::info!("Guardian: Running system health check...");

        self.last_check = Instant::now();
        Ok(())
    }

    pub async fn monitor_artifacts(&mut self, artifact_registry: &ArtifactRegistry) -> Result<()> {
        if self.last_check.elapsed() < self.check_interval {
            return Ok(());
        }

        tracing::info!("Guardian: Running artifact health check...");

        let team_report_count = artifact_registry.count_by_type("team_report").await;
        if team_report_count > self.artifact_cleanup_threshold {
            tracing::warn!(
                "Artifact count exceeds threshold: {team_report_count}. Running cleanup...",
            );
            artifact_registry.cleanup().await?;
        }

        self.last_check = Instant::now();
        Ok(())
    }
}
