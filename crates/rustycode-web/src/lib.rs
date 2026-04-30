//! RustyCode Web — Brutalist Edition
//!
//! WebAssembly frontend for RustyCode with brutalist UI design.

use wasm_bindgen::prelude::*;

/// Version information
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the web application
#[wasm_bindgen]
pub fn init() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    Ok(())
}

/// Get the application version
#[wasm_bindgen]
pub fn version() -> String {
    format!("v{}", VERSION)
}

/// Get the application name
#[wasm_bindgen]
pub fn name() -> String {
    "RustyCode".to_string()
}

/// Check if brutalist mode is enabled (always true for web)
#[wasm_bindgen]
pub fn brutalist_mode() -> bool {
    true
}

/// Get the theme colors as JSON
#[wasm_bindgen]
pub fn theme_colors() -> String {
    let colors = [
        ("background", "#2e3440"),
        ("foreground", "#eceff4"),
        ("primary", "#5e81ac"),
        ("secondary", "#81a1c4"),
        ("accent", "#88c0d0"),
        ("success", "#a3be8c"),
        ("warning", "#ebcb8b"),
        ("error", "#bf616a"),
        ("muted", "#4c566a"),
        ("user", "#b48ead"),
        ("ai", "#8fbcbb"),
    ];

    let pairs: Vec<String> = colors.iter().map(|(k, v)| format!("\"{}\":\"{}\"", k, v)).collect();
    format!("{{{}}}", pairs.join(","))
}

/// Format a message for display
#[wasm_bindgen]
pub fn format_message(role: &str, content: &str, timestamp: &str) -> String {
    let role_lower = role.to_lowercase();
    let role_display = match role_lower.as_str() {
        "user" => "you",
        "ai" | "assistant" => "ai",
        "system" => "sys",
        _ => &role_lower,
    };

    format!(
        "▐ {} ({})\n  {}",
        role_display,
        timestamp,
        content.replace('\n', "\n  ")
    )
}

/// Format a tool execution for display
#[wasm_bindgen]
pub fn format_tool(name: &str, status: &str) -> String {
    let icon = match status {
        "running" => "◐",
        "complete" | "success" => "●",
        "failed" | "error" => "✗",
        _ => "○",
    };

    format!("  {} ╶─ {} ─╴", icon, name)
}

/// Greeting message for new users
#[wasm_bindgen]
pub fn greeting() -> String {
    "╶─ RustyCode ─╴\n\nautonomous development framework\n\ntype a message and press Enter to start\npress ? for help".to_string()
}

/// Streaming animation frames (4-cycle)
#[wasm_bindgen]
pub fn streaming_frame(frame: usize) -> char {
    match frame % 4 {
        0 => '◐',
        1 => '◑',
        2 => '◒',
        _ => '◓',
    }
}

/// Console log for debugging
#[wasm_bindgen]
pub fn log(message: &str) {
    web_sys::console::log_1(&message.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(version().starts_with('v'));
    }

    #[test]
    fn test_name() {
        assert_eq!(name(), "RustyCode");
    }

    #[test]
    fn test_brutalist_mode() {
        assert!(brutalist_mode());
    }

    #[test]
    fn test_format_message() {
        let formatted = format_message("user", "hello", "12:00");
        assert!(formatted.contains("you (12:00)"));
        assert!(formatted.contains("hello"));
    }

    #[test]
    fn test_streaming_frame() {
        assert_eq!(streaming_frame(0), '◐');
        assert_eq!(streaming_frame(1), '◑');
        assert_eq!(streaming_frame(2), '◒');
        assert_eq!(streaming_frame(3), '◓');
        assert_eq!(streaming_frame(4), '◐'); // cycles
    }

    #[test]
    fn test_format_message_ai_role() {
        let formatted = format_message("assistant", "world", "10:30");
        assert!(formatted.contains("ai (10:30)"));
        assert!(formatted.contains("world"));
    }

    #[test]
    fn test_format_message_system_role() {
        let formatted = format_message("system", "ready", "00:01");
        assert!(formatted.contains("sys (00:01)"));
    }

    #[test]
    fn test_format_message_unknown_role() {
        let formatted = format_message("custom", "test", "now");
        assert!(formatted.contains("custom (now)"));
    }

    #[test]
    fn test_format_message_multiline() {
        let formatted = format_message("user", "line1\nline2\nline3", "12:00");
        assert!(formatted.contains("line1\n  line2\n  line3"));
    }

    #[test]
    fn test_format_tool_running() {
        let result = format_tool("bash", "running");
        assert!(result.contains('◐'));
        assert!(result.contains("bash"));
    }

    #[test]
    fn test_format_tool_complete() {
        let result = format_tool("read_file", "complete");
        assert!(result.contains('●'));
        assert!(result.contains("read_file"));
    }

    #[test]
    fn test_format_tool_failed() {
        let result = format_tool("bash", "failed");
        assert!(result.contains('✗'));
    }

    #[test]
    fn test_format_tool_unknown_status() {
        let result = format_tool("tool", "pending");
        assert!(result.contains('○'));
    }

    #[test]
    fn test_greeting() {
        let g = greeting();
        assert!(g.contains("RustyCode"));
        assert!(g.contains("autonomous development"));
    }

    #[test]
    fn test_theme_colors_valid_json() {
        let json = theme_colors();
        assert!(json.starts_with('{'));
        assert!(json.ends_with('}'));
        assert!(json.contains("\"background\""));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_theme_colors_contains_expected_keys() {
        let json = theme_colors();
        let expected_keys = [
            "background", "foreground", "primary", "secondary", "accent",
            "success", "warning", "error", "muted", "user", "ai",
        ];
        for key in &expected_keys {
            assert!(json.contains(&format!("\"{}\"", key)), "missing key: {}", key);
        }
    }

    #[test]
    fn test_streaming_frame_large_values() {
        // Verify cycling works with large frame numbers
        assert_eq!(streaming_frame(100), '◐');
        assert_eq!(streaming_frame(101), '◑');
        assert_eq!(streaming_frame(102), '◒');
        assert_eq!(streaming_frame(103), '◓');
    }
}
