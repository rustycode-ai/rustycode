//! Anthropic SSE stream parsing logic for streaming completions.
//!
//! Contains the `complete_stream_internal` method that builds streaming requests
//! and parses SSE events into structured `StreamEvent` results.

use crate::advisor::AdvisorTool;
use crate::provider::{CompletionRequest, ProviderError, StreamChunk};
use futures::{Stream, StreamExt};
use std::pin::Pin;

impl super::AnthropicProvider {
    /// Internal implementation of streaming completion without retry logic.
    pub async fn complete_stream_internal(
        &self,
        mut request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(_) => None, // Already provided in request.tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
            }
        };

        // Inject advisor tool if configured
        let advisor_tool = self
            .advisor_config
            .as_ref()
            .map(|c| c.advisor.to_anthropic_tool())
            .or_else(|| {
                std::env::var("RUSTYCODE_ADVISOR_MODEL")
                    .ok()
                    .map(|model| AdvisorTool::new(model).to_anthropic_tool())
            });

        if let Some(tool) = advisor_tool {
            if let Some(ref mut t_list) = request.tools {
                t_list.push(tool);
            } else {
                request.tools = Some(vec![tool]);
            }
        }

        // Execute via Route
        let stream = self
            .route
            .execute_stream(&request, tools.as_deref())
            .await
            .map_err(|e| ProviderError::Network(format!("route execution failed: {}", e)))?;

        // Map anyhow::Error to ProviderError
        let mapped_stream =
            stream.map(|res| res.map_err(|e| ProviderError::Network(e.to_string())));

        Ok(Box::pin(mapped_stream))
    }
}
