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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    #[default]
    LocalRest,
    SuperSync,
}

impl SyncMode {
    pub fn next(self) -> Self {
        match self {
            Self::LocalRest => Self::SuperSync,
            Self::SuperSync => Self::LocalRest,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LocalRest => "Local REST",
            Self::SuperSync => "SuperSync",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub sync_mode: SyncMode,
    pub api_url: String,
    pub api_token: String,
    #[serde(default = "default_supersync_url")]
    pub supersync_url: String,
    #[serde(default)]
    pub supersync_token: String,
    #[serde(default)]
    pub supersync_encrypt_key: String,
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
    #[serde(default)]
    pub seen_local_notice: bool,
}

fn default_supersync_url() -> String {
    "https://sync.super-productivity.com".to_string()
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
            sync_mode: SyncMode::LocalRest,
            api_url: "http://127.0.0.1:3876".to_string(),
            api_token: String::new(),
            supersync_url: default_supersync_url(),
            supersync_token: String::new(),
            supersync_encrypt_key: String::new(),
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
            seen_local_notice: false,
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
        if let Ok(env_ss_url) = std::env::var("SP_SUPERSYNC_URL")
            && !env_ss_url.is_empty()
        {
            cfg.supersync_url = env_ss_url;
        }
        if cfg.supersync_token.is_empty()
            && let Ok(env_ss_token) = std::env::var("SP_SUPERSYNC_TOKEN")
        {
            cfg.supersync_token = env_ss_token;
        }
        if cfg.supersync_encrypt_key.is_empty()
            && let Ok(env_ss_key) = std::env::var("SP_SUPERSYNC_ENCRYPT_KEY")
        {
            cfg.supersync_encrypt_key = env_ss_key;
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

/// Returns `true` when `SPT_FORCE_NO_SP=1` (or `true`) is set to simulate SP not being installed/running.
pub fn force_no_sp() -> bool {
    std::env::var("SPT_FORCE_NO_SP").is_ok_and(|val| {
        let v = val.trim().to_ascii_lowercase();
        v == "1" || v == "true"
    })
}

/// Returns `true` if a Super Productivity desktop executable is detected on this machine.
/// Developers can set `SPT_FORCE_NO_SP=1` (or `true`) to simulate SP not being installed.
pub fn is_sp_installed() -> bool {
    if force_no_sp() {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            candidates.push(
                base.join("Programs")
                    .join("superProductivity")
                    .join("superProductivity.exe"),
            );
            candidates.push(base.join("superProductivity").join("superProductivity.exe"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(
                PathBuf::from(pf)
                    .join("superProductivity")
                    .join("superProductivity.exe"),
            );
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            candidates.push(
                PathBuf::from(pf86)
                    .join("superProductivity")
                    .join("superProductivity.exe"),
            );
        }
        if candidates.iter().any(|p| p.is_file()) {
            return true;
        }
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                if dir.join("superProductivity.exe").is_file()
                    || dir.join("superproductivity.exe").is_file()
                {
                    return true;
                }
            }
        }
        false
    }

    #[cfg(target_os = "linux")]
    {
        let mut candidates = vec![
            PathBuf::from("/usr/bin/superproductivity"),
            PathBuf::from("/usr/local/bin/superproductivity"),
            PathBuf::from("/opt/superProductivity/superproductivity"),
            PathBuf::from("/var/lib/flatpak/exports/bin/com.super_productivity.SuperProductivity"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            let h = PathBuf::from(home);
            candidates.push(h.join(".local/bin/superproductivity"));
            candidates.push(
                h.join(".local/share/flatpak/exports/bin/com.super_productivity.SuperProductivity"),
            );
        }
        if candidates.iter().any(|p| p.is_file()) {
            return true;
        }
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                if dir.join("superproductivity").is_file() {
                    return true;
                }
            }
        }
        false
    }

    #[cfg(target_os = "macos")]
    {
        let mut candidates = vec![PathBuf::from("/Applications/superProductivity.app")];
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/superProductivity.app"));
        }
        if candidates.iter().any(|p| p.exists()) {
            return true;
        }
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                if dir.join("superproductivity").is_file() {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::is_sp_installed;

    #[test]
    fn sp_detection_respects_force_no_sp_override() {
        // Safe in unit test scope; restore immediately.
        let prev = std::env::var("SPT_FORCE_NO_SP").ok();
        unsafe {
            std::env::set_var("SPT_FORCE_NO_SP", "1");
        }
        assert!(!is_sp_installed());
        unsafe {
            match prev {
                Some(v) => std::env::set_var("SPT_FORCE_NO_SP", v),
                None => std::env::remove_var("SPT_FORCE_NO_SP"),
            }
        }
    }
}
