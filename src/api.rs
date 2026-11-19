use crate::models::{
    ApiResponse, CreateTaskPayload, HealthData, Project, SimpleCounter, Tag, Task,
};
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SuperProductivityClient {
    base_url: String,
    token: String,
    http: Client,
}

impl SuperProductivityClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            http,
        }
    }

    pub fn update_credentials(&mut self, base_url: impl Into<String>, token: impl Into<String>) {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self.token = token.into();
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn parse_response<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        let parsed: ApiResponse<T> = resp.json().await.context("Invalid JSON from API")?;
        if parsed.ok {
            parsed
                .data
                .ok_or_else(|| anyhow!("Missing data in API response"))
        } else if let Some(err) = parsed.error {
            Err(anyhow!("{}: {}", err.code, err.message))
        } else {
            Err(anyhow!("Unknown API error"))
        }
    }

    pub async fn health(&self) -> Result<HealthData> {
        let resp = self
            .http
            .get(self.url("/health"))
            .send()
            .await
            .context("Failed to reach Super Productivity API")?;
        Self::parse_response(resp).await
    }

    pub async fn get_tasks(&self) -> Result<Vec<Task>> {
        let resp = self
            .http
            .get(self.url("/tasks"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn get_projects(&self) -> Result<Vec<Project>> {
        let resp = self
            .http
            .get(self.url("/projects"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn get_tags(&self) -> Result<Vec<Tag>> {
        let resp = self
            .http
            .get(self.url("/tags"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn get_counters(&self) -> Result<Vec<SimpleCounter>> {
        let resp = self
            .http
            .get(self.url("/counters"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn update_counter(&self, id: &str, count: i64) -> Result<SimpleCounter> {
        let resp = self
            .http
            .patch(self.url(&format!("/counters/{id}")))
            .bearer_auth(&self.token)
            .json(&json!({ "count": count }))
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn create_task(&self, payload: CreateTaskPayload) -> Result<Task> {
        let resp = self
            .http
            .post(self.url("/tasks"))
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn update_task(&self, id: &str, patch: serde_json::Value) -> Result<Task> {
        let resp = self
            .http
            .patch(self.url(&format!("/tasks/{id}")))
            .bearer_auth(&self.token)
            .json(&patch)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn toggle_done(&self, id: &str, is_done: bool) -> Result<Task> {
        self.update_task(id, json!({ "isDone": is_done })).await
    }

    pub async fn rename_task(&self, id: &str, title: &str) -> Result<Task> {
        self.update_task(id, json!({ "title": title })).await
    }

    pub async fn delete_task(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/tasks/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        let parsed: ApiResponse<serde_json::Value> = resp.json().await?;
        if parsed.ok {
            Ok(())
        } else if let Some(err) = parsed.error {
            Err(anyhow!("{}: {}", err.code, err.message))
        } else {
            Err(anyhow!("Failed to delete task"))
        }
    }

    pub async fn rename_project(&self, id: &str, title: &str) -> Result<Project> {
        let resp = self
            .http
            .patch(self.url(&format!("/projects/{id}")))
            .bearer_auth(&self.token)
            .json(&json!({ "title": title }))
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn delete_project(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/projects/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        let parsed: ApiResponse<serde_json::Value> = resp.json().await?;
        if parsed.ok {
            Ok(())
        } else if let Some(err) = parsed.error {
            Err(anyhow!("{}: {}", err.code, err.message))
        } else {
            Err(anyhow!("Failed to delete project"))
        }
    }

    pub async fn rename_tag(&self, id: &str, title: &str) -> Result<Tag> {
        let resp = self
            .http
            .patch(self.url(&format!("/tags/{id}")))
            .bearer_auth(&self.token)
            .json(&json!({ "title": title }))
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn delete_tag(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/tags/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        let parsed: ApiResponse<serde_json::Value> = resp.json().await?;
        if parsed.ok {
            Ok(())
        } else if let Some(err) = parsed.error {
            Err(anyhow!("{}: {}", err.code, err.message))
        } else {
            Err(anyhow!("Failed to delete tag"))
        }
    }

    pub async fn get_current_task(&self) -> Result<Option<Task>> {
        let resp = self
            .http
            .get(self.url("/task-control/current"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        let parsed: ApiResponse<Option<Task>> = resp.json().await?;
        if parsed.ok {
            Ok(parsed.data.flatten())
        } else {
            Ok(None)
        }
    }

    pub async fn create_project(&self, title: &str, color: &str) -> Result<Project> {
        let resp = self
            .http
            .post(self.url("/projects"))
            .bearer_auth(&self.token)
            .json(&json!({ "title": title, "theme": { "primary": color } }))
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn create_tag(&self, title: &str, color: &str) -> Result<Tag> {
        let resp = self
            .http
            .post(self.url("/tags"))
            .bearer_auth(&self.token)
            .json(&json!({ "title": title, "color": color }))
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    pub async fn set_current_task(&self, task_id: Option<&str>) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/task-control/current"))
            .bearer_auth(&self.token)
            .json(&json!({ "taskId": task_id }))
            .send()
            .await?;
        let parsed: ApiResponse<serde_json::Value> = resp.json().await?;
        if parsed.ok {
            Ok(())
        } else if let Some(err) = parsed.error {
            Err(anyhow!("{}: {}", err.code, err.message))
        } else {
            Err(anyhow!("Failed to update current task"))
        }
    }
}
