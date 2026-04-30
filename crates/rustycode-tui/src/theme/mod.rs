pub mod builtin;
pub use builtin::{builtin_themes, Theme, ThemeColors};

use ratatui::style::Color;

pub fn parse_color(color: &str) -> Color {
    match color {
        "black" => Color::Black,
        "blue" => Color::Blue,
        "cyan" => Color::Cyan,
        "green" => Color::Green,
        "magenta" => Color::Magenta,
        "red" => Color::Red,
        "white" => Color::White,
        "yellow" => Color::Yellow,
        "gray" => Color::Gray,
        _ => {
            if color.starts_with('#') && color.len() == 7 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&color[1..3], 16),
                    u8::from_str_radix(&color[3..5], 16),
                    u8::from_str_radix(&color[5..7], 16),
                ) {
                    return Color::Rgb(r, g, b);
                }
            }
            Color::Reset
        }
    }
}

pub fn is_dark_theme(bg_color: &str) -> bool {
    matches!(
        bg_color,
        "#000000"
            | "#101010"
            | "#1e1e1e"
            | "#282a36"
            | "#1a1b26"
            | "#0d1117"
            | "#21252b"
            | "#2d2d2d"
            | "#1e1e2e"
            | "#1d1f21"
            | "#272822"
            | "#002b36"
            | "#1f1f28"
            | "#191724"
            | "#313244"
            | "#11111b"
    )
}
