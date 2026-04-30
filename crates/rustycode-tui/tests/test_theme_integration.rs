#![allow(
    clippy::cast_lossless,
    clippy::items_after_statements,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]

//! Integration tests for live theme switching

use ratatui::style::Color;
use rustycode_tui::theme::{builtin_themes, parse_color, Theme, ThemeColors};

#[test]
fn test_theme_list_includes_all_themes() {
    let themes = builtin_themes();
    assert!(themes.len() >= 16, "Should have at least 16 themes");

    // Check that key themes exist
    let theme_names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
    assert!(theme_names.contains(&"tokyo-night"));
    assert!(theme_names.contains(&"dracula"));
    assert!(theme_names.contains(&"monokai"));
    assert!(theme_names.contains(&"nord"));
    assert!(theme_names.contains(&"gruvbox-dark"));
    assert!(theme_names.contains(&"alabaster-light"));
}

#[test]
fn test_theme_find_by_name() {
    let themes = builtin_themes();
    let tokyo = themes.iter().find(|t| t.name == "tokyo-night");
    assert!(tokyo.is_some());
    assert_eq!(tokyo.unwrap().name, "tokyo-night");

    let nonexistent = themes.iter().find(|t| t.name == "nonexistent-theme");
    assert!(nonexistent.is_none());
}

#[test]
fn test_theme_colors_conversion() {
    let themes = builtin_themes();

    for theme in themes.iter().take(10) {
        // Test that all themes can be converted to ThemeColors
        let colors = ThemeColors::from(theme);

        // Verify all colors parse correctly
        use ratatui::style::Color;
        match colors.background {
            Color::Rgb(_, _, _) => {}
            _ => panic!("Theme {} has invalid background color", theme.name),
        }

        match colors.primary {
            Color::Rgb(_, _, _) => {}
            _ => panic!("Theme {} has invalid primary color", theme.name),
        }
    }
}

#[test]
fn test_parse_color() {
    let color = parse_color("#7aa2f7");
    use ratatui::style::Color;
    match color {
        Color::Rgb(r, g, b) => {
            assert_eq!(r, 122);
            assert_eq!(g, 162);
            assert_eq!(b, 247);
        }
        _ => panic!("Expected RGB color"),
    }

    // Test without hash — returns Reset (parser requires '#' prefix)
    let color = parse_color("ff5555");
    assert_eq!(color, Color::Reset);

    // Test invalid color
    let color = parse_color("invalid");
    assert_eq!(color, Color::Reset);
}

#[test]
fn test_dark_theme_detection() {
    fn is_dark_theme(background: &str) -> bool {
        let bg = parse_color(background);
        match bg {
            Color::Rgb(r, g, b) => {
                let luminance = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
                luminance < 0.5
            }
            _ => true,
        }
    }

    // Tokyo Night should be dark
    assert!(is_dark_theme("#1a1b26"));

    // Gruvbox Light should be light
    assert!(!is_dark_theme("#fbf1c7"));

    // GitHub Light should be light
    assert!(!is_dark_theme("#ffffff"));

    // Dracula should be dark
    assert!(is_dark_theme("#282a36"));

    // Nord should be dark
    assert!(is_dark_theme("#2e3440"));
}

#[test]
fn test_theme_default() {
    let theme = Theme::default();
    assert_eq!(theme.name, "tokyo-night");
}

#[test]
fn test_theme_colors_from_theme() {
    let theme = Theme::default();
    let colors = ThemeColors::from(&theme);

    use ratatui::style::Color;
    match colors.background {
        Color::Rgb(r, g, b) => {
            // Tokyo Night background (#1a1b26)
            assert_eq!(r, 26);
            assert_eq!(g, 27);
            assert_eq!(b, 38);
        }
        _ => panic!("Expected RGB color"),
    }

    match colors.primary {
        Color::Rgb(_, _, _) => {}
        _ => panic!("Expected RGB color for primary"),
    }
}

#[test]
fn test_builtin_themes_consistency() {
    let themes = builtin_themes();

    // Verify all themes have valid color data
    for theme in &themes {
        assert!(!theme.name.is_empty());
        assert!(!theme.colors.background.is_empty());
        assert!(!theme.colors.foreground.is_empty());
        assert!(!theme.colors.primary.is_empty());
        assert!(!theme.colors.secondary.is_empty());
        assert!(!theme.colors.accent.is_empty());
        assert!(!theme.colors.success.is_empty());
        assert!(!theme.colors.warning.is_empty());
        assert!(!theme.colors.error.is_empty());
        assert!(!theme.colors.muted.is_empty());

        // Verify all colors parse correctly
        assert!(parse_color(&theme.colors.background) != Color::Reset);
        assert!(parse_color(&theme.colors.foreground) != Color::Reset);
        assert!(parse_color(&theme.colors.primary) != Color::Reset);
    }
}
