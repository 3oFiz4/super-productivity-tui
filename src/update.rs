use crate::config::AppConfig;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const GITHUB_API_LATEST: &str =
    "https://api.github.com/repos/3oFiz4/super-productivity-tui/releases/latest";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CACHE_TTL_SECS: u64 = 6 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UpdateCache {
    checked_at_secs: u64,
    latest: Option<ReleaseInfo>,
}

impl UpdateCache {
    fn path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("super-productivity-tui")
            .join("update_cache.json")
    }

    fn load() -> Self {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, raw);
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let trimmed = v.trim().trim_start_matches(['v', 'V']);
    let core = trimmed.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub fn should_notify(cfg: &AppConfig, rel: &ReleaseInfo) -> bool {
    !cfg.ignore_update_forever
        && is_newer_version(&rel.tag_name, CURRENT_VERSION)
        && cfg.ignored_update_version.as_deref() != Some(rel.tag_name.as_str())
}

pub async fn fetch_latest_release(timeout: Duration) -> Result<ReleaseInfo> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!("spt/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client
        .get(GITHUB_API_LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow!("GitHub API returned {}", resp.status()));
    }
    let info: ReleaseInfo = resp.json().await?;
    Ok(info)
}

pub async fn check_for_update_cached(force: bool) -> Option<ReleaseInfo> {
    if cfg!(test) {
        return None;
    }
    let now = now_secs();
    let cache = UpdateCache::load();
    if !force
        && cache.checked_at_secs > 0
        && now.saturating_sub(cache.checked_at_secs) < CACHE_TTL_SECS
    {
        return cache.latest;
    }
    match fetch_latest_release(Duration::from_millis(1800)).await {
        Ok(rel) => {
            UpdateCache {
                checked_at_secs: now,
                latest: Some(rel.clone()),
            }
            .save();
            Some(rel)
        }
        Err(_) => cache.latest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_tags_correctly() {
        assert!(is_newer_version("v0.2.0", "0.1.0"));
        assert!(is_newer_version("0.1.1", "0.1.0"));
        assert!(!is_newer_version("v0.1.0", "0.1.0"));
        assert!(!is_newer_version("v0.0.9", "0.1.0"));
    }

    #[test]
    fn respects_ignore_until_next_and_ignore_forever() {
        let mut cfg = AppConfig::default();
        let rel = ReleaseInfo {
            tag_name: "v99.0.0".into(),
            name: "v99.0.0".into(),
            body: "Changelog".into(),
            html_url: String::new(),
        };
        assert!(should_notify(&cfg, &rel));

        cfg.ignored_update_version = Some("v99.0.0".into());
        assert!(!should_notify(&cfg, &rel));

        let newer = ReleaseInfo {
            tag_name: "v99.1.0".into(),
            ..rel.clone()
        };
        assert!(should_notify(&cfg, &newer));

        cfg.ignore_update_forever = true;
        assert!(!should_notify(&cfg, &newer));
    }
}
