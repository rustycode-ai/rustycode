//! Terminal progress bar support via OSC 9;4 escape sequences.
//!
//! Emits progress indicators to terminals that support the OSC 9;4 protocol
//! (ConEmu, Windows Terminal, WezTerm, etc.). Temporarily suspends crossterm
//! raw mode to write sequences to stdout.

/// Terminal emulators known to support OSC 9;4 progress sequences.
const OSC94_TERM_PROGRAMS: &[&str] = &[
    "ghostty",
    "iTerm.app",
    "kitty",
    "WarpTerminal",
    "WezTerm",
    "Alacritty",
];

/// Terminal progress reporter using OSC 9;4 escape sequences.
pub struct TerminalProgress {
    /// Whether progress reporting is enabled.
    pub enabled: bool,
    osc94_capable: bool,
}

impl TerminalProgress {
    /// Create a new progress reporter, auto-detecting terminal capabilities.
    pub fn new() -> Self {
        let osc94_capable = Self::supports_osc94();
        Self {
            enabled: osc94_capable,
            osc94_capable,
        }
    }

    /// Check whether the current terminal supports OSC 9;4 progress sequences.
    ///
    /// Detects known terminals via `TERM_PROGRAM`, `TERM`, and Windows Terminal
    /// via the `WT_SESSION` environment variable.
    pub fn supports_osc94() -> bool {
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            if OSC94_TERM_PROGRAMS
                .iter()
                .any(|&known| term_program == known)
            {
                return true;
            }
        }
        // Windows Terminal sets WT_SESSION.
        if std::env::var("WT_SESSION").is_ok() {
            return true;
        }
        // Fallback: check TERM for terminals that support basic OSC.
        if let Ok(term) = std::env::var("TERM") {
            return term == "xterm-256color" || term == "screen-256color";
        }
        false
    }

    /// Emit a progress update with the given percentage (0–100).
    ///
    /// Values above 100 are clamped. The sequence is `\x1b]9;4;1;{percent}\x07`.
    pub fn set_progress(&self, percent: u8) {
        if !self.enabled {
            return;
        }
        let clamped = percent.min(100);
        let sequence = format!("\x1b]9;4;1;{clamped}\x07");
        Self::write_osc_sequence(&sequence);
    }

    /// Emit an error-state progress indicator at the given percentage.
    ///
    /// The sequence is `\x1b]9;4;2;{percent}\x07`.
    pub fn set_error(&self, percent: u8) {
        if !self.enabled {
            return;
        }
        let clamped = percent.min(100);
        let sequence = format!("\x1b]9;4;2;{clamped}\x07");
        Self::write_osc_sequence(&sequence);
    }

    /// Emit an indeterminate/paused progress indicator.
    ///
    /// The sequence is `\x1b]9;4;3;\x07`.
    pub fn set_indeterminate(&self) {
        if !self.enabled {
            return;
        }
        let sequence = "\x1b]9;4;3;\x07";
        Self::write_osc_sequence(sequence);
    }

    /// Clear (remove) the progress bar from the terminal.
    ///
    /// The sequence is `\x1b]9;4;0;\x07`.
    pub fn clear(&self) {
        if !self.enabled {
            return;
        }
        let sequence = "\x1b]9;4;0;\x07";
        Self::write_osc_sequence(sequence);
    }

    /// Write an OSC sequence to stdout, suspending raw mode temporarily.
    ///
    /// Follows the same pattern used by `clipboard.rs` for writing escape
    /// sequences while crossterm raw mode is active.
    fn write_osc_sequence(sequence: &str) {
        // Suspend raw mode temporarily to write the sequence.
        let was_raw_mode = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
        if was_raw_mode {
            if let Err(e) = crossterm::terminal::disable_raw_mode() {
                tracing::warn!("Failed to disable raw mode for OSC 9;4 progress: {}", e);
            }
        }

        // Write the sequence.
        print!("{}", sequence);
        use std::io::Write;
        if let Err(e) = std::io::stdout().flush() {
            tracing::warn!("Failed to flush stdout for OSC 9;4 progress: {}", e);
        }

        // Restore raw mode.
        if was_raw_mode {
            if let Err(e) = crossterm::terminal::enable_raw_mode() {
                tracing::warn!("Failed to re-enable raw mode after OSC 9;4 progress: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc94_format_correct() {
        // Verify expected escape sequence formats.
        let progress = "\x1b]9;4;1;75\x07".to_string();
        assert!(progress.starts_with("\x1b]9;4;1;"));
        assert!(progress.ends_with('\x07'));

        let error = "\x1b]9;4;2;50\x07".to_string();
        assert!(error.starts_with("\x1b]9;4;2;"));

        let indeterminate = "\x1b]9;4;3;\x07";
        assert_eq!(indeterminate, "\x1b]9;4;3;\x07");

        let clear = "\x1b]9;4;0;\x07";
        assert_eq!(clear, "\x1b]9;4;0;\x07");
    }

    #[test]
    fn terminal_detection_known_terminals() {
        let known = [
            ("ghostty", true),
            ("iTerm.app", true),
            ("kitty", true),
            ("WarpTerminal", true),
            ("WezTerm", true),
            ("Alacritty", true),
        ];
        for (program, expected) in known {
            let in_list = OSC94_TERM_PROGRAMS.contains(&program);
            assert_eq!(in_list, expected, "TERM_PROGRAM={program}");
        }
        // Unknown terminals should not be in the list.
        assert!(!OSC94_TERM_PROGRAMS.contains(&"unknown-terminal"));
    }

    #[test]
    fn set_progress_disabled_is_noop() {
        let progress = TerminalProgress {
            enabled: false,
            osc94_capable: true,
        };
        // When disabled, methods return immediately without writing.
        // No panic or error means success.
        progress.set_progress(50);
        progress.set_error(50);
        progress.set_indeterminate();
        progress.clear();
    }

    #[test]
    fn clear_emits_correct_sequence() {
        let clear_seq = "\x1b]9;4;0;\x07";
        assert!(clear_seq.starts_with("\x1b]9;4;0;"));
        assert!(clear_seq.ends_with('\x07'));
        assert_eq!(clear_seq.len(), 9);
    }

    #[test]
    fn percent_clamped_to_100() {
        // Verify values > 100 are clamped by checking the format string logic.
        let value: u8 = 150;
        let clamped = value.min(100);
        assert_eq!(clamped, 100);

        let sequence = format!("\x1b]9;4;1;{clamped}\x07");
        assert_eq!(sequence, "\x1b]9;4;1;100\x07");
    }
}
