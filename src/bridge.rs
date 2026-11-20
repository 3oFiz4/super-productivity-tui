use crate::app::ApiEvent;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BridgeAction {
    CreateProject {
        local_id: String,
        title: String,
        color: String,
    },
    UpdateProject {
        id: String,
        title: Option<String>,
        color: Option<String>,
    },
    DeleteProject {
        id: String,
    },
    CreateTag {
        local_id: String,
        title: String,
        color: String,
    },
    UpdateTag {
        id: String,
        title: Option<String>,
        color: Option<String>,
    },
    DeleteTag {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCommand {
    pub cmd_id: String,
    #[serde(flatten)]
    pub action: BridgeAction,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeAck {
    pub cmd_id: String,
    pub ok: bool,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Default)]
struct BridgeInner {
    queue: VecDeque<BridgeCommand>,
    last_plugin_poll: Option<Instant>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct BridgeHandle {
    inner: Arc<Mutex<BridgeInner>>,
}

impl BridgeHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&self, action: BridgeAction) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.seq += 1;
            let cmd_id = format!("cmd-{}", guard.seq);
            guard.queue.push_back(BridgeCommand { cmd_id, action });
        }
    }

    pub fn is_plugin_active(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.last_plugin_poll)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
    }
}

pub fn spawn_bridge_server(port: u16, bridge: BridgeHandle, tx: mpsc::UnboundedSender<ApiEvent>) {
    if port == 0 {
        return;
    }
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{port}");
        let Ok(listener) = TcpListener::bind(&addr).await else {
            return;
        };
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let b = bridge.clone();
                let t = tx.clone();
                tokio::spawn(async move {
                    handle_conn(stream, b, t).await;
                });
            }
        }
    });
}

async fn handle_conn(
    mut stream: TcpStream,
    bridge: BridgeHandle,
    tx: mpsc::UnboundedSender<ApiEvent>,
) {
    let mut buf = vec![0u8; 16384];
    let Ok(n) = stream.read(&mut buf).await else {
        return;
    };
    if n == 0 {
        return;
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method == "OPTIONS" {
        let _ = write_http_response(&mut stream, 204, "").await;
        return;
    }

    match (method, path) {
        ("GET", "/health") => {
            let _ = write_http_response(&mut stream, 200, r#"{"ok":true,"bridge":"spt"}"#).await;
        }
        ("GET", "/commands") => {
            let cmds: Vec<BridgeCommand> = if let Ok(mut g) = bridge.inner.lock() {
                g.last_plugin_poll = Some(Instant::now());
                g.queue.iter().cloned().collect()
            } else {
                Vec::new()
            };
            let body = serde_json::to_string(&cmds).unwrap_or_else(|_| "[]".to_string());
            let _ = write_http_response(&mut stream, 200, &body).await;
        }
        ("POST", "/commands/ack") => {
            let body_str = req.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            if let Ok(ack) = serde_json::from_str::<BridgeAck>(body_str) {
                let removed = if let Ok(mut g) = bridge.inner.lock() {
                    g.last_plugin_poll = Some(Instant::now());
                    if let Some(pos) = g.queue.iter().position(|c| c.cmd_id == ack.cmd_id) {
                        g.queue.remove(pos)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(cmd) = removed {
                    let _ = tx.send(ApiEvent::BridgeAcked { cmd, ack });
                }
                let _ = write_http_response(&mut stream, 200, r#"{"ok":true}"#).await;
            } else {
                let _ = write_http_response(&mut stream, 400, r#"{"ok":false}"#).await;
            }
        }
        _ => {
            let _ =
                write_http_response(&mut stream, 404, r#"{"ok":false,"error":"not_found"}"#).await;
        }
    }
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bridge_serves_commands_and_processes_ack() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let bridge = BridgeHandle::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<ApiEvent>();

        let b_srv = bridge.clone();
        let t_srv = tx.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    let b = b_srv.clone();
                    let t = t_srv.clone();
                    tokio::spawn(async move {
                        handle_conn(stream, b, t).await;
                    });
                }
            }
        });

        bridge.enqueue(BridgeAction::CreateProject {
            local_id: "local-proj-1".into(),
            title: "Bridge Test".into(),
            color: "#89b4fa".into(),
        });

        let client = reqwest::Client::new();
        let cmds: Vec<BridgeCommand> = client
            .get(format!("http://{addr}/commands"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(bridge.is_plugin_active());

        let ack_resp = client
            .post(format!("http://{addr}/commands/ack"))
            .json(&serde_json::json!({
                "cmd_id": cmds[0].cmd_id,
                "ok": true,
                "entity_id": "sp-proj-999"
            }))
            .send()
            .await
            .unwrap();
        assert!(ack_resp.status().is_success());

        match rx.recv().await {
            Some(ApiEvent::BridgeAcked { cmd, ack }) => {
                assert_eq!(cmd.cmd_id, cmds[0].cmd_id);
                assert_eq!(ack.entity_id.as_deref(), Some("sp-proj-999"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
