use crate::models::parse_hex_color;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub name: String,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub muted: String,
    pub border: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
    pub selection_bg: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::builtin_themes().into_iter().next().unwrap()
    }
}

impl ThemeConfig {
    pub fn builtin_themes() -> Vec<Self> {
        vec![
            Self {
                name: "Dark (Default)".to_string(),
                bg: "#232323".to_string(),
                fg: "#e5e7eb".to_string(),
                accent: "#22d3ee".to_string(),
                muted: "#6b7280".to_string(),
                border: "#4b5563".to_string(),
                success: "#22c55e".to_string(),
                warning: "#eab308".to_string(),
                danger: "#ef4444".to_string(),
                selection_bg: "#374151".to_string(),
            },
            Self {
                name: "Catppuccin Mocha".to_string(),
                bg: "#1e1e2e".to_string(),
                fg: "#cdd6f4".to_string(),
                accent: "#89b4fa".to_string(),
                muted: "#6c7086".to_string(),
                border: "#45475a".to_string(),
                success: "#a6e3a1".to_string(),
                warning: "#f9e2af".to_string(),
                danger: "#f38ba8".to_string(),
                selection_bg: "#313244".to_string(),
            },
            Self {
                name: "Gruvbox Dark".to_string(),
                bg: "#282828".to_string(),
                fg: "#ebdbb2".to_string(),
                accent: "#83a598".to_string(),
                muted: "#928374".to_string(),
                border: "#504945".to_string(),
                success: "#b8bb26".to_string(),
                warning: "#fabd2f".to_string(),
                danger: "#fb4934".to_string(),
                selection_bg: "#3c3836".to_string(),
            },
            Self {
                name: "Tokyo Night".to_string(),
                bg: "#1a1b26".to_string(),
                fg: "#c0caf5".to_string(),
                accent: "#7aa2f7".to_string(),
                muted: "#565f89".to_string(),
                border: "#3b4261".to_string(),
                success: "#9ece6a".to_string(),
                warning: "#e0af68".to_string(),
                danger: "#f7768e".to_string(),
                selection_bg: "#283457".to_string(),
            },
            Self {
                name: "Nord".to_string(),
                bg: "#2e3440".to_string(),
                fg: "#eceff4".to_string(),
                accent: "#88c0d0".to_string(),
                muted: "#616e88".to_string(),
                border: "#4c566a".to_string(),
                success: "#a3be8c".to_string(),
                warning: "#ebcb8b".to_string(),
                danger: "#bf616a".to_string(),
                selection_bg: "#3b4252".to_string(),
            },
        ]
    }

    pub fn bg_color(&self) -> Color {
        parse_hex_color(&self.bg).unwrap_or(Color::Reset)
    }

    pub fn fg_color(&self) -> Color {
        parse_hex_color(&self.fg).unwrap_or(Color::White)
    }

    pub fn accent_color(&self) -> Color {
        parse_hex_color(&self.accent).unwrap_or(Color::Cyan)
    }

    pub fn muted_color(&self) -> Color {
        parse_hex_color(&self.muted).unwrap_or(Color::DarkGray)
    }

    pub fn border_color(&self) -> Color {
        parse_hex_color(&self.border).unwrap_or(Color::DarkGray)
    }

    pub fn success_color(&self) -> Color {
        parse_hex_color(&self.success).unwrap_or(Color::Green)
    }

    pub fn warning_color(&self) -> Color {
        parse_hex_color(&self.warning).unwrap_or(Color::Yellow)
    }

    pub fn danger_color(&self) -> Color {
        parse_hex_color(&self.danger).unwrap_or(Color::Red)
    }

    pub fn selection_bg_color(&self) -> Color {
        parse_hex_color(&self.selection_bg).unwrap_or(Color::DarkGray)
    }
}
