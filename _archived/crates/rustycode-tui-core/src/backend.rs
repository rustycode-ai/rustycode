//! Terminal backend abstraction
//!
//! This module provides a clean interface for terminal operations,
//! abstracting away the details of crossterm and ratatui.

use anyhow::Result;
use crossterm::{execute, terminal};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{Stdout, Write};

/// Terminal backend abstraction
pub struct TuiBackend {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TuiBackend {
    /// Create a new terminal backend
    pub fn new() -> Result<Self> {
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Get mutable reference to the terminal
    #[allow(clippy::missing_const_for_fn)]
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Setup terminal for TUI mode
    pub fn setup(&mut self) -> Result<()> {
        // Enter alternate screen first, then clear
        execute!(
            std::io::stdout(),
            terminal::EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableMouseCapture,
        )?;

        self.terminal.clear()?;
        terminal::enable_raw_mode()?;

        Ok(())
    }

    /// Cleanup terminal from TUI mode
    pub fn cleanup(&self) -> Result<()> {
        terminal::disable_raw_mode()?;
        execute!(
            std::io::stdout(),
            terminal::LeaveAlternateScreen,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
        )?;

        // Flush to ensure all commands are executed
        std::io::stdout().flush()?;

        Ok(())
    }

    /// Set terminal title
    pub fn set_title(&self, title: &str) -> Result<()> {
        // Sanitize: strip control characters to prevent terminal escape injection
        let sanitized: String = title.chars().filter(|c| !c.is_control()).collect();
        // OSC 0 sets the terminal window/tab title
        print!("\x1b]0;{}\x07", sanitized);
        std::io::stdout().flush()?;
        Ok(())
    }

    /// Clear terminal title
    pub fn clear_title(&self) -> Result<()> {
        print!("\x1b]0;\x07");
        std::io::stdout().flush()?;
        Ok(())
    }
}
