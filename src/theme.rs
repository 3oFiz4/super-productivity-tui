use crate::models::parse_hex_color;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub name: String,
    pub bg: String,
    #[serde(default = "default_sidebar_bg")]
    pub sidebar_bg: String,
    #[serde(default = "default_status_bg")]
    pub status_bg: String,
    pub fg: String,
    pub accent: String,
    pub muted: String,
    pub border: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
    pub selection_bg: String,
    #[serde(default = "default_hint_fg")]
    pub hint_fg: String,
}

fn default_hint_fg() -> String {
    "#eab308".to_string()
}

fn default_sidebar_bg() -> String {
    "#1e1e1e".to_string()
}

fn default_status_bg() -> String {
    "#181818".to_string()
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
                sidebar_bg: "#1e1e1e".to_string(),
                status_bg: "#181818".to_string(),
                fg: "#e5e7eb".to_string(),
                accent: "#22d3ee".to_string(),
                muted: "#6b7280".to_string(),
                border: "#4b5563".to_string(),
                success: "#22c55e".to_string(),
                warning: "#eab308".to_string(),
                danger: "#ef4444".to_string(),
                selection_bg: "#374151".to_string(),
                hint_fg: "#eab308".to_string(),
            },
            Self {
                name: "Catppuccin Mocha".to_string(),
                bg: "#1e1e2e".to_string(),
                sidebar_bg: "#181825".to_string(),
                status_bg: "#11111b".to_string(),
                fg: "#cdd6f4".to_string(),
                accent: "#89b4fa".to_string(),
                muted: "#6c7086".to_string(),
                border: "#45475a".to_string(),
                success: "#a6e3a1".to_string(),
                warning: "#f9e2af".to_string(),
                danger: "#f38ba8".to_string(),
                selection_bg: "#313244".to_string(),
                hint_fg: "#f9e2af".to_string(),
            },
            Self {
                name: "Gruvbox Dark".to_string(),
                bg: "#282828".to_string(),
                sidebar_bg: "#1d2021".to_string(),
                status_bg: "#1d2021".to_string(),
                fg: "#ebdbb2".to_string(),
                accent: "#83a598".to_string(),
                muted: "#928374".to_string(),
                border: "#504945".to_string(),
                success: "#b8bb26".to_string(),
                warning: "#fabd2f".to_string(),
                danger: "#fb4934".to_string(),
                selection_bg: "#3c3836".to_string(),
                hint_fg: "#fabd2f".to_string(),
            },
            Self {
                name: "Tokyo Night".to_string(),
                bg: "#1a1b26".to_string(),
                sidebar_bg: "#16161e".to_string(),
                status_bg: "#16161e".to_string(),
                fg: "#c0caf5".to_string(),
                accent: "#7aa2f7".to_string(),
                muted: "#565f89".to_string(),
                border: "#3b4261".to_string(),
                success: "#9ece6a".to_string(),
                warning: "#e0af68".to_string(),
                danger: "#f7768e".to_string(),
                selection_bg: "#283457".to_string(),
                hint_fg: "#e0af68".to_string(),
            },
            Self {
                name: "Nord".to_string(),
                bg: "#2e3440".to_string(),
                sidebar_bg: "#272c36".to_string(),
                status_bg: "#242933".to_string(),
                fg: "#eceff4".to_string(),
                accent: "#88c0d0".to_string(),
                muted: "#616e88".to_string(),
                border: "#4c566a".to_string(),
                success: "#a3be8c".to_string(),
                warning: "#ebcb8b".to_string(),
                danger: "#bf616a".to_string(),
                selection_bg: "#3b4252".to_string(),
                hint_fg: "#ebcb8b".to_string(),
            },
        ]
    }

    pub fn bg_color(&self) -> Color {
        parse_hex_color(&self.bg).unwrap_or(Color::Reset)
    }

    pub fn sidebar_bg_color(&self) -> Color {
        parse_hex_color(&self.sidebar_bg).unwrap_or_else(|| self.bg_color())
    }

    pub fn status_bg_color(&self) -> Color {
        parse_hex_color(&self.status_bg).unwrap_or_else(|| self.bg_color())
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

    pub fn hint_fg_color(&self) -> Color {
        parse_hex_color(&self.hint_fg).unwrap_or(Color::Yellow)
    }
}

pub fn parse_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

pub fn format_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Maps 2D grid `(x, y)` in `w x h` to `(r, g, b)`:
/// X = Hue (0..360, with last column grayscale), Y = Brightness/Saturation sweep.
pub fn map_coords_to_rgb(x: usize, y: usize, w: usize, h: usize) -> (u8, u8, u8) {
    let w = w.max(2);
    let h = h.max(2);
    if x + 1 >= w {
        let v = (255.0 * (1.0 - (y as f64) / ((h - 1) as f64))).round() as u8;
        return (v, v, v);
    }
    let hue = ((x as f64) / ((w - 2) as f64)) * 360.0;
    let ny = (y as f64) / ((h - 1) as f64);
    let (sat, val) = if ny <= 0.5 {
        (0.2 + ny * 1.6, 1.0)
    } else {
        (1.0, 1.0 - (ny - 0.5) * 1.7)
    };
    hsv_to_rgb(hue, sat.clamp(0.0, 1.0), val.clamp(0.05, 1.0))
}

/// Maps `(r, g, b)` back to nearest `(x, y)` on the `w x h` 2D map.
pub fn rgb_to_map_coords(r: u8, g: u8, b: u8, w: usize, h: usize) -> (usize, usize) {
    let w = w.max(2);
    let h = h.max(2);
    if r == g && g == b {
        let ny = 1.0 - (r as f64 / 255.0);
        let y = (ny * ((h - 1) as f64)).round() as usize;
        return (w - 1, y.min(h - 1));
    }
    let (hue, sat, val) = rgb_to_hsv(r, g, b);
    let x = ((hue / 360.0) * ((w - 2) as f64)).round() as usize;
    let ny = if val >= 0.99 && sat < 1.0 {
        ((sat - 0.2) / 1.6).clamp(0.0, 0.5)
    } else {
        (0.5 + (1.0 - val) / 1.7).clamp(0.5, 1.0)
    };
    let y = (ny * ((h - 1) as f64)).round() as usize;
    (x.min(w - 2), y.min(h - 1))
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (rp, gp, bp) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((rp + m) * 255.0).round() as u8,
        ((gp + m) * 255.0).round() as u8,
        ((bp + m) * 255.0).round() as u8,
    )
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let d = max - min;
    let hue = if d == 0.0 {
        0.0
    } else if (max - rf).abs() < f64::EPSILON {
        60.0 * (((gf - bf) / d).rem_euclid(6.0))
    } else if (max - gf).abs() < f64::EPSILON {
        60.0 * (((bf - rf) / d) + 2.0)
    } else {
        60.0 * (((rf - gf) / d) + 4.0)
    };
    let sat = if max == 0.0 { 0.0 } else { d / max };
    (hue, sat, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_map_coords_sync_cleanly() {
        assert_eq!(parse_rgb("#222222"), Some((34, 34, 34)));
        let (x, y) = rgb_to_map_coords(34, 34, 34, 24, 8);
        assert_eq!(x, 23);
        assert!(y >= 5);
        let (r, g, b) = map_coords_to_rgb(x, y, 24, 8);
        assert_eq!(r, g);
        assert_eq!(g, b);
    }
}
