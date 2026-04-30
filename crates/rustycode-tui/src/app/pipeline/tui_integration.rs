use crate::app::event_loop::TUI;
use anyhow::Result;

impl TUI {
    pub async fn tick_pipeline(&mut self) -> Result<()> {
        self.pipeline.run_available(&mut self.pipeline_ctx).await?;
        Ok(())
    }
}
