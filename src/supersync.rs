use crate::models::{CreateTaskPayload, EntityTheme, Project, SimpleCounter, Tag, Task};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, anyhow, bail};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, Local};
use flate2::Compression;
use flate2::write::GzEncoder;
use rand::RngCore;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: u32 = 4;
const SALT_LEN: usize = 16;
const IV_LEN: usize = 12;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const MIN_ARGON2_CIPHERTEXT_LEN: usize = SALT_LEN + IV_LEN + TAG_LEN;
const MAX_APPLIED_OP_IDS: usize = 5000;
const DOWNLOAD_LIMIT: u64 = 500;
const MAX_DOWNLOAD_PAGES: usize = 100;

/// Session cache for Argon2id keys so downloading a batch sharing SP's session
/// salt or uploading multiple ops only runs the 64 MiB KDF once per salt.
#[derive(Default)]
pub struct CryptoCache {
    decrypt_keys: HashMap<(String, [u8; SALT_LEN]), [u8; KEY_LEN]>,
    upload_session: Option<(String, [u8; SALT_LEN], [u8; KEY_LEN])>,
}

fn derive_argon2id_key(password: &str, salt: &[u8; SALT_LEN]) -> Result<[u8; KEY_LEN]> {
    let params = Params::new(65536, 3, 1, Some(KEY_LEN))
        .map_err(|e| anyhow!("Invalid Argon2id params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("Argon2id derivation failed: {e}"))?;
    Ok(key)
}

pub fn encrypt_payload(
    plaintext_json: &str,
    password: &str,
    cache: &Arc<StdMutex<CryptoCache>>,
) -> Result<String> {
    if password.is_empty() {
        bail!("SuperSync encryption password is empty");
    }

    let (salt, key) = {
        let guard = cache.lock().map_err(|_| anyhow!("Crypto cache poisoned"))?;
        if let Some((cached_pw, cached_salt, cached_key)) = &guard.upload_session
            && cached_pw == password
        {
            (*cached_salt, *cached_key)
        } else {
            drop(guard);
            let mut salt = [0u8; SALT_LEN];
            rand::thread_rng().fill_bytes(&mut salt);
            let key = derive_argon2id_key(password, &salt)?;
            let mut guard = cache.lock().map_err(|_| anyhow!("Crypto cache poisoned"))?;
            guard.upload_session = Some((password.to_string(), salt, key));
            (salt, key)
        }
    };

    let mut iv = [0u8; IV_LEN];
    rand::thread_rng().fill_bytes(&mut iv);

    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("Invalid AES-256-GCM key: {e}"))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&iv), plaintext_json.as_bytes())
        .map_err(|e| anyhow!("AES-GCM encryption failed: {e}"))?;

    let mut out = Vec::with_capacity(SALT_LEN + IV_LEN + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(out))
}

pub fn decrypt_payload(
    encrypted_b64: &str,
    password: &str,
    cache: &Arc<StdMutex<CryptoCache>>,
) -> Result<Value> {
    if password.is_empty() {
        bail!("SuperSync encryption password is required to decrypt server data");
    }

    let raw = BASE64
        .decode(encrypted_b64.trim())
        .context("Failed to decode base64 encrypted operation payload")?;
    if raw.len() < MIN_ARGON2_CIPHERTEXT_LEN {
        bail!(
            "Unsupported or corrupt encrypted payload ({} bytes; expected >= {} Argon2id format)",
            raw.len(),
            MIN_ARGON2_CIPHERTEXT_LEN
        );
    }

    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&raw[..SALT_LEN]);
    let iv = &raw[SALT_LEN..SALT_LEN + IV_LEN];
    let ciphertext = &raw[SALT_LEN + IV_LEN..];

    let cache_key = (password.to_string(), salt);
    let key = {
        let guard = cache.lock().map_err(|_| anyhow!("Crypto cache poisoned"))?;
        guard.decrypt_keys.get(&cache_key).copied()
    };
    let key = match key {
        Some(k) => k,
        None => {
            let derived = derive_argon2id_key(password, &salt)?;
            let mut guard = cache.lock().map_err(|_| anyhow!("Crypto cache poisoned"))?;
            if guard.decrypt_keys.len() > 64 {
                guard.decrypt_keys.clear();
            }
            guard.decrypt_keys.insert(cache_key, derived);
            derived
        }
    };

    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("Invalid AES-256-GCM key: {e}"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(iv), ciphertext)
        .map_err(|_| {
            anyhow!("Decryption failed (wrong SuperSync encryption password or corrupt data)")
        })?;

    serde_json::from_slice::<Value>(&plaintext)
        .context("Failed to parse decrypted operation JSON payload")
}

pub type VectorClock = BTreeMap<String, u64>;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockCmp {
    Equal,
    LessThan,
    GreaterThan,
    Concurrent,
}

#[cfg(test)]
pub fn compare_clocks(a: &VectorClock, b: &VectorClock) -> ClockCmp {
    let mut a_gt = false;
    let mut b_gt = false;
    for (k, &av) in a {
        let bv = b.get(k).copied().unwrap_or(0);
        if av > bv {
            a_gt = true;
        } else if bv > av {
            b_gt = true;
        }
        if a_gt && b_gt {
            return ClockCmp::Concurrent;
        }
    }
    for (k, &bv) in b {
        if !a.contains_key(k) && bv > 0 {
            b_gt = true;
        }
        if a_gt && b_gt {
            return ClockCmp::Concurrent;
        }
    }
    match (a_gt, b_gt) {
        (true, false) => ClockCmp::GreaterThan,
        (false, true) => ClockCmp::LessThan,
        (false, false) => ClockCmp::Equal,
        (true, true) => ClockCmp::Concurrent,
    }
}

pub fn merge_clocks(a: &VectorClock, b: &VectorClock) -> VectorClock {
    let mut merged = a.clone();
    for (k, &v) in b {
        let entry = merged.entry(k.clone()).or_insert(0);
        if v > *entry {
            *entry = v;
        }
    }
    merged
}

pub fn increment_clock(clock: &VectorClock, client_id: &str) -> VectorClock {
    let mut next = clock.clone();
    let entry = next.entry(client_id.to_string()).or_insert(0);
    *entry += 1;
    next
}

pub fn generate_client_id() -> String {
    let suffix = Alphanumeric.sample_string(&mut rand::thread_rng(), 10);
    format!("SPT_{suffix}")
}

pub fn generate_entity_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 21)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOperation {
    pub id: String,
    pub client_id: String,
    pub action_type: String,
    pub op_type: String,
    pub entity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_ids: Option<Vec<String>>,
    pub payload: Value,
    pub vector_clock: VectorClock,
    pub timestamp: i64,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_payload_encrypted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_import_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_base_server_seq: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerOperation {
    pub server_seq: u64,
    pub op: WireOperation,
    #[serde(default)]
    #[allow(dead_code)]
    pub received_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOpsResponse {
    pub ops: Vec<ServerOperation>,
    pub has_more: bool,
    pub latest_seq: u64,
    #[serde(default)]
    pub gap_detected: Option<bool>,
    #[serde(default)]
    pub snapshot_vector_clock: Option<VectorClock>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadOpsRequest<'a> {
    ops: &'a [WireOperation],
    client_id: &'a str,
    last_known_server_seq: u64,
    request_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub op_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub server_seq: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub existing_clock: Option<VectorClock>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadOpsResponse {
    pub results: Vec<UploadResult>,
    #[serde(default)]
    pub new_ops: Option<Vec<ServerOperation>>,
    pub latest_seq: u64,
    #[serde(default)]
    pub has_more_piggyback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusResponse {
    pub latest_seq: u64,
    #[allow(dead_code)]
    pub devices_online: u64,
}

#[derive(Debug, Clone)]
pub struct BrowserConnectTicket {
    pub request_id: String,
    pub poll_secret: String,
    pub match_code: String,
    pub login_url: String,
    pub poll_interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserConnectPoll {
    Pending,
    Approved(String),
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOpRecord {
    pub id: String,
    pub action_type: String,
    pub op_type: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub entity_ids: Option<Vec<String>>,
    /// Unencrypted MultiEntityPayload JSON so conflicts/retries can re-sign clocks cleanly.
    pub action_payload: Value,
    pub vector_clock: VectorClock,
    pub timestamp: i64,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperSyncState {
    #[serde(default)]
    pub account_key: String,
    #[serde(default = "generate_client_id")]
    pub client_id: String,
    #[serde(default)]
    pub last_server_seq: u64,
    #[serde(default)]
    pub applied_op_ids: Vec<String>,
    #[serde(default)]
    pub vector_clock: VectorClock,
    #[serde(default)]
    pub entity_clocks: HashMap<String, VectorClock>,
    #[serde(default)]
    pub pending_ops: Vec<PendingOpRecord>,
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
    pub today_task_ids: Vec<String>,
}

impl Default for SuperSyncState {
    fn default() -> Self {
        Self {
            account_key: String::new(),
            client_id: generate_client_id(),
            last_server_seq: 0,
            applied_op_ids: Vec::new(),
            vector_clock: BTreeMap::new(),
            entity_clocks: HashMap::new(),
            pending_ops: Vec::new(),
            tasks: Vec::new(),
            projects: Vec::new(),
            tags: Vec::new(),
            counters: Vec::new(),
            current_task_id: None,
            today_task_ids: Vec::new(),
        }
    }
}

impl SuperSyncState {
    pub fn state_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("super-productivity-tui")
            .join("supersync_state.json")
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<SuperSyncState>(&raw).ok())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn compute_account_key(base_url: &str, token: &str) -> String {
        let normalized = base_url.trim().trim_end_matches('/');
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized.hash(&mut hasher);
        token.trim().hash(&mut hasher);
        format!("{normalized}|{:016x}", hasher.finish())
    }

    pub fn switch_account_if_needed(&mut self, base_url: &str, token: &str) {
        let next_key = Self::compute_account_key(base_url, token);
        if self.account_key != next_key {
            let client_id = if self.client_id.is_empty() {
                generate_client_id()
            } else {
                self.client_id.clone()
            };
            *self = Self {
                account_key: next_key,
                client_id,
                ..Default::default()
            };
        }
    }

    fn record_applied_op(&mut self, op_id: &str) {
        if self.applied_op_ids.iter().any(|id| id == op_id) {
            return;
        }
        self.applied_op_ids.push(op_id.to_string());
        if self.applied_op_ids.len() > MAX_APPLIED_OP_IDS {
            let excess = self.applied_op_ids.len() - MAX_APPLIED_OP_IDS;
            self.applied_op_ids.drain(0..excess);
        }
    }
}

#[derive(Debug, Clone)]
pub struct SuperSyncSnapshot {
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub tags: Vec<Tag>,
    pub counters: Vec<SimpleCounter>,
    pub current_task_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
struct SuperSyncConfig {
    base_url: String,
    token: String,
    encrypt_key: String,
}

struct SuperSyncInner {
    cfg: SuperSyncConfig,
    state: SuperSyncState,
}

pub struct OpWriteGuard {
    counter: Arc<AtomicUsize>,
    notify: Arc<Notify>,
}

impl Drop for OpWriteGuard {
    fn drop(&mut self) {
        if self.counter.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.notify.notify_waiters();
        }
    }
}

#[derive(Clone)]
pub struct SuperSyncClient {
    http: reqwest::Client,
    inner: Arc<Mutex<SuperSyncInner>>,
    crypto: Arc<StdMutex<CryptoCache>>,
    sync_lock: Arc<Mutex<()>>,
    pending_writes: Arc<AtomicUsize>,
    writes_notify: Arc<Notify>,
}

impl SuperSyncClient {
    pub fn new(base_url: &str, token: &str, encrypt_key: &str, persist_load: bool) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(75))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let mut state = if persist_load {
            SuperSyncState::load()
        } else {
            SuperSyncState::default()
        };
        if !token.trim().is_empty() {
            state.switch_account_if_needed(base_url, token);
        }
        Self {
            http,
            inner: Arc::new(Mutex::new(SuperSyncInner {
                cfg: SuperSyncConfig {
                    base_url: normalize_base_url(base_url),
                    token: sanitize_token(token),
                    encrypt_key: encrypt_key.to_string(),
                },
                state,
            })),
            crypto: Arc::new(StdMutex::new(CryptoCache::default())),
            sync_lock: Arc::new(Mutex::new(())),
            pending_writes: Arc::new(AtomicUsize::new(0)),
            writes_notify: Arc::new(Notify::new()),
        }
    }

    pub fn begin_op_write(&self) -> OpWriteGuard {
        self.pending_writes.fetch_add(1, Ordering::SeqCst);
        OpWriteGuard {
            counter: self.pending_writes.clone(),
            notify: self.writes_notify.clone(),
        }
    }

    pub async fn flush_pending_writes(&self) {
        while self.pending_writes.load(Ordering::SeqCst) > 0 {
            self.writes_notify.notified().await;
        }
    }

    pub async fn has_pending_ops(&self) -> bool {
        !self.inner.lock().await.state.pending_ops.is_empty()
    }

    pub async fn update_credentials(&self, base_url: &str, token: &str, encrypt_key: &str) {
        let norm_url = normalize_base_url(base_url);
        let clean_token = sanitize_token(token);
        let mut inner = self.inner.lock().await;
        inner.cfg.base_url = norm_url.clone();
        inner.cfg.token = clean_token.clone();
        inner.cfg.encrypt_key = encrypt_key.to_string();
        if !clean_token.is_empty() {
            inner
                .state
                .switch_account_if_needed(&norm_url, &clean_token);
        }
        #[cfg(not(test))]
        let _ = inner.state.save();
    }

    pub async fn is_configured(&self) -> bool {
        let inner = self.inner.lock().await;
        !inner.cfg.base_url.is_empty()
            && !inner.cfg.token.is_empty()
            && !inner.cfg.encrypt_key.is_empty()
    }

    pub fn try_cached_snapshot(&self) -> Option<SuperSyncSnapshot> {
        self.inner
            .try_lock()
            .ok()
            .map(|inner| snapshot_from_state(&inner.state, String::new()))
    }

    pub async fn set_current_task_local(&self, task_id: Option<String>) {
        let mut inner = self.inner.lock().await;
        inner.state.current_task_id = task_id;
        #[cfg(not(test))]
        let _ = inner.state.save();
    }

    pub async fn status(&self) -> Result<SyncStatusResponse> {
        let (base_url, token) = {
            let inner = self.inner.lock().await;
            if inner.cfg.token.is_empty() {
                bail!("SuperSync token not configured");
            }
            if inner.cfg.encrypt_key.is_empty() {
                bail!("SuperSync encryption password not configured");
            }
            (inner.cfg.base_url.clone(), inner.cfg.token.clone())
        };
        let url = format!("{base_url}/api/sync/status");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Failed to connect to SuperSync server")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("SuperSync status HTTP {status}: {body}");
        }
        resp.json::<SyncStatusResponse>()
            .await
            .context("Invalid SuperSync status response")
    }

    /// Full sync cycle: downloads new ops, decrypts and applies them, then uploads any pending local ops.
    pub async fn sync_cycle(&self) -> Result<SuperSyncSnapshot> {
        let _sync_guard = self.sync_lock.lock().await;
        let (base_url, token, encrypt_key, mut state_snapshot) = {
            let inner = self.inner.lock().await;
            if inner.cfg.base_url.is_empty() {
                bail!("SuperSync URL is empty");
            }
            if inner.cfg.token.is_empty() {
                bail!("SuperSync token missing — paste token or Connect via Browser in :s");
            }
            if inner.cfg.encrypt_key.is_empty() {
                bail!("SuperSync encryption password missing — set SS Password in :s");
            }
            (
                inner.cfg.base_url.clone(),
                inner.cfg.token.clone(),
                inner.cfg.encrypt_key.clone(),
                inner.state.clone(),
            )
        };

        let downloaded = self
            .download_and_apply_into(&base_url, &token, &encrypt_key, &mut state_snapshot, false)
            .await?;

        let uploaded = if !state_snapshot.pending_ops.is_empty() {
            self.upload_pending_into(&base_url, &token, &encrypt_key, &mut state_snapshot)
                .await?
        } else {
            0
        };

        // Commit updated state back safely
        {
            let mut inner = self.inner.lock().await;
            if inner.state.account_key == state_snapshot.account_key
                || inner.state.account_key.is_empty()
            {
                // Preserve any new pending ops added concurrently while sync was in-flight
                let concurrent_tail: Vec<PendingOpRecord> = inner
                    .state
                    .pending_ops
                    .iter()
                    .filter(|op| !state_snapshot.applied_op_ids.contains(&op.id))
                    .filter(|op| !state_snapshot.pending_ops.iter().any(|p| p.id == op.id))
                    .cloned()
                    .collect();
                inner.state = state_snapshot;
                inner.state.pending_ops.extend(concurrent_tail);
                #[cfg(not(test))]
                let _ = inner.state.save();
            }
            let msg = if uploaded > 0 && downloaded > 0 {
                format!("SuperSync: +{downloaded} / ↑{uploaded}")
            } else if uploaded > 0 {
                format!("SuperSync: synced {uploaded} change(s)")
            } else if downloaded > 0 {
                format!("SuperSync: applied {downloaded} remote op(s)")
            } else {
                String::new()
            };
            Ok(snapshot_from_state(&inner.state, msg))
        }
    }

    async fn download_and_apply_into(
        &self,
        base_url: &str,
        token: &str,
        encrypt_key: &str,
        state: &mut SuperSyncState,
        force_from_zero: bool,
    ) -> Result<usize> {
        let mut since_seq = if force_from_zero {
            0
        } else {
            state.last_server_seq
        };
        let mut has_reset_for_gap = false;
        let mut applied_count = 0usize;

        for _page in 0..MAX_DOWNLOAD_PAGES {
            let url = format!(
                "{base_url}/api/sync/ops?sinceSeq={since_seq}&limit={DOWNLOAD_LIMIT}&excludeClient={}",
                state.client_id
            );
            let resp = self
                .http
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .context("SuperSync download request failed")?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    bail!("SuperSync auth failed (HTTP {status}) — update token in :s");
                }
                bail!("SuperSync download HTTP {status}: {text}");
            }
            let dl: DownloadOpsResponse = resp
                .json()
                .await
                .context("Invalid SuperSync download JSON")?;

            if dl.gap_detected.unwrap_or(false) && !has_reset_for_gap {
                has_reset_for_gap = true;
                since_seq = 0;
                state.last_server_seq = 0;
                continue;
            }

            if let Some(snap_clock) = &dl.snapshot_vector_clock {
                state.vector_clock = merge_clocks(&state.vector_clock, snap_clock);
            }

            if dl.ops.is_empty() {
                state.last_server_seq = dl.latest_seq.max(since_seq);
                break;
            }

            let count = self
                .decrypt_and_apply_batch(state, &dl.ops, encrypt_key)
                .await?;
            applied_count += count;

            let page_last_seq = dl.ops.last().map(|o| o.server_seq).unwrap_or(since_seq);
            since_seq = page_last_seq;
            state.last_server_seq = page_last_seq.max(state.last_server_seq);

            if !dl.has_more {
                state.last_server_seq = dl.latest_seq.max(state.last_server_seq);
                break;
            }
        }
        Ok(applied_count)
    }

    async fn decrypt_and_apply_batch(
        &self,
        state: &mut SuperSyncState,
        server_ops: &[ServerOperation],
        encrypt_key: &str,
    ) -> Result<usize> {
        let crypto = self.crypto.clone();
        let pw = encrypt_key.to_string();
        let ops_to_decrypt: Vec<(usize, String)> = server_ops
            .iter()
            .enumerate()
            .filter_map(|(i, s_op)| {
                if s_op.op.is_payload_encrypted.unwrap_or(false) || s_op.op.payload.is_string() {
                    s_op.op.payload.as_str().map(|s| (i, s.to_string()))
                } else {
                    None
                }
            })
            .collect();

        let decrypted_map: HashMap<usize, Value> = if ops_to_decrypt.is_empty() {
            HashMap::new()
        } else {
            tokio::task::spawn_blocking(move || -> Result<HashMap<usize, Value>> {
                let mut map = HashMap::with_capacity(ops_to_decrypt.len());
                for (idx, b64) in ops_to_decrypt {
                    let val = decrypt_payload(&b64, &pw, &crypto)?;
                    map.insert(idx, val);
                }
                Ok(map)
            })
            .await
            .context("Decrypt worker panicked")??
        };

        let mut applied = 0usize;
        for (i, s_op) in server_ops.iter().enumerate() {
            let op = &s_op.op;
            state.vector_clock = merge_clocks(&state.vector_clock, &op.vector_clock);
            record_entity_clocks(state, op);

            if state.applied_op_ids.iter().any(|id| id == &op.id) {
                continue;
            }
            let payload_val = decrypted_map
                .get(&i)
                .cloned()
                .unwrap_or_else(|| op.payload.clone());

            apply_remote_operation(state, op, &payload_val);
            state.record_applied_op(&op.id);
            applied += 1;
        }
        Ok(applied)
    }

    async fn upload_pending_into(
        &self,
        base_url: &str,
        token: &str,
        encrypt_key: &str,
        state: &mut SuperSyncState,
    ) -> Result<usize> {
        let mut total_uploaded = 0usize;
        // Max 2 retry rounds if server returns CONFLICT_* or state-replacement fence
        for _attempt in 0..3 {
            if state.pending_ops.is_empty() {
                break;
            }

            let chunk_len = state.pending_ops.len().min(100);
            let batch: Vec<PendingOpRecord> = state.pending_ops[..chunk_len].to_vec();
            let crypto = self.crypto.clone();
            let pw = encrypt_key.to_string();
            let client_id = state.client_id.clone();

            let wire_ops: Vec<WireOperation> =
                tokio::task::spawn_blocking(move || -> Result<Vec<WireOperation>> {
                    let mut out = Vec::with_capacity(batch.len());
                    for pending in batch {
                        let multi_payload = json!({
                            "actionPayload": pending.action_payload,
                            "entityChanges": []
                        });
                        let encrypted =
                            encrypt_payload(&serde_json::to_string(&multi_payload)?, &pw, &crypto)?;
                        out.push(WireOperation {
                            id: pending.id,
                            client_id: client_id.clone(),
                            action_type: pending.action_type,
                            op_type: pending.op_type,
                            entity_type: pending.entity_type,
                            entity_id: pending.entity_id,
                            entity_ids: pending.entity_ids,
                            payload: Value::String(encrypted),
                            vector_clock: pending.vector_clock,
                            timestamp: pending.timestamp,
                            schema_version: pending.schema_version,
                            is_payload_encrypted: Some(true),
                            sync_import_reason: None,
                            repair_base_server_seq: None,
                        });
                    }
                    Ok(out)
                })
                .await
                .context("Encrypt worker panicked")??;

            let req_body = UploadOpsRequest {
                ops: &wire_ops,
                client_id: &state.client_id,
                last_known_server_seq: state.last_server_seq,
                request_id: Alphanumeric.sample_string(&mut rand::thread_rng(), 24),
            };
            let json_bytes = serde_json::to_vec(&req_body)?;
            let mut gz = GzEncoder::new(Vec::new(), Compression::default());
            gz.write_all(&json_bytes)?;
            let compressed = gz.finish()?;

            let url = format!("{base_url}/api/sync/ops");
            let resp = self
                .http
                .post(&url)
                .bearer_auth(token)
                .header("Content-Type", "application/json")
                .header("Content-Encoding", "gzip")
                .body(compressed)
                .send()
                .await
                .context("SuperSync upload request failed")?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                bail!("SuperSync upload HTTP {status}: {text}");
            }
            let up: UploadOpsResponse =
                resp.json().await.context("Invalid SuperSync upload JSON")?;

            // Apply piggybacked remote ops first if any
            if let Some(new_ops) = &up.new_ops
                && !new_ops.is_empty()
            {
                self.decrypt_and_apply_batch(state, new_ops, encrypt_key)
                    .await?;
            }
            if up.has_more_piggyback.unwrap_or(false) {
                let _ = self
                    .download_and_apply_into(base_url, token, encrypt_key, state, false)
                    .await?;
            } else {
                state.last_server_seq = up.latest_seq.max(state.last_server_seq);
            }

            let mut needs_retry = false;
            let mut done_ids: HashSet<String> = HashSet::new();
            let mut retry_clocks: HashMap<String, VectorClock> = HashMap::new();
            let mut needs_download = false;

            for res in &up.results {
                let code = res.error_code.as_deref().unwrap_or("");
                if res.accepted || code == "DUPLICATE_OPERATION" {
                    done_ids.insert(res.op_id.clone());
                    if res.accepted {
                        total_uploaded += 1;
                    }
                    if let Some(seq) = res.server_seq {
                        state.last_server_seq = seq.max(state.last_server_seq);
                    }
                } else if matches!(code, "CONFLICT_CONCURRENT" | "CONFLICT_SUPERSEDED") {
                    if let Some(existing) = &res.existing_clock {
                        retry_clocks.insert(res.op_id.clone(), existing.clone());
                    }
                    needs_retry = true;
                } else if code == "INTERNAL_ERROR"
                    && res
                        .error
                        .as_deref()
                        .is_some_and(|e| e.contains("full-state replacement"))
                {
                    needs_download = true;
                    needs_retry = true;
                } else {
                    // Terminal validation error — drop so queue doesn't wedge forever
                    done_ids.insert(res.op_id.clone());
                }
            }

            for done in &done_ids {
                state.record_applied_op(done);
            }
            state.pending_ops.retain(|p| !done_ids.contains(&p.id));

            if needs_download {
                self.download_and_apply_into(base_url, token, encrypt_key, state, false)
                    .await?;
            }

            if !retry_clocks.is_empty() {
                let cid = state.client_id.clone();
                let now = chrono::Utc::now().timestamp_millis();
                for p in &mut state.pending_ops {
                    if let Some(srv_clock) = retry_clocks.get(&p.id) {
                        let merged = merge_clocks(&state.vector_clock, srv_clock);
                        let merged = merge_clocks(&merged, &p.vector_clock);
                        let next_clock = increment_clock(&merged, &cid);
                        state.vector_clock = next_clock.clone();
                        p.vector_clock = next_clock;
                        p.timestamp = now;
                    }
                }
            }

            if !needs_retry {
                break;
            }
        }
        Ok(total_uploaded)
    }

    // === Enqueue Local Mutations (Optimistic + Durable Op Generation) ===

    async fn enqueue_op(
        &self,
        action_type: &str,
        op_type: &str,
        entity_type: &str,
        entity_id: Option<String>,
        entity_ids: Option<Vec<String>>,
        action_payload: Value,
    ) {
        let mut inner = self.inner.lock().await;
        let client_id = inner.state.client_id.clone();
        let next_clock = increment_clock(&inner.state.vector_clock, &client_id);
        inner.state.vector_clock = next_clock.clone();

        if let Some(ref eid) = entity_id {
            inner
                .state
                .entity_clocks
                .insert(format!("{entity_type}:{eid}"), next_clock.clone());
        }
        if let Some(ref eids) = entity_ids {
            for eid in eids {
                inner
                    .state
                    .entity_clocks
                    .insert(format!("{entity_type}:{eid}"), next_clock.clone());
            }
        }

        let op = PendingOpRecord {
            id: Uuid::now_v7().to_string(),
            action_type: action_type.to_string(),
            op_type: op_type.to_string(),
            entity_type: entity_type.to_string(),
            entity_id,
            entity_ids,
            action_payload,
            vector_clock: next_clock,
            timestamp: chrono::Utc::now().timestamp_millis(),
            schema_version: CURRENT_SCHEMA_VERSION,
        };
        inner.state.pending_ops.push(op);
        #[cfg(not(test))]
        let _ = inner.state.save();
    }

    pub async fn create_task_op(
        &self,
        id: String,
        payload: CreateTaskPayload,
        to_top: bool,
    ) -> Task {
        let now = chrono::Utc::now().timestamp_millis();
        let task = Task {
            id: id.clone(),
            title: payload.title.clone(),
            is_done: false,
            notes: payload.notes.clone(),
            project_id: payload.project_id.clone().filter(|p| !p.is_empty()),
            parent_id: payload.parent_id.clone(),
            tag_ids: payload
                .tag_ids
                .iter()
                .filter(|t| *t != "TODAY")
                .cloned()
                .collect(),
            due_day: payload.due_day.clone(),
            deadline_day: payload.deadline_day.clone(),
            remind_at: None,
            done_on: None,
            time_estimate: payload.time_estimate,
            time_spent: payload.time_spent,
            repeat_cfg_id: None,
        };

        let mut sp_task = json!({
            "id": task.id,
            "title": task.title,
            "isDone": false,
            "projectId": task.project_id.as_deref().unwrap_or(""),
            "tagIds": task.tag_ids,
            "subTaskIds": [],
            "timeSpentOnDay": {},
            "timeSpent": task.time_spent.unwrap_or(0),
            "timeEstimate": task.time_estimate.unwrap_or(0),
            "created": now,
            "modified": now,
            "attachments": [],
        });
        if let Some(obj) = sp_task.as_object_mut() {
            if let Some(notes) = &task.notes {
                obj.insert("notes".to_string(), Value::String(notes.clone()));
            }
            if let Some(pid) = &task.parent_id {
                obj.insert("parentId".to_string(), Value::String(pid.clone()));
            }
            if let Some(due) = &task.due_day {
                obj.insert("dueDay".to_string(), Value::String(due.clone()));
            }
            if let Some(dl) = &task.deadline_day {
                obj.insert("deadlineDay".to_string(), Value::String(dl.clone()));
            }
        }

        {
            let mut inner = self.inner.lock().await;
            if to_top {
                inner.state.tasks.insert(0, task.clone());
            } else {
                inner.state.tasks.push(task.clone());
            }
        }

        if let Some(parent_id) = &task.parent_id {
            self.enqueue_op(
                "[Task] Add SubTask",
                "CRT",
                "TASK",
                Some(id.clone()),
                None,
                json!({
                    "task": sp_task,
                    "parentId": parent_id,
                    "isIgnoreShortSyntax": true,
                }),
            )
            .await;
        } else {
            let (work_context_id, work_context_type) = if let Some(pid) = &task.project_id {
                (pid.clone(), "PROJECT")
            } else if let Some(tid) = task.tag_ids.first() {
                (tid.clone(), "TAG")
            } else {
                ("TODAY".to_string(), "TAG")
            };
            self.enqueue_op(
                "[Task Shared] addTask",
                "CRT",
                "TASK",
                Some(id.clone()),
                None,
                json!({
                    "task": sp_task,
                    "workContextId": work_context_id,
                    "workContextType": work_context_type,
                    "isAddToBacklog": false,
                    "isAddToBottom": !to_top,
                    "isIgnoreShortSyntax": true,
                }),
            )
            .await;
        }
        task
    }

    pub async fn update_task_op(&self, id: &str, mut patch: Value) -> Option<Task> {
        let now = chrono::Utc::now().timestamp_millis();
        let updated_task = {
            let mut inner = self.inner.lock().await;
            let idx = inner.state.tasks.iter().position(|t| t.id == id)?;
            apply_task_patch(&mut inner.state.tasks[idx], &patch);
            Some(inner.state.tasks[idx].clone())
        }?;

        if let Some(obj) = patch.as_object_mut() {
            if let Some(is_done) = obj.get("isDone").and_then(|v| v.as_bool()) {
                if is_done {
                    if !obj.contains_key("doneOn") {
                        obj.insert("doneOn".to_string(), json!(now));
                    }
                } else {
                    obj.insert("doneOn".to_string(), Value::Null);
                }
            }
            if let Some(Value::Null) = obj.get("projectId") {
                obj.insert("projectId".to_string(), Value::String(String::new()));
            }
            obj.insert("modified".to_string(), json!(now));
        }

        self.enqueue_op(
            "[Task Shared] updateTask",
            "UPD",
            "TASK",
            Some(id.to_string()),
            None,
            json!({
                "task": {
                    "id": id,
                    "changes": patch,
                },
                "isIgnoreShortSyntax": true,
            }),
        )
        .await;

        Some(updated_task)
    }

    pub async fn delete_task_op(&self, id: &str) {
        let removed_info = {
            let mut inner = self.inner.lock().await;
            let main_task = inner.state.tasks.iter().find(|t| t.id == id).cloned();
            let sub_tasks: Vec<Task> = inner
                .state
                .tasks
                .iter()
                .filter(|t| t.parent_id.as_deref() == Some(id))
                .cloned()
                .collect();
            inner
                .state
                .tasks
                .retain(|t| t.id != id && t.parent_id.as_deref() != Some(id));
            main_task.map(|t| (t, sub_tasks))
        };

        let task_json = match removed_info {
            Some((t, subs)) => {
                let sub_ids: Vec<String> = subs.iter().map(|s| s.id.clone()).collect();
                let sub_objs: Vec<Value> =
                    subs.iter().map(|s| spt_task_to_sp_value(s, &[])).collect();
                let mut v = spt_task_to_sp_value(&t, &sub_ids);
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("subTasks".to_string(), Value::Array(sub_objs));
                }
                v
            }
            None => json!({
                "id": id,
                "title": "",
                "isDone": false,
                "projectId": "",
                "tagIds": [],
                "subTaskIds": [],
                "subTasks": [],
                "timeSpentOnDay": {},
                "timeSpent": 0,
                "timeEstimate": 0,
                "created": chrono::Utc::now().timestamp_millis(),
                "attachments": [],
            }),
        };

        self.enqueue_op(
            "[Task Shared] deleteTask",
            "DEL",
            "TASK",
            Some(id.to_string()),
            None,
            json!({ "task": task_json }),
        )
        .await;
    }

    pub async fn create_project_op(&self, id: String, title: &str, color: &str) -> Project {
        let proj = Project {
            id: id.clone(),
            title: title.to_string(),
            theme: Some(EntityTheme {
                primary: Some(color.to_string()),
            }),
            color: None,
        };
        {
            let mut inner = self.inner.lock().await;
            inner.state.projects.push(proj.clone());
        }
        self.enqueue_op(
            "[Project] Add Project",
            "CRT",
            "PROJECT",
            Some(id.clone()),
            None,
            json!({
                "project": {
                    "id": id,
                    "title": title,
                    "isArchived": false,
                    "isHiddenFromMenu": false,
                    "isEnableBacklog": false,
                    "taskIds": [],
                    "backlogTaskIds": [],
                    "noteIds": [],
                    "advancedCfg": {
                        "worklogExportSettings": {
                            "cols": ["DATE", "START", "END", "TIME_CLOCK", "TITLE"],
                            "roundWorkTimeTo": null,
                            "roundStartTimeTo": null,
                            "roundEndTimeTo": null,
                            "separateTasksBy": " | ",
                            "groupBy": "DATE"
                        }
                    },
                    "theme": {
                        "isAutoContrast": true,
                        "isDisableBackgroundTint": false,
                        "primary": color,
                        "huePrimary": "500",
                        "accent": "#ff4081",
                        "hueAccent": "500",
                        "warn": "#e11826",
                        "hueWarn": "500",
                        "backgroundImageDark": null,
                        "backgroundImageLight": null
                    }
                }
            }),
        )
        .await;
        proj
    }

    pub async fn update_project_color_title_op(&self, id: &str, title: &str, color: &str) {
        {
            let mut inner = self.inner.lock().await;
            if let Some(p) = inner.state.projects.iter_mut().find(|p| p.id == id) {
                p.title = title.to_string();
                p.color = Some(color.to_string());
                p.theme = Some(EntityTheme {
                    primary: Some(color.to_string()),
                });
            }
        }
        self.enqueue_op(
            "[Project] Update Project",
            "UPD",
            "PROJECT",
            Some(id.to_string()),
            None,
            json!({
                "project": {
                    "id": id,
                    "changes": {
                        "title": title,
                        "theme": {
                            "primary": color
                        }
                    }
                },
                "isSkipSnack": true
            }),
        )
        .await;
    }

    pub async fn rename_project_op(&self, id: &str, title: &str) {
        {
            let mut inner = self.inner.lock().await;
            if let Some(p) = inner.state.projects.iter_mut().find(|p| p.id == id) {
                p.title = title.to_string();
            }
        }
        self.enqueue_op(
            "[Project] Update Project",
            "UPD",
            "PROJECT",
            Some(id.to_string()),
            None,
            json!({
                "project": {
                    "id": id,
                    "changes": { "title": title }
                },
                "isSkipSnack": true
            }),
        )
        .await;
    }

    pub async fn delete_project_op(&self, project_id: &str) {
        let all_task_ids: Vec<String> = {
            let mut inner = self.inner.lock().await;
            let ids: Vec<String> = inner
                .state
                .tasks
                .iter()
                .filter(|t| t.project_id.as_deref() == Some(project_id))
                .map(|t| t.id.clone())
                .collect();
            inner.state.projects.retain(|p| p.id != project_id);
            inner
                .state
                .tasks
                .retain(|t| t.project_id.as_deref() != Some(project_id));
            ids
        };
        self.enqueue_op(
            "[Task Shared] deleteProject",
            "DEL",
            "PROJECT",
            Some(project_id.to_string()),
            None,
            json!({
                "projectId": project_id,
                "noteIds": [],
                "allTaskIds": all_task_ids,
                "projectDeleteWins": true
            }),
        )
        .await;
    }

    pub async fn create_tag_op(&self, id: String, title: &str, color: &str) -> Tag {
        let now = chrono::Utc::now().timestamp_millis();
        let tag = Tag {
            id: id.clone(),
            title: title.to_string(),
            color: Some(color.to_string()),
            theme: Some(EntityTheme {
                primary: Some(color.to_string()),
            }),
        };
        {
            let mut inner = self.inner.lock().await;
            inner.state.tags.push(tag.clone());
        }
        self.enqueue_op(
            "[Tag] Add Tag",
            "CRT",
            "TAG",
            Some(id.clone()),
            None,
            json!({
                "tag": {
                    "id": id,
                    "title": title,
                    "icon": null,
                    "color": color,
                    "created": now,
                    "taskIds": [],
                    "advancedCfg": {
                        "worklogExportSettings": {
                            "cols": ["DATE", "START", "END", "TIME_CLOCK", "TITLE"],
                            "roundWorkTimeTo": null,
                            "roundStartTimeTo": null,
                            "roundEndTimeTo": null,
                            "separateTasksBy": " | ",
                            "groupBy": "DATE"
                        }
                    },
                    "theme": {
                        "isAutoContrast": true,
                        "isDisableBackgroundTint": false,
                        "primary": color,
                        "huePrimary": "500",
                        "accent": "#ff4081",
                        "hueAccent": "500",
                        "warn": "#e11826",
                        "hueWarn": "500"
                    }
                }
            }),
        )
        .await;
        tag
    }

    pub async fn update_tag_color_title_op(&self, id: &str, title: &str, color: &str) {
        {
            let mut inner = self.inner.lock().await;
            if let Some(t) = inner.state.tags.iter_mut().find(|t| t.id == id) {
                t.title = title.to_string();
                t.color = Some(color.to_string());
                t.theme = Some(EntityTheme {
                    primary: Some(color.to_string()),
                });
            }
        }
        self.enqueue_op(
            "[Tag] Update Tag",
            "UPD",
            "TAG",
            Some(id.to_string()),
            None,
            json!({
                "tag": {
                    "id": id,
                    "changes": {
                        "title": title,
                        "color": color,
                        "theme": {
                            "primary": color
                        }
                    }
                },
                "isSkipSnack": true
            }),
        )
        .await;
    }

    pub async fn rename_tag_op(&self, id: &str, title: &str) {
        {
            let mut inner = self.inner.lock().await;
            if let Some(t) = inner.state.tags.iter_mut().find(|t| t.id == id) {
                t.title = title.to_string();
            }
        }
        self.enqueue_op(
            "[Tag] Update Tag",
            "UPD",
            "TAG",
            Some(id.to_string()),
            None,
            json!({
                "tag": {
                    "id": id,
                    "changes": { "title": title }
                },
                "isSkipSnack": true
            }),
        )
        .await;
    }

    pub async fn delete_tag_op(&self, id: &str) {
        {
            let mut inner = self.inner.lock().await;
            inner.state.tags.retain(|t| t.id != id);
            for t in &mut inner.state.tasks {
                t.tag_ids.retain(|tid| tid != id);
            }
        }
        self.enqueue_op(
            "[Tag] Delete Tag",
            "DEL",
            "TAG",
            Some(id.to_string()),
            None,
            json!({
                "id": id,
                "tagDeleteWins": true
            }),
        )
        .await;
    }

    pub async fn set_counter_today_op(&self, id: &str, new_val: i64) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        {
            let mut inner = self.inner.lock().await;
            if let Some(c) = inner.state.counters.iter_mut().find(|c| c.id == id) {
                c.count = new_val;
            }
        }
        self.enqueue_op(
            "[SimpleCounter] Set SimpleCounter Counter Today",
            "UPD",
            "SIMPLE_COUNTER",
            Some(id.to_string()),
            None,
            json!({
                "id": id,
                "newVal": new_val,
                "today": today,
            }),
        )
        .await;
    }

    // === Browser Connect Login Helper ===

    pub async fn start_browser_connect(&self) -> Result<BrowserConnectTicket> {
        let base_url = {
            let inner = self.inner.lock().await;
            inner.cfg.base_url.clone()
        };
        if base_url.is_empty() {
            bail!("SuperSync URL is empty");
        }
        #[cfg(target_os = "windows")]
        let platform = "windows";
        #[cfg(target_os = "macos")]
        let platform = "mac";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let platform = "linux";

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CreateResp {
            request_id: String,
            poll_secret: String,
            match_code: String,
            poll_interval_ms: Option<u64>,
        }

        let resp = self
            .http
            .post(format!("{base_url}/api/connect/requests"))
            .json(&json!({
                "deviceName": "Super Productivity TUI",
                "platform": platform
            }))
            .send()
            .await
            .context("Failed to initiate SuperSync browser login")?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Browser connect request failed ({status}): {text}");
        }
        let body: CreateResp = resp.json().await?;
        let login_url = format!("{base_url}/?r={}", body.request_id);
        Ok(BrowserConnectTicket {
            request_id: body.request_id,
            poll_secret: body.poll_secret,
            match_code: body.match_code,
            login_url,
            poll_interval_ms: body.poll_interval_ms.unwrap_or(3000),
        })
    }

    pub async fn poll_browser_connect(
        &self,
        ticket: &BrowserConnectTicket,
    ) -> Result<BrowserConnectPoll> {
        let base_url = {
            let inner = self.inner.lock().await;
            inner.cfg.base_url.clone()
        };
        #[derive(Deserialize)]
        struct PollResp {
            status: String,
            #[serde(default)]
            token: Option<String>,
        }
        let resp = self
            .http
            .post(format!(
                "{base_url}/api/connect/requests/{}/poll",
                ticket.request_id
            ))
            .json(&json!({ "pollSecret": ticket.poll_secret }))
            .send()
            .await?;
        let body: PollResp = resp.json().await?;
        Ok(match body.status.as_str() {
            "approved" => match body.token {
                Some(tok) => BrowserConnectPoll::Approved(tok),
                None => BrowserConnectPoll::Expired,
            },
            "denied" => BrowserConnectPoll::Denied,
            "expired" => BrowserConnectPoll::Expired,
            _ => BrowserConnectPoll::Pending,
        })
    }
}

pub fn open_browser_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

fn normalize_base_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "https://sync.super-productivity.com".to_string()
    } else {
        trimmed.to_string()
    }
}

fn sanitize_token(token: &str) -> String {
    token
        .chars()
        .filter(|c| (' '..='~').contains(c))
        .collect::<String>()
        .trim()
        .to_string()
}

fn snapshot_from_state(state: &SuperSyncState, message: String) -> SuperSyncSnapshot {
    SuperSyncSnapshot {
        tasks: state.tasks.clone(),
        projects: state.projects.clone(),
        tags: state.tags.clone(),
        counters: state.counters.clone(),
        current_task_id: state.current_task_id.clone(),
        message,
    }
}

fn record_entity_clocks(state: &mut SuperSyncState, op: &WireOperation) {
    if let Some(eid) = &op.entity_id {
        let key = format!("{}:{eid}", op.entity_type);
        let merged = state
            .entity_clocks
            .get(&key)
            .map(|c| merge_clocks(c, &op.vector_clock))
            .unwrap_or_else(|| op.vector_clock.clone());
        state.entity_clocks.insert(key, merged);
    }
    if let Some(eids) = &op.entity_ids {
        for eid in eids {
            let key = format!("{}:{eid}", op.entity_type);
            let merged = state
                .entity_clocks
                .get(&key)
                .map(|c| merge_clocks(c, &op.vector_clock))
                .unwrap_or_else(|| op.vector_clock.clone());
            state.entity_clocks.insert(key, merged);
        }
    }
}

fn ms_to_local_ymd(ms: i64) -> Option<String> {
    DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

fn sp_value_to_spt_task(v: &Value) -> Option<Task> {
    let id = v.get("id")?.as_str()?.to_string();
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let is_done = v.get("isDone").and_then(|b| b.as_bool()).unwrap_or(false);
    let notes = v
        .get("notes")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let project_id = v
        .get("projectId")
        .and_then(|p| p.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let parent_id = v
        .get("parentId")
        .and_then(|p| p.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let tag_ids = v
        .get("tagIds")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .filter(|tid| *tid != "TODAY")
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let due_day = v
        .get("dueDay")
        .and_then(|d| d.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            v.get("dueWithTime")
                .and_then(|m| m.as_i64())
                .and_then(ms_to_local_ymd)
        });
    let deadline_day = v
        .get("deadlineDay")
        .and_then(|d| d.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            v.get("deadlineWithTime")
                .and_then(|m| m.as_i64())
                .and_then(ms_to_local_ymd)
        });
    let remind_at = v.get("remindAt").and_then(|r| r.as_u64());
    let done_on = v.get("doneOn").and_then(|d| d.as_u64());
    let time_estimate = v.get("timeEstimate").and_then(|t| t.as_u64());
    let time_spent = v.get("timeSpent").and_then(|t| t.as_u64());
    let repeat_cfg_id = v
        .get("repeatCfgId")
        .and_then(|r| r.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    Some(Task {
        id,
        title,
        is_done,
        notes,
        project_id,
        parent_id,
        tag_ids,
        due_day,
        deadline_day,
        remind_at,
        done_on,
        time_estimate,
        time_spent,
        repeat_cfg_id,
    })
}

fn spt_task_to_sp_value(t: &Task, sub_task_ids: &[String]) -> Value {
    let mut obj = json!({
        "id": t.id,
        "title": t.title,
        "isDone": t.is_done,
        "projectId": t.project_id.as_deref().unwrap_or(""),
        "tagIds": t.tag_ids,
        "subTaskIds": sub_task_ids,
        "timeSpentOnDay": {},
        "timeSpent": t.time_spent.unwrap_or(0),
        "timeEstimate": t.time_estimate.unwrap_or(0),
        "created": chrono::Utc::now().timestamp_millis(),
        "attachments": [],
    });
    if let Some(m) = obj.as_object_mut() {
        if let Some(n) = &t.notes {
            m.insert("notes".to_string(), json!(n));
        }
        if let Some(pid) = &t.parent_id {
            m.insert("parentId".to_string(), json!(pid));
        }
        if let Some(due) = &t.due_day {
            m.insert("dueDay".to_string(), json!(due));
        }
        if let Some(dl) = &t.deadline_day {
            m.insert("deadlineDay".to_string(), json!(dl));
        }
        if let Some(r) = t.remind_at {
            m.insert("remindAt".to_string(), json!(r));
        }
        if let Some(d) = t.done_on {
            m.insert("doneOn".to_string(), json!(d));
        }
    }
    obj
}

fn sp_value_to_project(v: &Value) -> Option<Project> {
    if v.get("isArchived").and_then(|b| b.as_bool()) == Some(true) {
        return None;
    }
    if v.get("isDone").and_then(|b| b.as_bool()) == Some(true) {
        return None;
    }
    let id = v.get("id")?.as_str()?.to_string();
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let primary = v
        .get("theme")
        .and_then(|th| th.get("primary"))
        .and_then(|p| p.as_str())
        .map(ToString::to_string);
    Some(Project {
        id,
        title,
        theme: primary.map(|p| EntityTheme { primary: Some(p) }),
        color: None,
    })
}

fn sp_value_to_tag(v: &Value) -> Option<Tag> {
    let id = v.get("id")?.as_str()?.to_string();
    if id == "TODAY" {
        return None;
    }
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let color = v
        .get("color")
        .and_then(|c| c.as_str())
        .map(ToString::to_string);
    let primary = v
        .get("theme")
        .and_then(|th| th.get("primary"))
        .and_then(|p| p.as_str())
        .map(ToString::to_string);
    Some(Tag {
        id,
        title,
        color,
        theme: primary.map(|p| EntityTheme { primary: Some(p) }),
    })
}

fn sp_value_to_counter(v: &Value, today_ymd: &str) -> Option<SimpleCounter> {
    if v.get("isEnabled").and_then(|b| b.as_bool()) == Some(false) {
        return None;
    }
    let id = v.get("id")?.as_str()?.to_string();
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let count = v
        .get("countOnDay")
        .and_then(|m| m.get(today_ymd))
        .and_then(|n| n.as_i64())
        .unwrap_or(0);
    let is_on = v.get("isOn").and_then(|b| b.as_bool()).unwrap_or(false);
    Some(SimpleCounter {
        id,
        title,
        count,
        is_on,
    })
}

fn apply_task_patch(task: &mut Task, changes: &Value) {
    let Some(obj) = changes.as_object() else {
        return;
    };
    if let Some(title) = obj.get("title").and_then(|v| v.as_str()) {
        task.title = title.to_string();
    }
    if let Some(done) = obj.get("isDone").and_then(|v| v.as_bool()) {
        task.is_done = done;
        if !done {
            task.done_on = None;
        }
    }
    if let Some(notes_val) = obj.get("notes") {
        task.notes = notes_val
            .as_str()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    }
    if let Some(pid_val) = obj.get("projectId") {
        task.project_id = pid_val
            .as_str()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    }
    if let Some(parent_val) = obj.get("parentId") {
        task.parent_id = parent_val
            .as_str()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    }
    if let Some(tags) = obj.get("tagIds").and_then(|v| v.as_array()) {
        task.tag_ids = tags
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| *s != "TODAY")
            .map(ToString::to_string)
            .collect();
    }
    if let Some(due_val) = obj.get("dueDay") {
        task.due_day = due_val
            .as_str()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    } else if let Some(ms) = obj.get("dueWithTime").and_then(|v| v.as_i64()) {
        task.due_day = ms_to_local_ymd(ms);
    }
    if let Some(dl_val) = obj.get("deadlineDay") {
        task.deadline_day = dl_val
            .as_str()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    } else if let Some(ms) = obj.get("deadlineWithTime").and_then(|v| v.as_i64()) {
        task.deadline_day = ms_to_local_ymd(ms);
    }
    if let Some(rem) = obj.get("remindAt") {
        task.remind_at = rem.as_u64();
    }
    if let Some(done_on) = obj.get("doneOn") {
        task.done_on = done_on.as_u64();
    }
    if let Some(te) = obj.get("timeEstimate") {
        task.time_estimate = te.as_u64();
    }
    if let Some(ts) = obj.get("timeSpent") {
        task.time_spent = ts.as_u64();
    }
    if let Some(rc) = obj.get("repeatCfgId") {
        task.repeat_cfg_id = rc
            .as_str()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    }
}

fn apply_cleared_fields(mut obj: Value, cleared_fields: Option<&Value>) -> Value {
    if let (Some(map), Some(arr)) = (
        obj.as_object_mut(),
        cleared_fields.and_then(|v| v.as_array()),
    ) {
        for field in arr.iter().filter_map(|f| f.as_str()) {
            if field != "id" {
                map.insert(field.to_string(), Value::Null);
            }
        }
    }
    obj
}

fn extract_action_payload(payload: &Value) -> &Value {
    if payload.get("entityChanges").is_some()
        && let Some(ap) = payload.get("actionPayload")
    {
        return ap;
    }
    payload
}

fn reorder_by_ids<T>(items: &mut Vec<T>, ordered_ids: &[String], id_fn: fn(&T) -> &str) {
    let mut map: HashMap<String, T> = items
        .drain(..)
        .map(|item| (id_fn(&item).to_string(), item))
        .collect();
    for id in ordered_ids {
        if let Some(item) = map.remove(id) {
            items.push(item);
        }
    }
    for (_id, item) in map {
        items.push(item);
    }
}

pub fn apply_remote_operation(state: &mut SuperSyncState, op: &WireOperation, payload: &Value) {
    let today_ymd = Local::now().format("%Y-%m-%d").to_string();

    if matches!(
        op.op_type.as_str(),
        "SYNC_IMPORT" | "BACKUP_IMPORT" | "REPAIR"
    ) {
        let full = payload.get("appDataComplete").unwrap_or(payload);
        apply_full_state_import(state, full, &today_ymd);
        return;
    }

    let action_payload = extract_action_payload(payload);
    let is_lww_replace = payload.get("lwwUpdateMode").and_then(|v| v.as_str()) == Some("replace");
    let cleared_fields = payload.get("clearedFields");

    match op.action_type.as_str() {
        // === Tasks ===
        "[Task Shared] addTask" | "[Task] Add SubTask" => {
            if let Some(task_val) = action_payload.get("task")
                && let Some(task) = sp_value_to_spt_task(task_val)
            {
                let to_bottom = action_payload
                    .get("isAddToBottom")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                state.tasks.retain(|t| t.id != task.id);
                if to_bottom {
                    state.tasks.push(task);
                } else {
                    state.tasks.insert(0, task);
                }
            }
        }
        "[Task Shared] updateTask" | "[Task] Update Task Ui" => {
            if is_lww_replace && let Some(task) = sp_value_to_spt_task(action_payload) {
                if let Some(existing) = state.tasks.iter_mut().find(|t| t.id == task.id) {
                    *existing = task;
                } else {
                    state.tasks.insert(0, task);
                }
                return;
            }
            if let Some(task_obj) = action_payload.get("task")
                && let Some(tid) = task_obj.get("id").and_then(|s| s.as_str())
                && let Some(changes) = task_obj.get("changes")
            {
                let patched = apply_cleared_fields(changes.clone(), cleared_fields);
                if let Some(existing) = state.tasks.iter_mut().find(|t| t.id == tid) {
                    apply_task_patch(existing, &patched);
                }
            } else if let Some(tid) = action_payload
                .get("id")
                .and_then(|s| s.as_str())
                .or(op.entity_id.as_deref())
            {
                // LWW patch fallback on flat entity object
                let patched = apply_cleared_fields(action_payload.clone(), cleared_fields);
                if let Some(existing) = state.tasks.iter_mut().find(|t| t.id == tid) {
                    apply_task_patch(existing, &patched);
                }
            }
        }
        "[Task] Update multiple Tasks (simple)" => {
            if let Some(arr) = action_payload.get("taskUpdates").and_then(|v| v.as_array()) {
                for u in arr {
                    if let (Some(tid), Some(ch)) =
                        (u.get("id").and_then(|s| s.as_str()), u.get("changes"))
                        && let Some(existing) = state.tasks.iter_mut().find(|t| t.id == tid)
                    {
                        apply_task_patch(existing, ch);
                    }
                }
            }
        }
        "[Task Shared] updateTasks" => {
            if let Some(arr) = action_payload.get("tasks").and_then(|v| v.as_array()) {
                for u in arr {
                    if let (Some(tid), Some(ch)) =
                        (u.get("id").and_then(|s| s.as_str()), u.get("changes"))
                        && let Some(existing) = state.tasks.iter_mut().find(|t| t.id == tid)
                    {
                        apply_task_patch(existing, ch);
                    }
                }
            }
        }
        "[Task Shared] deleteTask" => {
            if let Some(task_obj) = action_payload.get("task")
                && let Some(tid) = task_obj.get("id").and_then(|s| s.as_str())
            {
                let mut remove_ids: HashSet<String> = HashSet::new();
                remove_ids.insert(tid.to_string());
                if let Some(subs) = task_obj.get("subTaskIds").and_then(|v| v.as_array()) {
                    for s in subs.iter().filter_map(|v| v.as_str()) {
                        remove_ids.insert(s.to_string());
                    }
                }
                state
                    .tasks
                    .retain(|t| !remove_ids.contains(&t.id) && t.parent_id.as_deref() != Some(tid));
            } else if let Some(tid) = op.entity_id.as_deref() {
                state
                    .tasks
                    .retain(|t| t.id != tid && t.parent_id.as_deref() != Some(tid));
            }
        }
        "[Task Shared] deleteTasks" => {
            let mut remove_ids: HashSet<String> = HashSet::new();
            if let Some(arr) = action_payload.get("taskIds").and_then(|v| v.as_array()) {
                for id in arr.iter().filter_map(|v| v.as_str()) {
                    remove_ids.insert(id.to_string());
                }
            }
            if let Some(arr) = op.entity_ids.as_ref() {
                for id in arr {
                    remove_ids.insert(id.clone());
                }
            }
            state.tasks.retain(|t| {
                !remove_ids.contains(&t.id)
                    && t.parent_id
                        .as_deref()
                        .is_none_or(|pid| !remove_ids.contains(pid))
            });
        }
        "[Task Shared] moveToArchive" => {
            let mut archive_ids: HashSet<String> = HashSet::new();
            if let Some(arr) = action_payload.get("tasks").and_then(|v| v.as_array()) {
                for t in arr {
                    if let Some(id) = t.get("id").and_then(|s| s.as_str()) {
                        archive_ids.insert(id.to_string());
                    }
                    if let Some(subs) = t.get("subTaskIds").and_then(|v| v.as_array()) {
                        for sid in subs.iter().filter_map(|v| v.as_str()) {
                            archive_ids.insert(sid.to_string());
                        }
                    }
                }
            }
            state.tasks.retain(|t| {
                !archive_ids.contains(&t.id)
                    && t.parent_id
                        .as_deref()
                        .is_none_or(|pid| !archive_ids.contains(pid))
            });
        }
        "[Task Shared] restoreTask" => {
            if let Some(t_val) = action_payload.get("task")
                && let Some(task) = sp_value_to_spt_task(t_val)
            {
                state.tasks.retain(|t| t.id != task.id);
                state.tasks.insert(0, task);
            }
            if let Some(subs) = action_payload.get("subTasks").and_then(|v| v.as_array()) {
                for s_val in subs {
                    if let Some(sub) = sp_value_to_spt_task(s_val) {
                        state.tasks.retain(|t| t.id != sub.id);
                        state.tasks.push(sub);
                    }
                }
            }
        }
        "[Task Shared] restoreDeletedTask" => {
            if let Some(map) = action_payload
                .get("deletedTaskEntities")
                .and_then(|v| v.as_object())
            {
                for (_id, t_val) in map {
                    if let Some(task) = sp_value_to_spt_task(t_val) {
                        state.tasks.retain(|t| t.id != task.id);
                        state.tasks.insert(0, task);
                    }
                }
            }
        }
        "[Task Shared] scheduleTaskWithTime" | "[Task Shared] reScheduleTaskWithTime" => {
            if let Some(tid) = action_payload
                .get("task")
                .and_then(|t| t.get("id"))
                .and_then(|s| s.as_str())
                && let Some(due_ms) = action_payload.get("dueWithTime").and_then(|v| v.as_i64())
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                t.due_day = ms_to_local_ymd(due_ms);
                t.remind_at = action_payload.get("remindAt").and_then(|r| r.as_u64());
            }
        }
        "[Task Shared] unscheduleTask" => {
            if let Some(tid) = action_payload.get("id").and_then(|s| s.as_str())
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                let leave = action_payload
                    .get("isLeaveInToday")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                if !leave {
                    t.due_day = None;
                }
                t.remind_at = None;
            }
        }
        "[Task Shared] planTasksForToday" => {
            let day = action_payload
                .get("today")
                .and_then(|s| s.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| today_ymd.clone());
            if let Some(arr) = action_payload.get("taskIds").and_then(|v| v.as_array()) {
                let ids: HashSet<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                for t in &mut state.tasks {
                    if ids.contains(t.id.as_str()) {
                        t.due_day = Some(day.clone());
                    }
                }
            }
        }
        "[Task Shared] removeTasksFromTodayTag" => {
            if let Some(arr) = action_payload.get("taskIds").and_then(|v| v.as_array()) {
                let ids: HashSet<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                for t in &mut state.tasks {
                    if ids.contains(t.id.as_str())
                        && t.due_day.as_deref() == Some(today_ymd.as_str())
                    {
                        t.due_day = None;
                    }
                }
            }
        }
        "[Task Shared] setDeadline" => {
            if let Some(tid) = action_payload.get("taskId").and_then(|s| s.as_str())
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                if let Some(dl) = action_payload.get("deadlineDay").and_then(|s| s.as_str()) {
                    t.deadline_day = Some(dl.to_string());
                } else if let Some(ms) = action_payload
                    .get("deadlineWithTime")
                    .and_then(|n| n.as_i64())
                {
                    t.deadline_day = ms_to_local_ymd(ms);
                }
            }
        }
        "[Task Shared] removeDeadline" => {
            if let Some(tid) = action_payload.get("taskId").and_then(|s| s.as_str())
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                t.deadline_day = None;
            }
        }
        "[Task Shared] addTagToTask" => {
            if let (Some(task_id), Some(tag_id)) = (
                action_payload.get("taskId").and_then(|s| s.as_str()),
                action_payload.get("tagId").and_then(|s| s.as_str()),
            ) && tag_id != "TODAY"
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == task_id)
                && !t.tag_ids.iter().any(|x| x == tag_id)
            {
                t.tag_ids.push(tag_id.to_string());
            }
        }
        "[Task Shared] moveToOtherProject" => {
            if let Some(tid) = action_payload
                .get("task")
                .and_then(|t| t.get("id"))
                .and_then(|s| s.as_str())
            {
                let target = action_payload
                    .get("targetProjectId")
                    .and_then(|s| s.as_str())
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string);
                for t in &mut state.tasks {
                    if t.id == tid || t.parent_id.as_deref() == Some(tid) {
                        t.project_id = target.clone();
                    }
                }
            }
        }
        "[Task Shared] convertToSubTask" => {
            if let (Some(tid), Some(pid)) = (
                action_payload.get("taskId").and_then(|s| s.as_str()),
                action_payload
                    .get("targetParentId")
                    .and_then(|s| s.as_str()),
            ) {
                let parent_proj = state
                    .tasks
                    .iter()
                    .find(|t| t.id == pid)
                    .and_then(|p| p.project_id.clone());
                if let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid) {
                    t.parent_id = Some(pid.to_string());
                    t.due_day = None;
                    t.project_id = parent_proj;
                }
            }
        }
        "[Task Shared] convertToMainTask" => {
            if let Some(tid) = action_payload
                .get("task")
                .and_then(|t| t.get("id"))
                .and_then(|s| s.as_str())
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                t.parent_id = None;
                if let Some(is_done) = action_payload.get("isDone").and_then(|b| b.as_bool()) {
                    t.is_done = is_done;
                }
                if let Some(done_on) = action_payload.get("doneOn").and_then(|n| n.as_u64()) {
                    t.done_on = Some(done_on);
                }
                if action_payload
                    .get("isPlanForToday")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false)
                    && let Some(day) = action_payload.get("today").and_then(|s| s.as_str())
                {
                    t.due_day = Some(day.to_string());
                }
            }
        }
        "[Task Shared] applyShortSyntax" => {
            if let Some(tid) = action_payload
                .get("task")
                .and_then(|t| t.get("id"))
                .and_then(|s| s.as_str())
                && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                if let Some(changes) = action_payload.get("taskChanges") {
                    apply_task_patch(t, changes);
                }
                if let Some(pid) = action_payload
                    .get("targetProjectId")
                    .and_then(|s| s.as_str())
                {
                    t.project_id = if pid.is_empty() {
                        None
                    } else {
                        Some(pid.to_string())
                    };
                }
                if let Some(day) = action_payload
                    .get("schedulingInfo")
                    .and_then(|s| s.get("day"))
                    .and_then(|d| d.as_str())
                {
                    t.due_day = Some(day.to_string());
                }
            }
        }
        "[TimeTracking] Sync time spent" if op.entity_type == "TASK" => {
            if let (Some(tid), Some(dur)) = (
                action_payload.get("taskId").and_then(|s| s.as_str()),
                action_payload.get("duration").and_then(|n| n.as_u64()),
            ) && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                t.time_spent = Some(t.time_spent.unwrap_or(0).saturating_add(dur));
            }
        }
        "[Task] Remove time spent" => {
            if let (Some(tid), Some(dur)) = (
                action_payload.get("id").and_then(|s| s.as_str()),
                action_payload.get("duration").and_then(|n| n.as_u64()),
            ) && let Some(t) = state.tasks.iter_mut().find(|t| t.id == tid)
            {
                t.time_spent = Some(t.time_spent.unwrap_or(0).saturating_sub(dur));
            }
        }

        // === Projects ===
        "[Project] Add Project" => {
            if let Some(p_val) = action_payload.get("project")
                && let Some(proj) = sp_value_to_project(p_val)
                && !state.projects.iter().any(|p| p.id == proj.id)
            {
                state.projects.push(proj);
            }
        }
        "[Project] Update Project" => {
            if is_lww_replace && let Some(proj) = sp_value_to_project(action_payload) {
                if let Some(p) = state.projects.iter_mut().find(|p| p.id == proj.id) {
                    *p = proj;
                } else {
                    state.projects.push(proj);
                }
                return;
            }
            if let Some(p_obj) = action_payload.get("project")
                && let Some(pid) = p_obj.get("id").and_then(|s| s.as_str())
                && let Some(changes) = p_obj.get("changes")
            {
                if changes.get("isArchived").and_then(|b| b.as_bool()) == Some(true) {
                    state.projects.retain(|p| p.id != pid);
                } else if let Some(p) = state.projects.iter_mut().find(|p| p.id == pid) {
                    if let Some(title) = changes.get("title").and_then(|s| s.as_str()) {
                        p.title = title.to_string();
                    }
                    if let Some(primary) = changes
                        .get("theme")
                        .and_then(|th| th.get("primary"))
                        .and_then(|s| s.as_str())
                    {
                        p.theme = Some(EntityTheme {
                            primary: Some(primary.to_string()),
                        });
                    }
                }
            } else if let Some(pid) = action_payload
                .get("id")
                .and_then(|s| s.as_str())
                .or(op.entity_id.as_deref())
                && let Some(p) = state.projects.iter_mut().find(|p| p.id == pid)
                && let Some(title) = action_payload.get("title").and_then(|s| s.as_str())
            {
                p.title = title.to_string();
            }
        }
        "[Project] Archive Project" | "[Project] Complete Project" => {
            if let Some(pid) = action_payload.get("id").and_then(|s| s.as_str()) {
                state.projects.retain(|p| p.id != pid);
            }
        }
        "[Project] Update Project Order" => {
            if let Some(arr) = action_payload.get("ids").and_then(|v| v.as_array()) {
                let ids: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect();
                reorder_by_ids(&mut state.projects, &ids, |p| &p.id);
            }
        }
        "[Task Shared] deleteProject" => {
            if let Some(pid) = action_payload.get("projectId").and_then(|s| s.as_str()) {
                state.projects.retain(|p| p.id != pid);
                state.tasks.retain(|t| t.project_id.as_deref() != Some(pid));
            }
        }

        // === Tags ===
        "[Tag] Add Tag" => {
            if let Some(t_val) = action_payload.get("tag")
                && let Some(tag) = sp_value_to_tag(t_val)
                && !state.tags.iter().any(|t| t.id == tag.id)
            {
                state.tags.push(tag);
            }
        }
        "[Tag] Update Tag" => {
            if is_lww_replace && let Some(tag) = sp_value_to_tag(action_payload) {
                if let Some(t) = state.tags.iter_mut().find(|t| t.id == tag.id) {
                    *t = tag;
                } else {
                    state.tags.push(tag);
                }
                return;
            }
            if let Some(t_obj) = action_payload.get("tag")
                && let Some(tid) = t_obj.get("id").and_then(|s| s.as_str())
                && let Some(changes) = t_obj.get("changes")
            {
                if let Some(t) = state.tags.iter_mut().find(|t| t.id == tid) {
                    if let Some(title) = changes.get("title").and_then(|s| s.as_str()) {
                        t.title = title.to_string();
                    }
                    if let Some(c) = changes.get("color").and_then(|s| s.as_str()) {
                        t.color = Some(c.to_string());
                    }
                    if let Some(primary) = changes
                        .get("theme")
                        .and_then(|th| th.get("primary"))
                        .and_then(|s| s.as_str())
                    {
                        t.theme = Some(EntityTheme {
                            primary: Some(primary.to_string()),
                        });
                    }
                }
            } else if let Some(tid) = action_payload
                .get("id")
                .and_then(|s| s.as_str())
                .or(op.entity_id.as_deref())
                && let Some(t) = state.tags.iter_mut().find(|t| t.id == tid)
                && let Some(title) = action_payload.get("title").and_then(|s| s.as_str())
            {
                t.title = title.to_string();
            }
        }
        "[Tag] Delete Tag" => {
            if let Some(tid) = action_payload.get("id").and_then(|s| s.as_str()) {
                state.tags.retain(|t| t.id != tid);
                for task in &mut state.tasks {
                    task.tag_ids.retain(|id| id != tid);
                }
            }
        }
        "[Tag] Delete multiple Tags" => {
            if let Some(arr) = action_payload.get("ids").and_then(|v| v.as_array()) {
                let ids: HashSet<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                state.tags.retain(|t| !ids.contains(t.id.as_str()));
                for task in &mut state.tasks {
                    task.tag_ids.retain(|id| !ids.contains(id.as_str()));
                }
            }
        }
        "[Tag] Update Tag Order" => {
            if let Some(arr) = action_payload.get("ids").and_then(|v| v.as_array()) {
                let ids: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect();
                reorder_by_ids(&mut state.tags, &ids, |t| &t.id);
            }
        }

        // === Simple Counters ===
        "[SimpleCounter] Add SimpleCounter" => {
            if let Some(c_val) = action_payload.get("simpleCounter")
                && let Some(c) = sp_value_to_counter(c_val, &today_ymd)
                && !state.counters.iter().any(|x| x.id == c.id)
            {
                state.counters.push(c);
            }
        }
        "[SimpleCounter] Update SimpleCounter" => {
            if is_lww_replace && let Some(c) = sp_value_to_counter(action_payload, &today_ymd) {
                if let Some(existing) = state.counters.iter_mut().find(|x| x.id == c.id) {
                    *existing = c;
                } else {
                    state.counters.push(c);
                }
                return;
            }
            if let Some(c_obj) = action_payload.get("simpleCounter")
                && let Some(cid) = c_obj.get("id").and_then(|s| s.as_str())
                && let Some(changes) = c_obj.get("changes")
                && let Some(c) = state.counters.iter_mut().find(|x| x.id == cid)
            {
                if let Some(title) = changes.get("title").and_then(|s| s.as_str()) {
                    c.title = title.to_string();
                }
                if let Some(count) = changes
                    .get("countOnDay")
                    .and_then(|m| m.get(&today_ymd))
                    .and_then(|n| n.as_i64())
                {
                    c.count = count;
                }
            }
        }
        "[SimpleCounter] Delete SimpleCounter" => {
            if let Some(cid) = action_payload.get("id").and_then(|s| s.as_str()) {
                state.counters.retain(|c| c.id != cid);
            }
        }
        "[SimpleCounter] Delete multiple SimpleCounters" => {
            if let Some(arr) = action_payload.get("ids").and_then(|v| v.as_array()) {
                let ids: HashSet<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                state.counters.retain(|c| !ids.contains(c.id.as_str()));
            }
        }
        "[SimpleCounter] Set SimpleCounter Counter Today" => {
            if let (Some(cid), Some(new_val)) = (
                action_payload.get("id").and_then(|s| s.as_str()),
                action_payload.get("newVal").and_then(|n| n.as_i64()),
            ) && let Some(c) = state.counters.iter_mut().find(|x| x.id == cid)
            {
                let date = action_payload.get("today").and_then(|s| s.as_str());
                if date.is_none() || date == Some(today_ymd.as_str()) {
                    c.count = new_val;
                }
            }
        }
        "[Simple Counter] Set SimpleCounter Counter For Date" => {
            if let (Some(cid), Some(date), Some(new_val)) = (
                action_payload.get("id").and_then(|s| s.as_str()),
                action_payload.get("date").and_then(|s| s.as_str()),
                action_payload.get("newVal").and_then(|n| n.as_i64()),
            ) && date == today_ymd
                && let Some(c) = state.counters.iter_mut().find(|x| x.id == cid)
            {
                c.count = new_val;
            }
        }
        "[SimpleCounter] Sync counter time" => {
            if let (Some(cid), Some(date), Some(dur)) = (
                action_payload.get("id").and_then(|s| s.as_str()),
                action_payload.get("date").and_then(|s| s.as_str()),
                action_payload.get("duration").and_then(|n| n.as_i64()),
            ) && date == today_ymd
                && let Some(c) = state.counters.iter_mut().find(|x| x.id == cid)
            {
                c.count = c.count.saturating_add(dur);
            }
        }
        "[SimpleCounter] Update all SimpleCounters" => {
            if let Some(arr) = action_payload.get("items").and_then(|v| v.as_array()) {
                state.counters = arr
                    .iter()
                    .filter_map(|v| sp_value_to_counter(v, &today_ymd))
                    .collect();
            }
        }
        "[SimpleCounter] Update SimpleCounter Order" => {
            if let Some(arr) = action_payload.get("ids").and_then(|v| v.as_array()) {
                let ids: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect();
                reorder_by_ids(&mut state.counters, &ids, |c| &c.id);
            }
        }

        _ => {}
    }
}

fn collect_ordered_entities(slice: &Value) -> Vec<&Value> {
    let Some(entities) = slice.get("entities").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    if let Some(ids) = slice.get("ids").and_then(|v| v.as_array()) {
        for id in ids.iter().filter_map(|v| v.as_str()) {
            if let Some(entity) = entities.get(id) {
                out.push(entity);
                seen.insert(id);
            }
        }
    }
    for (id, entity) in entities {
        if !seen.contains(id.as_str()) {
            out.push(entity);
        }
    }
    out
}

fn apply_full_state_import(state: &mut SuperSyncState, full: &Value, today_ymd: &str) {
    if let Some(task_slice) = full.get("task") {
        state.tasks = collect_ordered_entities(task_slice)
            .into_iter()
            .filter_map(sp_value_to_spt_task)
            .collect();
    }
    if let Some(proj_slice) = full.get("project") {
        state.projects = collect_ordered_entities(proj_slice)
            .into_iter()
            .filter_map(sp_value_to_project)
            .collect();
    }
    if let Some(tag_slice) = full.get("tag") {
        if let Some(today_ids) = tag_slice
            .get("entities")
            .and_then(|e| e.get("TODAY"))
            .and_then(|t| t.get("taskIds"))
            .and_then(|arr| arr.as_array())
        {
            state.today_task_ids = today_ids
                .iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect();
        }
        state.tags = collect_ordered_entities(tag_slice)
            .into_iter()
            .filter_map(sp_value_to_tag)
            .collect();
    }
    if let Some(counter_slice) = full.get("simpleCounter") {
        state.counters = collect_ordered_entities(counter_slice)
            .into_iter()
            .filter_map(|v| sp_value_to_counter(v, today_ymd))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip_matches_wire_layout() {
        let cache = Arc::new(StdMutex::new(CryptoCache::default()));
        let password = "correct-horse-battery-staple";
        let payload = json!({
            "actionPayload": {
                "task": { "id": "t1", "title": "Buy milk" }
            },
            "entityChanges": []
        });
        let raw_json = serde_json::to_string(&payload).unwrap();
        let enc = encrypt_payload(&raw_json, password, &cache).unwrap();

        let decoded_bytes = BASE64.decode(&enc).unwrap();
        assert!(decoded_bytes.len() >= MIN_ARGON2_CIPHERTEXT_LEN);

        // Second encryption uses a different IV (different ciphertext)
        let enc2 = encrypt_payload(&raw_json, password, &cache).unwrap();
        assert_ne!(enc, enc2);

        let dec = decrypt_payload(&enc, password, &cache).unwrap();
        assert_eq!(dec, payload);

        // Wrong password must fail AES-GCM authentication
        let wrong_cache = Arc::new(StdMutex::new(CryptoCache::default()));
        assert!(decrypt_payload(&enc, "wrong-password", &wrong_cache).is_err());
    }

    #[test]
    fn vector_clock_comparison_and_merge() {
        let mut a = VectorClock::new();
        a.insert("c1".to_string(), 2);
        let mut b = VectorClock::new();
        b.insert("c1".to_string(), 2);
        b.insert("c2".to_string(), 1);
        assert_eq!(compare_clocks(&a, &b), ClockCmp::LessThan);
        assert_eq!(compare_clocks(&b, &a), ClockCmp::GreaterThan);

        let mut c = a.clone();
        c.insert("c1".to_string(), 3);
        assert_eq!(compare_clocks(&c, &b), ClockCmp::Concurrent);

        let merged = merge_clocks(&c, &b);
        assert_eq!(merged.get("c1"), Some(&3));
        assert_eq!(merged.get("c2"), Some(&1));
        assert_eq!(compare_clocks(&merged, &b), ClockCmp::GreaterThan);
        assert_eq!(compare_clocks(&merged, &c), ClockCmp::GreaterThan);
    }

    #[test]
    fn full_state_import_and_incremental_ops_fold_accurately() {
        let mut state = SuperSyncState::default();
        let today = Local::now().format("%Y-%m-%d").to_string();

        let import_op = WireOperation {
            id: "op-1".into(),
            client_id: "E_desk".into(),
            action_type: "[SP_ALL] Load(import) all data".into(),
            op_type: "SYNC_IMPORT".into(),
            entity_type: "ALL".into(),
            entity_id: None,
            entity_ids: None,
            payload: Value::Null,
            vector_clock: BTreeMap::from([("E_desk".into(), 1)]),
            timestamp: 1000,
            schema_version: 4,
            is_payload_encrypted: Some(false),
            sync_import_reason: None,
            repair_base_server_seq: None,
        };
        let import_payload = json!({
            "appDataComplete": {
                "task": {
                    "ids": ["t1"],
                    "entities": {
                        "t1": {
                            "id": "t1",
                            "title": "Initial task",
                            "isDone": false,
                            "projectId": "p1",
                            "tagIds": ["tag1"],
                            "dueDay": today
                        }
                    }
                },
                "project": {
                    "ids": ["p1", "p_arch"],
                    "entities": {
                        "p1": { "id": "p1", "title": "Work", "isArchived": false },
                        "p_arch": { "id": "p_arch", "title": "Old", "isArchived": true }
                    }
                },
                "tag": {
                    "ids": ["TODAY", "tag1"],
                    "entities": {
                        "TODAY": { "id": "TODAY", "title": "Today", "taskIds": ["t1"] },
                        "tag1": { "id": "tag1", "title": "Urgent", "color": "#ff0000" }
                    }
                },
                "simpleCounter": {
                    "ids": ["sc1"],
                    "entities": {
                        "sc1": {
                            "id": "sc1",
                            "title": "Coffee",
                            "isEnabled": true,
                            "isOn": false,
                            "countOnDay": { (today.clone()): 2 }
                        }
                    }
                }
            }
        });
        apply_remote_operation(&mut state, &import_op, &import_payload);

        assert_eq!(state.tasks.len(), 1);
        assert_eq!(state.tasks[0].title, "Initial task");
        assert_eq!(state.projects.len(), 1);
        assert_eq!(state.projects[0].id, "p1");
        assert_eq!(state.tags.len(), 1);
        assert_eq!(state.tags[0].id, "tag1");
        assert_eq!(state.counters.len(), 1);
        assert_eq!(state.counters[0].count, 2);

        // Incremental task update (multi-entity wrapper)
        let upd_op = WireOperation {
            id: "op-2".into(),
            client_id: "E_desk".into(),
            action_type: "[Task Shared] updateTask".into(),
            op_type: "UPD".into(),
            entity_type: "TASK".into(),
            entity_id: Some("t1".into()),
            entity_ids: None,
            payload: Value::Null,
            vector_clock: BTreeMap::from([("E_desk".into(), 2)]),
            timestamp: 2000,
            schema_version: 4,
            is_payload_encrypted: Some(false),
            sync_import_reason: None,
            repair_base_server_seq: None,
        };
        let upd_payload = json!({
            "actionPayload": {
                "task": {
                    "id": "t1",
                    "changes": { "isDone": true, "title": "Done task", "dueDay": null }
                }
            },
            "entityChanges": []
        });
        apply_remote_operation(&mut state, &upd_op, &upd_payload);
        assert!(state.tasks[0].is_done);
        assert_eq!(state.tasks[0].title, "Done task");
        assert_eq!(state.tasks[0].due_day, None);

        // Archive removes task
        let arch_op = WireOperation {
            id: "op-3".into(),
            client_id: "E_desk".into(),
            action_type: "[Task Shared] moveToArchive".into(),
            op_type: "UPD".into(),
            entity_type: "TASK".into(),
            entity_id: Some("t1".into()),
            entity_ids: Some(vec!["t1".into()]),
            payload: Value::Null,
            vector_clock: BTreeMap::from([("E_desk".into(), 3)]),
            timestamp: 3000,
            schema_version: 4,
            is_payload_encrypted: Some(false),
            sync_import_reason: None,
            repair_base_server_seq: None,
        };
        apply_remote_operation(
            &mut state,
            &arch_op,
            &json!({ "actionPayload": { "tasks": [{ "id": "t1", "subTaskIds": [] }] }, "entityChanges": [] }),
        );
        assert!(state.tasks.is_empty());
    }
}
