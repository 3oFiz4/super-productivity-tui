use crate::models::SimpleCounter;
use crate::theme::ThemeConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeybindMode {
    Normal,
    Nvim,
    #[default]
    Both,
}

impl KeybindMode {
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Nvim,
            Self::Nvim => Self::Both,
            Self::Both => Self::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Nvim => "NVIM",
            Self::Both => "Both",
        }
    }

    pub fn is_nvim_active(self) -> bool {
        matches!(self, Self::Nvim | Self::Both)
    }

    pub fn is_normal_active(self) -> bool {
        matches!(self, Self::Normal | Self::Both)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub api_url: String,
    pub api_token: String,
    pub keybind_mode: KeybindMode,
    pub hint_keybind: bool,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub saved_themes: Vec<ThemeConfig>,
    #[serde(default)]
    pub counters: Vec<SimpleCounter>,
    #[serde(default = "default_bridge_port")]
    pub bridge_port: u16,
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,
    #[serde(default = "default_true")]
    pub sound_alerts: bool,
    #[serde(default)]
    pub ignore_update_forever: bool,
    #[serde(default)]
    pub ignored_update_version: Option<String>,
}

fn default_bridge_port() -> u16 {
    3877
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_url: "http://127.0.0.1:3876".to_string(),
            api_token: String::new(),
            keybind_mode: KeybindMode::Both,
            hint_keybind: true,
            theme: ThemeConfig::default(),
            saved_themes: Vec::new(),
            counters: vec![
                SimpleCounter {
                    id: "habit-water".to_string(),
                    title: "Water (glasses)".to_string(),
                    count: 0,
                    is_on: false,
                },
                SimpleCounter {
                    id: "habit-stretch".to_string(),
                    title: "Stretch Breaks".to_string(),
                    count: 0,
                    is_on: false,
                },
            ],
            bridge_port: default_bridge_port(),
            desktop_notifications: true,
            sound_alerts: true,
            ignore_update_forever: false,
            ignored_update_version: None,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("super-productivity-tui").join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        let mut cfg = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| toml::from_str::<AppConfig>(&raw).ok())
            .unwrap_or_default();

        if cfg.api_token.is_empty()
            && let Ok(env_token) = std::env::var("SP_API_TOKEN")
        {
            cfg.api_token = env_token;
        }
        if let Ok(env_url) = std::env::var("SP_API_URL")
            && !env_url.is_empty()
        {
            cfg.api_url = env_url;
        }
        cfg
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}
