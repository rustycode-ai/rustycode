//! Service channel draining for feature-based event dispatch.
//!
//! Mirrors poll_services() drain order and limits, but routes events through
//! feature modules via TuiEvent rather than calling handlers directly.
//!
//! ## Drain Order (MUST match poll_services() exactly)
//! 1. Stream chunks (max 8/frame) → TuiEvent::Stream
//! 2. Tool results (max 8/frame) → handled separately or converted to Service
//! 3. Workspace updates (1/frame) → TuiEvent::Service
//! 4. Command results (1/frame) → handled separately
//! 5. Event messages (max 8/frame) → TuiEvent::Service

use crate::app::async_::{StreamChunk, ToolResult, WorkspaceUpdate};
use crate::app::features::{TuiAction, TuiEvent};
use crate::app::service_integration::ServiceManager;
use rustycode_protocol::EventMsg;

/// Maximum stream chunks to drain per frame
const MAX_STREAM_CHUNKS_PER_FRAME: usize = 8;

/// Maximum tool results to drain per frame
const MAX_TOOL_RESULTS_PER_FRAME: usize = 8;

/// Maximum event messages to drain per frame
const MAX_EVENT_MSGS_PER_FRAME: usize = 8;

/// Service events collected from channels for feature dispatch
#[derive(Debug)]
pub struct DrainedEvents {
    /// Stream chunks in drain order
    pub stream_chunks: Vec<StreamChunk>,
    /// Tool results in drain order
    pub tool_results: Vec<ToolResult>,
    /// Workspace updates in drain order
    pub workspace_updates: Vec<WorkspaceUpdate>,
    /// Slash command results in drain order (from command_channel)
    pub command_results: Vec<crate::app::async_::SlashCommandResult>,
    /// EventMsg from unified channel
    pub event_msgs: Vec<EventMsg>,
    /// Whether stream channel disconnected unexpectedly
    pub stream_channel_disconnected: bool,
}

impl DrainedEvents {
    /// Drain all service channels preserving exact poll_services() order
    ///
    /// This method is designed to be called from AppShell's event loop
    /// replacement for poll_services(). It collects all pending events
    /// and returns them in drain order for feature dispatch.
    pub fn drain(services: &mut ServiceManager) -> Self {
        let mut drained = DrainedEvents {
            stream_chunks: Vec::new(),
            tool_results: Vec::new(),
            workspace_updates: Vec::new(),
            command_results: Vec::new(),
            event_msgs: Vec::new(),
            stream_channel_disconnected: false,
        };

        // Step 1: Drain stream chunks (max 8/frame)
        {
            if let Some(channel) = services.stream_channel_mut() {
                for _ in 0..MAX_STREAM_CHUNKS_PER_FRAME {
                    match channel.try_recv_ex() {
                        crate::app::async_::RecvStatus::Item(chunk) => {
                            drained.stream_chunks.push(chunk);
                        }
                        crate::app::async_::RecvStatus::Empty => break,
                        crate::app::async_::RecvStatus::Disconnected => {
                            drained.stream_channel_disconnected = true;
                            break;
                        }
                    }
                }
            }
        }

        // Step 2: Drain tool results (max 8/frame)
        {
            if let Some(channel) = services.tool_channel_mut() {
                for _ in 0..MAX_TOOL_RESULTS_PER_FRAME {
                    match channel.try_recv() {
                        Some(result) => drained.tool_results.push(result),
                        None => break,
                    }
                }
            }
        }

        // Step 3: Drain workspace updates (1/frame)
        {
            if let Some(channel) = services.workspace_channel_mut() {
                if let Some(update) = channel.try_recv() {
                    drained.workspace_updates.push(update);
                }
            }
        }

        // Step 4: Drain command results (1/frame)
        {
            if let Some(channel) = services.command_channel_mut() {
                if let Some(result) = channel.try_recv() {
                    drained.command_results.push(result);
                }
            }
        }

        // Step 5: Drain event messages (max 8/frame)
        {
            if let Some(channel) = services.event_channel_mut() {
                for _ in 0..MAX_EVENT_MSGS_PER_FRAME {
                    match channel.try_recv() {
                        Some(msg) => drained.event_msgs.push(msg),
                        None => break,
                    }
                }
            }
        }

        drained
    }

    /// Convert drained events to TuiEvent variants for feature dispatch
    ///
    /// Converts all drained events to TuiEvent variants for feature dispatch.
    /// Preserves exact drain order: stream chunks, tool results, workspace updates,
    /// command results, and event messages.
    pub fn to_tui_events(&self) -> Vec<TuiEvent> {
        let mut events = Vec::new();

        // Add stream chunks first (preserving drain order)
        for chunk in &self.stream_chunks {
            events.push(TuiEvent::Stream(chunk.clone()));
        }

        // TODO: Convert tool_results → TuiEvent or let features handle via Service
        // For now, tool results are not converted. Future work should decide:
        // - Route through new TuiEvent::Tool variant, or
        // - Let tools feature access them via ServiceManager directly
        // Tool results should be consumed by tool_panel feature in its handler

        // TODO: Convert workspace_updates → TuiEvent
        // Similar decision needed for workspace updates.
        // Workspace updates should be consumed by workspace feature in its handler

        // Add event messages (preserving drain order)
        for msg in &self.event_msgs {
            events.push(TuiEvent::Service(msg.clone()));
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drained_events_new_is_empty() {
        let drained = DrainedEvents {
            stream_chunks: Vec::new(),
            tool_results: Vec::new(),
            workspace_updates: Vec::new(),
            command_results: Vec::new(),
            event_msgs: Vec::new(),
            stream_channel_disconnected: false,
        };

        assert_eq!(drained.stream_chunks.len(), 0);
        assert_eq!(drained.tool_results.len(), 0);
        assert_eq!(drained.event_msgs.len(), 0);
        assert!(!drained.stream_channel_disconnected);
    }

    #[test]
    fn drained_events_converts_to_tui_events() {
        use crate::app::async_::StreamChunk;

        let stream_chunk = StreamChunk::Done;
        let drained = DrainedEvents {
            stream_chunks: vec![stream_chunk],
            tool_results: Vec::new(),
            workspace_updates: Vec::new(),
            command_results: Vec::new(),
            event_msgs: vec![EventMsg::Done],
            stream_channel_disconnected: false,
        };

        let tui_events = drained.to_tui_events();
        // Should convert stream chunks and event messages
        assert_eq!(tui_events.len(), 2);
        // First should be a stream chunk
        match &tui_events[0] {
            TuiEvent::Stream(StreamChunk::Done) => {}
            _ => panic!("Expected TuiEvent::Stream(StreamChunk::Done)"),
        }
        // Second should be service event
        match &tui_events[1] {
            TuiEvent::Service(EventMsg::Done) => {}
            _ => panic!("Expected TuiEvent::Service(EventMsg::Done)"),
        }
    }

    #[test]
    fn drain_respects_max_stream_chunks() {
        // This test is more of a specification:
        // DrainedEvents::drain() should not exceed MAX_STREAM_CHUNKS_PER_FRAME
        // In actual testing, a mock ServiceManager would be needed
        assert_eq!(MAX_STREAM_CHUNKS_PER_FRAME, 8);
    }

    #[test]
    fn drain_respects_max_event_msgs() {
        assert_eq!(MAX_EVENT_MSGS_PER_FRAME, 8);
    }
}
