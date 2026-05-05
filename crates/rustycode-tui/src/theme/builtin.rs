use crate::ui::message_types::MessageRole;
use crate::ui::message_types::ToolStatus;
use crate::ui::spinner::SpinnerStatus;
use crate::ui::toast::ToastLevel;
use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub background: Color,
    pub foreground: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub muted: Color,
}

impl ThemeColors {
    /// Get color for a tool execution status
    pub fn tool_status_color(&self, status: &ToolStatus) -> Color {
        match status {
            ToolStatus::Running => self.warning,
            ToolStatus::Complete => self.success,
            ToolStatus::Failed => self.error,
            ToolStatus::Cancelled => self.muted,
        }
    }

    pub fn message_pipe_style(&self, role: &MessageRole) -> (char, Color) {
        match role {
            MessageRole::User => ('▌', self.secondary),
            MessageRole::Assistant => ('▌', self.primary),
            MessageRole::System => ('│', self.muted),
        }
    }

    /// Get color for a message role label
    pub fn message_role_color(&self, role: &MessageRole) -> Color {
        match role {
            MessageRole::User => self.secondary,
            MessageRole::Assistant => self.primary,
            MessageRole::System => self.muted,
        }
    }

    /// Get color for a toast notification level
    pub fn toast_level_color(&self, level: &ToastLevel) -> Color {
        match level {
            ToastLevel::Info => self.accent,
            ToastLevel::Success => self.success,
            ToastLevel::Warning => self.warning,
            ToastLevel::Error => self.error,
        }
    }

    /// Get color for a spinner status
    pub fn spinner_status_color(&self, status: &SpinnerStatus) -> Color {
        match status {
            SpinnerStatus::Working => self.accent,
            SpinnerStatus::Done => self.success,
            SpinnerStatus::Error => self.error,
            SpinnerStatus::Cancelled => self.muted,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeDefinition {
    pub name: String,
    pub colors: ThemeDefinitionColors,
}

#[derive(Debug, Clone)]
pub struct ThemeDefinitionColors {
    pub background: String,
    pub foreground: String,
    pub primary: String,
    pub secondary: String,
    pub accent: String,
    pub success: String,
    pub warning: String,
    pub error: String,
    pub muted: String,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub colors: ThemeDefinitionColors,
}

impl Default for Theme {
    fn default() -> Self {
        builtin_themes()
            .into_iter()
            .find(|t| t.name == "tokyo-night")
            .unwrap_or_else(|| {
                builtin_themes()
                    .into_iter()
                    .next()
                    .expect("builtin_themes() must always return at least one theme")
            })
    }
}

impl From<&Theme> for ThemeColors {
    fn from(theme: &Theme) -> Self {
        Self {
            background: super::parse_color(&theme.colors.background),
            foreground: super::parse_color(&theme.colors.foreground),
            primary: super::parse_color(&theme.colors.primary),
            secondary: super::parse_color(&theme.colors.secondary),
            accent: super::parse_color(&theme.colors.accent),
            success: super::parse_color(&theme.colors.success),
            warning: super::parse_color(&theme.colors.warning),
            error: super::parse_color(&theme.colors.error),
            muted: super::parse_color(&theme.colors.muted),
        }
    }
}

pub fn builtin_themes() -> Vec<Theme> {
    vec![
        Theme {
            name: "pitch-black".into(),
            colors: ThemeDefinitionColors {
                background: "#000000".into(),
                foreground: "#e0e0e0".into(), // Soft white
                primary: "#61afef".into(),    // Readable blue
                secondary: "#c678dd".into(),  // Purple
                accent: "#56b6c2".into(),     // Teal
                success: "#98c379".into(),    // Soft green
                warning: "#e5c07b".into(),    // Soft yellow
                error: "#e06c75".into(),      // Soft red
                muted: "#5c6370".into(),      // Grey
            },
        },
        Theme {
            name: "tokyo-night".into(),
            colors: ThemeDefinitionColors {
                background: "#1a1b26".into(),
                foreground: "#a9b1d6".into(),
                primary: "#7aa2f7".into(),
                secondary: "#bb9af7".into(),
                accent: "#7dcfff".into(),
                success: "#9ece6a".into(),
                warning: "#e0af68".into(),
                error: "#f7768e".into(),
                muted: "#565f89".into(),
            },
        },
        Theme {
            name: "catppuccin-mocha".into(),
            colors: ThemeDefinitionColors {
                background: "#1e1e2e".into(),
                foreground: "#cdd6f4".into(),
                primary: "#89b4fa".into(),
                secondary: "#cba6f7".into(),
                accent: "#94e2d5".into(),
                success: "#a6e3a1".into(),
                warning: "#f9e2af".into(),
                error: "#f38ba8".into(),
                muted: "#6c7086".into(),
            },
        },
        Theme {
            name: "dracula".into(),
            colors: ThemeDefinitionColors {
                background: "#282a36".into(),
                foreground: "#f8f8f2".into(),
                primary: "#bd93f9".into(),
                secondary: "#ff79c6".into(),
                accent: "#8be9fd".into(),
                success: "#50fa7b".into(),
                warning: "#f1fa8c".into(),
                error: "#ff5555".into(),
                muted: "#6272a4".into(),
            },
        },
        Theme {
            name: "gruvbox-dark".into(),
            colors: ThemeDefinitionColors {
                background: "#1d1f21".into(),
                foreground: "#ebdbb2".into(),
                primary: "#83a598".into(),
                secondary: "#d3869b".into(),
                accent: "#8ec07c".into(),
                success: "#b8bb26".into(),
                warning: "#fabd2f".into(),
                error: "#fb4934".into(),
                muted: "#504945".into(),
            },
        },
        Theme {
            name: "nord".into(),
            colors: ThemeDefinitionColors {
                background: "#2e3440".into(),
                foreground: "#d8dee9".into(),
                primary: "#88c0d0".into(),
                secondary: "#b48ead".into(),
                accent: "#8fbcbb".into(),
                success: "#a3be8c".into(),
                warning: "#ebcb8b".into(),
                error: "#bf616a".into(),
                muted: "#4c566a".into(),
            },
        },
        Theme {
            name: "solarized-dark".into(),
            colors: ThemeDefinitionColors {
                background: "#002b36".into(),
                foreground: "#839496".into(),
                primary: "#268bd2".into(),
                secondary: "#6c71c4".into(),
                accent: "#2aa198".into(),
                success: "#859900".into(),
                warning: "#b58900".into(),
                error: "#dc322f".into(),
                muted: "#586e75".into(),
            },
        },
        Theme {
            name: "one-dark".into(),
            colors: ThemeDefinitionColors {
                background: "#282c34".into(),
                foreground: "#abb2bf".into(),
                primary: "#61afef".into(),
                secondary: "#c678dd".into(),
                accent: "#56b6c2".into(),
                success: "#98c379".into(),
                warning: "#e5c07b".into(),
                error: "#e06c75".into(),
                muted: "#5c6370".into(),
            },
        },
        Theme {
            name: "github-dark".into(),
            colors: ThemeDefinitionColors {
                background: "#0d1117".into(),
                foreground: "#c9d1d9".into(),
                primary: "#58a6ff".into(),
                secondary: "#bc8cff".into(),
                accent: "#39d353".into(),
                success: "#3fb950".into(),
                warning: "#d29922".into(),
                error: "#f85149".into(),
                muted: "#484f58".into(),
            },
        },
        Theme {
            name: "monokai".into(),
            colors: ThemeDefinitionColors {
                background: "#272822".into(),
                foreground: "#f8f8f2".into(),
                primary: "#66d9ef".into(),
                secondary: "#ae81ff".into(),
                accent: "#a6e22e".into(),
                success: "#a6e22e".into(),
                warning: "#e6db74".into(),
                error: "#f92672".into(),
                muted: "#49483e".into(),
            },
        },
        Theme {
            name: "rose-pine".into(),
            colors: ThemeDefinitionColors {
                background: "#191724".into(),
                foreground: "#e0def4".into(),
                primary: "#c4a7e7".into(),
                secondary: "#eb6f92".into(),
                accent: "#f6c177".into(),
                success: "#9ccfd8".into(),
                warning: "#f6c177".into(),
                error: "#eb6f92".into(),
                muted: "#6e6a86".into(),
            },
        },
        Theme {
            name: "kanagawa".into(),
            colors: ThemeDefinitionColors {
                background: "#1f1f28".into(),
                foreground: "#dcd7ba".into(),
                primary: "#7e9cd8".into(),
                secondary: "#957fb8".into(),
                accent: "#e6c384".into(),
                success: "#76946a".into(),
                warning: "#e6c384".into(),
                error: "#c34043".into(),
                muted: "#54546d".into(),
            },
        },
        Theme {
            name: "alabaster-light".into(),
            colors: ThemeDefinitionColors {
                background: "#f7f7f7".into(),
                foreground: "#434343".into(),
                primary: "#4271ae".into(),
                secondary: "#8959a8".into(),
                accent: "#3e999f".into(),
                success: "#718c00".into(),
                warning: "#eab700".into(),
                error: "#c82829".into(),
                muted: "#b0b0b0".into(),
            },
        },
        Theme {
            name: "solarized-light".into(),
            colors: ThemeDefinitionColors {
                background: "#fdf6e3".into(),
                foreground: "#657b83".into(),
                primary: "#268bd2".into(),
                secondary: "#6c71c4".into(),
                accent: "#2aa198".into(),
                success: "#859900".into(),
                warning: "#b58900".into(),
                error: "#dc322f".into(),
                muted: "#93a1a1".into(),
            },
        },
        Theme {
            name: "github-light".into(),
            colors: ThemeDefinitionColors {
                background: "#ffffff".into(),
                foreground: "#24292f".into(),
                primary: "#0969da".into(),
                secondary: "#8250df".into(),
                accent: "#1a7f37".into(),
                success: "#1a7f37".into(),
                warning: "#9a6700".into(),
                error: "#cf222e".into(),
                muted: "#8c959f".into(),
            },
        },
        Theme {
            name: "catppuccin-latte".into(),
            colors: ThemeDefinitionColors {
                background: "#eff1f5".into(),
                foreground: "#4c4f69".into(),
                primary: "#1e66f5".into(),
                secondary: "#8839ef".into(),
                accent: "#179299".into(),
                success: "#40a02b".into(),
                warning: "#df8e1d".into(),
                error: "#d20f39".into(),
                muted: "#9ca0b0".into(),
            },
        },
        Theme {
            name: "nord-light".into(),
            colors: ThemeDefinitionColors {
                background: "#eceff4".into(),
                foreground: "#2e3440".into(),
                primary: "#5e81ac".into(),
                secondary: "#b48ead".into(),
                accent: "#8fbcbb".into(),
                success: "#a3be8c".into(),
                warning: "#ebcb8b".into(),
                error: "#bf616a".into(),
                muted: "#d8dee9".into(),
            },
        },
    ]
}
