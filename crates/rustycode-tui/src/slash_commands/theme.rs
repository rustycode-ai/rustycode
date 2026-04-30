//! Theme slash command
//!
//! Provides quick theme switching via /theme command.

use crate::theme::{builtin_themes, is_dark_theme, Theme, ThemeColors};
use std::sync::{Arc, Mutex};

/// Result of theme command execution
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ThemeCommandResult {
    /// Theme applied successfully
    Success(String),
    /// List of available themes
    List(String),
    /// Error occurred
    Error(String),
}

/// Handle the /theme slash command
pub fn handle_theme_command(
    args: &[&str],
    theme_colors: &Arc<Mutex<ThemeColors>>,
) -> ThemeCommandResult {
    if args.is_empty() {
        // List available themes
        return ThemeCommandResult::List(list_available_themes(theme_colors));
    }

    let subcommand = args[0].to_lowercase();

    match subcommand.as_str() {
        "list" | "ls" => ThemeCommandResult::List(list_available_themes(theme_colors)),
        "next" => {
            let themes = builtin_themes();
            let current_name = get_current_theme_name(theme_colors);

            if let Some(current_idx) = themes.iter().position(|t| t.name == current_name) {
                let next_idx = (current_idx + 1) % themes.len();
                apply_theme(&themes[next_idx], theme_colors);
                ThemeCommandResult::Success(format!("Switched to '{}'", themes[next_idx].name))
            } else {
                // Default to first theme
                apply_theme(&themes[0], theme_colors);
                ThemeCommandResult::Success(format!("Switched to '{}'", themes[0].name))
            }
        }
        "prev" => {
            let themes = builtin_themes();
            let current_name = get_current_theme_name(theme_colors);

            if let Some(current_idx) = themes.iter().position(|t| t.name == current_name) {
                let prev_idx = if current_idx == 0 {
                    themes.len() - 1
                } else {
                    current_idx - 1
                };
                apply_theme(&themes[prev_idx], theme_colors);
                ThemeCommandResult::Success(format!("Switched to '{}'", themes[prev_idx].name))
            } else {
                apply_theme(&themes[0], theme_colors);
                ThemeCommandResult::Success(format!("Switched to '{}'", themes[0].name))
            }
        }
        "light" => {
            let themes = builtin_themes();
            let light_theme = themes.iter().find(|t| !is_dark_theme(&t.colors.background));

            if let Some(theme) = light_theme {
                apply_theme(theme, theme_colors);
                ThemeCommandResult::Success(format!("Switched to light theme '{}'", theme.name))
            } else {
                ThemeCommandResult::Error("No light themes available".to_string())
            }
        }
        "dark" => {
            let themes = builtin_themes();
            let dark_theme = themes.iter().find(|t| is_dark_theme(&t.colors.background));

            if let Some(theme) = dark_theme {
                apply_theme(theme, theme_colors);
                ThemeCommandResult::Success(format!("Switched to dark theme '{}'", theme.name))
            } else {
                ThemeCommandResult::Error("No dark themes available".to_string())
            }
        }
        theme_name => {
            // Try to find theme by name
            let themes = builtin_themes();
            let theme = themes
                .iter()
                .find(|t| t.name == theme_name || t.name == theme_name.replace([' ', '-'], ""));

            if let Some(theme) = theme {
                apply_theme(theme, theme_colors);
                let is_dark = is_dark_theme(&theme.colors.background);
                let style = if is_dark { "Dark" } else { "Light" };
                ThemeCommandResult::Success(format!("Switched to '{}' ({})", theme.name, style))
            } else {
                ThemeCommandResult::Error(format!(
                    "Theme '{}' not found. Use /theme to list available themes.",
                    theme_name
                ))
            }
        }
    }
}

/// List all available themes with their colors
fn list_available_themes(theme_colors: &Arc<Mutex<ThemeColors>>) -> String {
    let themes = builtin_themes();
    let mut themes_sorted = themes.clone();
    themes_sorted.sort_by(|a, b| a.name.cmp(&b.name));

    let current_name = get_current_theme_name(theme_colors);

    let mut output = String::from("\n═══════════════════════════════════════════════════\n");
    output.push_str("           🎨 Available Themes 🎨\n");
    output.push_str("═══════════════════════════════════════════════════\n\n");

    for (idx, theme) in themes_sorted.iter().enumerate() {
        let is_dark = is_dark_theme(&theme.colors.background);
        let style_tag = if is_dark { "🌙" } else { "☀️" };

        // Format colors as hex codes
        let swatch = format!(
            "● {} ● {} ● {}",
            &theme.colors.primary, &theme.colors.success, &theme.colors.error
        );

        // Mark current theme
        let current_marker = if theme.name == current_name {
            "▶ "
        } else {
            "  "
        };

        output.push_str(&format!(
            "{}{:2}. {:20} {}\n",
            current_marker,
            idx + 1,
            theme.name,
            swatch
        ));
        output.push_str(&format!(
            "    {}  Primary: {:10} Success: {:10} Error: {:10}\n",
            style_tag, &theme.colors.primary, &theme.colors.success, &theme.colors.error
        ));
    }

    output.push_str("\n─────────────────────────────────────────────────\n");
    output.push_str("Commands:\n");
    output.push_str("  /theme              - List themes\n");
    output.push_str("  /theme <name>       - Switch to specific theme\n");
    output.push_str("  /theme next         - Cycle to next theme\n");
    output.push_str("  /theme prev         - Cycle to previous theme\n");
    output.push_str("  /theme light        - Switch to light theme\n");
    output.push_str("  /theme dark         - Switch to dark theme\n");
    output.push_str("  Ctrl+T              - Open theme preview UI\n");
    output.push_str("  Alt+T               - Quick cycle to next theme\n");
    output.push_str("─────────────────────────────────────────────────");

    output
}

/// Apply a theme to the current theme colors
fn apply_theme(theme: &Theme, theme_colors: &Arc<Mutex<ThemeColors>>) {
    let colors = ThemeColors::from(theme);
    *theme_colors.lock().unwrap_or_else(|e| e.into_inner()) = colors;
}

/// Get the current theme name by checking background color
fn get_current_theme_name(theme_colors: &Arc<Mutex<ThemeColors>>) -> String {
    let colors = theme_colors.lock().unwrap_or_else(|e| e.into_inner());
    let themes = builtin_themes();

    // Try to match by background color
    for theme in &themes {
        let theme_bg = match colors.background {
            ratatui::style::Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            _ => "unknown".to_string(),
        };
        if theme.colors.background == theme_bg {
            return theme.name.clone();
        }
    }

    // Default to tokyo-night
    "tokyo-night".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_list() {
        let theme_colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let result = handle_theme_command(&[], &theme_colors);
        assert!(matches!(result, ThemeCommandResult::List(_)));
    }

    #[test]
    fn test_theme_next() {
        let theme_colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let result = handle_theme_command(&["next"], &theme_colors);
        assert!(matches!(result, ThemeCommandResult::Success(_)));
    }

    #[test]
    fn test_theme_invalid() {
        let theme_colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let result = handle_theme_command(&["invalid_theme_name"], &theme_colors);
        assert!(matches!(result, ThemeCommandResult::Error(_)));
    }

    #[test]
    fn test_theme_dark_light() {
        let theme_colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let dark_result = handle_theme_command(&["dark"], &theme_colors);
        assert!(matches!(dark_result, ThemeCommandResult::Success(_)));

        let light_result = handle_theme_command(&["light"], &theme_colors);
        // Should work since we have light themes
        assert!(matches!(light_result, ThemeCommandResult::Success(_)));
    }
}
