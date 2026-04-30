//! Terminal management utilities

use crate::backend::TuiBackend;
use anyhow::Result;
use crossterm::{execute, terminal};
use std::io::{stdout, Write};
use std::path::PathBuf;

/// Terminal manager for setup and cleanup
pub struct TerminalManager {
    backend: TuiBackend,
    cwd: PathBuf,
}

impl TerminalManager {
    /// Create a new terminal manager
    pub fn new(cwd: PathBuf) -> Result<Self> {
        let backend = TuiBackend::new()?;

        Ok(Self { backend, cwd })
    }

    /// Setup terminal for TUI operation
    pub fn setup(&mut self) -> Result<()> {
        self.backend.setup()?;

        // Set terminal title to project name
        if let Some(dir_name) = self.cwd.file_name().and_then(|n| n.to_str()) {
            self.backend
                .set_title(&format!("rustycode: {}", dir_name))?;
        }

        Ok(())
    }

    /// Cleanup terminal after TUI operation
    pub fn cleanup(&self) -> Result<()> {
        self.backend.cleanup()?;
        self.backend.clear_title()?;
        Ok(())
    }

    /// Get backend reference
    #[allow(clippy::missing_const_for_fn)]
    pub fn backend(&mut self) -> &mut TuiBackend {
        &mut self.backend
    }

    /// Install panic hook for terminal cleanup
    pub fn install_panic_hook() {
        std::panic::set_hook(Box::new(|panic_info| {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(
                stdout(),
                terminal::LeaveAlternateScreen,
                crossterm::event::DisableBracketedPaste,
                crossterm::event::DisableMouseCapture,
                crossterm::cursor::Show,
            );
            let _ = stdout().flush();
            eprintln!("\nRustyCode TUI panicked:");
            eprintln!("{}", panic_info);
            eprintln!("\nPlease report this bug at https://github.com/luengnat/rustycode/issues");
        }));
    }
}

/// Terminal cleanup guard - ensures terminal is restored even on panic
pub struct TerminalCleanupGuard {
    manager: Option<TerminalManager>,
}

impl TerminalCleanupGuard {
    pub const fn new(manager: TerminalManager) -> Self {
        Self {
            manager: Some(manager),
        }
    }
}

impl Drop for TerminalCleanupGuard {
    fn drop(&mut self) {
        if let Some(manager) = self.manager.take() {
            // Restore terminal state - ignore errors since we're in a panic handler
            let _ = manager.cleanup();
        }
    }
}
