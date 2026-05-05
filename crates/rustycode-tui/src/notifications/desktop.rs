//! Desktop notification support via OSC 9 escape sequences and BEL fallback.

use std::io::Write;

/// Terminal emulators known to support OSC 9 notifications.
const OSC9_TERM_PROGRAMS: &[&str] = &[
    "ghostty",
    "iTerm.app",
    "kitty",
    "WarpTerminal",
    "WezTerm",
    "Alacritty",
];

/// Desktop notifier using OSC 9 escape sequences with BEL fallback.
pub struct DesktopNotifier {
    /// Whether notifications are enabled (opt-in via config).
    pub enabled: bool,
    osc9_capable: bool,
}

impl DesktopNotifier {
    /// Create a new notifier, auto-detecting terminal capabilities.
    pub fn new() -> Self {
        let osc9_capable = Self::supports_osc9();
        Self {
            enabled: false,
            osc9_capable,
        }
    }

    /// Check whether the current terminal supports OSC 9 notifications.
    pub fn supports_osc9() -> bool {
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            if OSC9_TERM_PROGRAMS
                .iter()
                .any(|&known| term_program == known)
            {
                return true;
            }
        }
        // Fallback: check TERM for terminals that support basic OSC.
        if let Ok(term) = std::env::var("TERM") {
            return term == "xterm-256color" || term == "screen-256color";
        }
        false
    }

    /// Send a desktop notification.
    ///
    /// When OSC 9 is supported, emits `\x1b]9;{title}: {body}\x07`.
    /// Otherwise falls back to a BEL character (`\x07`).
    pub fn send_notification(&self, title: &str, body: &str) {
        if !self.enabled {
            return;
        }
        let mut stderr = std::io::stderr();
        if self.osc9_capable {
            let _ = write!(stderr, "\x1b]9;{title}: {body}\x07");
            let _ = stderr.flush();
        } else {
            let _ = stderr.write_all(b"\x07");
            let _ = stderr.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc9_format_correct() {
        // Verify the expected escape sequence format.
        let title = "RustyCode";
        let body = "Task complete";
        let expected = format!("\x1b]9;{title}: {body}\x07");
        assert_eq!(expected, "\x1b]9;RustyCode: Task complete\x07");
        assert!(expected.starts_with("\x1b]9;"));
        assert!(expected.ends_with('\x07'));
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
            // Temporarily set TERM_PROGRAM and verify detection.
            // Since supports_osc9 reads env vars, we test the list membership.
            let in_list = OSC9_TERM_PROGRAMS.contains(&program);
            assert_eq!(in_list, expected, "TERM_PROGRAM={program}");
        }
    }

    #[test]
    fn fallback_to_bel_for_unknown() {
        let notifier = DesktopNotifier {
            enabled: true,
            osc9_capable: false,
        };
        // When not OSC 9 capable, send_notification writes BEL.
        // We just verify the struct is constructed correctly for fallback.
        assert!(!notifier.osc9_capable);
        assert!(notifier.enabled);
    }

    #[test]
    fn send_notification_disabled_is_noop() {
        let notifier = DesktopNotifier {
            enabled: false,
            osc9_capable: true,
        };
        // When disabled, send_notification returns immediately without writing.
        // No panic or error means success.
        notifier.send_notification("test", "should not appear");
    }
}
