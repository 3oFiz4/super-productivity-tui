use crate::models::{CreateTaskPayload, Project, SimpleCounter, Tag, Task};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum QueuedOp {
    CreateTask {
        temp_id: String,
        payload: CreateTaskPayload,
        to_top: bool,
    },
    UpdateTask {
        id: String,
        patch: serde_json::Value,
    },
    DeleteTask {
        id: String,
    },
    SetCurrentTask {
        task_id: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfflineCache {
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub counters: Vec<SimpleCounter>,
    #[serde(default)]
    pub current_task_id: Option<String>,
    #[serde(default)]
    pub pending_ops: Vec<QueuedOp>,
    #[serde(default)]
    pub local_projects: Vec<Project>,
    #[serde(default)]
    pub local_tags: Vec<Tag>,
    #[serde(default)]
    pub project_id_map: HashMap<String, String>,
    #[serde(default)]
    pub tag_id_map: HashMap<String, String>,
    #[serde(default)]
    pub project_order: Vec<String>,
    #[serde(default)]
    pub tag_order: Vec<String>,
}

impl OfflineCache {
    #[allow(dead_code)]
    pub fn cache_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("super-productivity-tui").join("cache.json")
    }

    #[allow(dead_code)]
    pub fn load() -> Self {
        let path = Self::cache_path();
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<OfflineCache>(&raw).ok())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = Self::cache_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

pub fn sanitize_create_payload_for_sp(
    mut payload: CreateTaskPayload,
    task_id_map: &HashMap<String, String>,
    project_id_map: &HashMap<String, String>,
    tag_id_map: &HashMap<String, String>,
) -> CreateTaskPayload {
    if let Some(pid) = &payload.parent_id
        && let Some(real) = task_id_map.get(pid)
    {
        payload.parent_id = Some(real.clone());
    }
    if let Some(proj) = &payload.project_id {
        if let Some(real) = project_id_map.get(proj) {
            payload.project_id = Some(real.clone());
        } else if proj.starts_with("local-proj-") {
            payload.project_id = None;
        }
    }
    payload.tag_ids = payload
        .tag_ids
        .into_iter()
        .filter_map(|tid| {
            if let Some(real) = tag_id_map.get(&tid) {
                Some(real.clone())
            } else if tid.starts_with("local-tag-") {
                None
            } else {
                Some(tid)
            }
        })
        .collect();
    payload
}

pub fn sanitize_patch_for_sp(
    mut patch: serde_json::Value,
    project_id_map: &HashMap<String, String>,
    tag_id_map: &HashMap<String, String>,
) -> serde_json::Value {
    if let Some(obj) = patch.as_object_mut() {
        if let Some(proj_val) = obj.get("projectId").and_then(|v| v.as_str()) {
            if let Some(real) = project_id_map.get(proj_val) {
                obj.insert(
                    "projectId".to_string(),
                    serde_json::Value::String(real.clone()),
                );
            } else if proj_val.starts_with("local-proj-") {
                obj.remove("projectId");
            }
        }
        if let Some(tags) = obj.get("tagIds").and_then(|v| v.as_array()) {
            let filtered: Vec<serde_json::Value> = tags
                .iter()
                .filter_map(|v| v.as_str())
                .filter_map(|tid| {
                    if let Some(real) = tag_id_map.get(tid) {
                        Some(serde_json::Value::String(real.clone()))
                    } else if tid.starts_with("local-tag-") {
                        None
                    } else {
                        Some(serde_json::Value::String(tid.to_string()))
                    }
                })
                .collect();
            obj.insert("tagIds".to_string(), serde_json::Value::Array(filtered));
        }
    }
    patch
}

pub fn apply_order<T, F>(items: &mut [T], order: &[String], id_fn: F)
where
    F: Fn(&T) -> &str,
{
    if order.is_empty() {
        return;
    }
    let pos_map: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    items.sort_by_key(|item| pos_map.get(id_fn(item)).copied().unwrap_or(usize::MAX));
}
