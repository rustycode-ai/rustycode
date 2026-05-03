//! Event loop coordinator
//!
//! This module provides the main event loop coordination with frame budgeting
//! and responsive input handling.

use anyhow::Result;
use crossterm::event;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Frame budget for 60 FPS
pub const FRAME_BUDGET_60FPS: Duration = Duration::from_millis(16);

/// Maximum acceptable input latency (50ms = 20 FPS minimum)
pub const MAX_INPUT_LATENCY: Duration = Duration::from_millis(50);

/// Event loop configuration
#[derive(Debug, Clone)]
pub struct EventLoopConfig {
    pub frame_budget: Duration,
    pub max_input_latency: Duration,
    pub enable_animations: bool,
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            frame_budget: FRAME_BUDGET_60FPS,
            max_input_latency: MAX_INPUT_LATENCY,
            enable_animations: true,
        }
    }
}

/// Event loop coordinator
pub struct EventLoop {
    config: EventLoopConfig,
    running: bool,
}

impl Default for EventLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLoop {
    /// Create a new event loop with default configuration
    pub fn new() -> Self {
        Self::with_config(EventLoopConfig::default())
    }

    /// Create a new event loop with custom configuration
    pub const fn with_config(config: EventLoopConfig) -> Self {
        Self {
            config,
            running: true,
        }
    }

    /// Check if the event loop is running
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Stop the event loop
    pub const fn stop(&mut self) {
        self.running = false;
    }

    /// Run the main event loop
    pub fn run<F>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        shutdown_rx: &mpsc::Receiver<()>,
        mut frame_callback: F,
    ) -> Result<()>
    where
        F: FnMut(&mut Terminal<CrosstermBackend<Stdout>>, Duration) -> Result<bool>,
    {
        let mut last_frame_time = Instant::now();

        while self.running {
            // Check for shutdown signal (Ctrl+C)
            if shutdown_rx.try_recv().is_ok() {
                self.running = false;
                break;
            }

            let frame_start = Instant::now();

            // Calculate delta time for animations (in milliseconds)
            let delta_ms = last_frame_time.elapsed().as_millis() as u64;
            last_frame_time = frame_start;

            // Call frame callback with terminal and delta time
            // The callback returns true if rendering should occur
            let should_render = frame_callback(terminal, Duration::from_millis(delta_ms))?;

            // Check frame budget
            let elapsed = frame_start.elapsed();

            if elapsed < self.config.frame_budget {
                // Render if requested
                if should_render {
                    terminal.draw(|f| {
                        // This would be implemented by the caller
                        // For now, just clear the frame
                        f.render_widget(ratatui::widgets::Clear, f.area());
                    })?;
                }

                // Handle input with remaining time
                let timeout = self
                    .config
                    .frame_budget
                    .saturating_sub(frame_start.elapsed());
                if event::poll(timeout)? {
                    // Input handling would be delegated to input handler
                    // For now, just consume the event
                    let _ = event::read()?;
                }
            } else {
                // Frame over budget, handle input with small timeout
                if event::poll(Duration::from_millis(1))? {
                    let _ = event::read()?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_loop_config_defaults() {
        let config = EventLoopConfig::default();
        assert_eq!(config.frame_budget, FRAME_BUDGET_60FPS);
        assert_eq!(config.max_input_latency, MAX_INPUT_LATENCY);
        assert!(config.enable_animations);
    }

    #[test]
    fn test_event_loop_creation() {
        let event_loop = EventLoop::new();
        assert!(event_loop.is_running());
    }

    #[test]
    fn test_event_loop_stop() {
        let mut event_loop = EventLoop::new();
        event_loop.stop();
        assert!(!event_loop.is_running());
    }

    #[test]
    fn test_custom_config() {
        let config = EventLoopConfig {
            frame_budget: Duration::from_millis(33), // ~30 FPS
            max_input_latency: Duration::from_millis(100),
            enable_animations: false,
        };
        let event_loop = EventLoop::with_config(config);
        assert!(event_loop.is_running());
    }

    #[test]
    fn test_stop_is_idempotent() {
        let mut event_loop = EventLoop::new();
        event_loop.stop();
        event_loop.stop(); // Second stop should be fine
        assert!(!event_loop.is_running());
    }
}
