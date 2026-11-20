use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub is_done: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    #[serde(default)]
    pub due_day: Option<String>,
    #[serde(default)]
    pub deadline_day: Option<String>,
    #[serde(default)]
    pub remind_at: Option<u64>,
    #[serde(default)]
    pub done_on: Option<u64>,
    #[serde(default)]
    pub time_estimate: Option<u64>,
    #[serde(default)]
    pub time_spent: Option<u64>,
    #[serde(default)]
    pub repeat_cfg_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTheme {
    #[serde(default)]
    pub primary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub theme: Option<EntityTheme>,
    #[serde(default)]
    pub color: Option<String>,
}

impl Project {
    pub fn display_color(&self) -> Color {
        self.color
            .as_deref()
            .or_else(|| self.theme.as_ref().and_then(|t| t.primary.as_deref()))
            .and_then(parse_hex_color)
            .unwrap_or(Color::Magenta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub theme: Option<EntityTheme>,
}

impl Tag {
    pub fn display_color(&self) -> Color {
        self.color
            .as_deref()
            .or_else(|| self.theme.as_ref().and_then(|t| t.primary.as_deref()))
            .and_then(parse_hex_color)
            .unwrap_or(Color::Green)
    }
}

pub fn parse_hex_color(raw: &str) -> Option<Color> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskPayload {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tag_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_estimate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_spent: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_add_to_top: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthData {
    pub server: String,
    pub renderer_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleCounter {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub is_on: bool,
}
