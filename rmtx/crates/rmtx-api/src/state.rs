use std::sync::Arc;

use rmtx_conf::Conf;
use rmtx_path::PathManager;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::auth::auth_from_conf;
use crate::config_view;

/// Runtime path information returned by `/v3/paths/*`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub name: String,
    pub conf_name: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_time: Option<String>,
    #[serde(default)]
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_time: Option<String>,
    #[serde(default)]
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathSource>,
    #[serde(default)]
    pub tracks: Vec<String>,
    #[serde(default)]
    pub tracks2: Vec<serde_json::Value>,
    #[serde(default)]
    pub readers: Vec<PathReader>,
    #[serde(default)]
    pub inbound_bytes: u64,
    #[serde(default)]
    pub outbound_bytes: u64,
    #[serde(default)]
    pub inbound_frames_in_error: u64,
    #[serde(default)]
    pub bytes_received: u64,
    #[serde(default)]
    pub bytes_sent: u64,
}

impl PathInfo {
    pub fn new(name: impl Into<String>, conf_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            conf_name: conf_name.into(),
            ready: false,
            ready_time: None,
            available: false,
            available_time: None,
            online: false,
            online_time: None,
            source: None,
            tracks: Vec::new(),
            tracks2: Vec::new(),
            readers: Vec::new(),
            inbound_bytes: 0,
            outbound_bytes: 0,
            inbound_frames_in_error: 0,
            bytes_received: 0,
            bytes_sent: 0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathReader {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

struct Inner {
    conf: Conf,
    paths: Arc<PathManager>,
    auth: rmtx_auth::AuthManager,
    api_auth_enabled: bool,
}

/// Shared application state for the Control API.
#[derive(Clone)]
pub struct AppState {
    version: String,
    started: OffsetDateTime,
    inner: Arc<RwLock<Inner>>,
}

impl AppState {
    pub fn new(version: impl Into<String>, conf: Conf, paths: Arc<PathManager>) -> Self {
        let (auth, api_auth_enabled) = auth_from_conf(&conf);
        Self {
            version: version.into(),
            started: OffsetDateTime::now_utc(),
            inner: Arc::new(RwLock::new(Inner {
                conf,
                paths,
                auth,
                api_auth_enabled,
            })),
        }
    }

    /// Whether Control API requests must pass authentication.
    pub fn api_auth_enabled(&self) -> bool {
        self.inner
            .try_read()
            .map(|g| g.api_auth_enabled)
            .unwrap_or(false)
    }

    pub(crate) fn auth(&self) -> rmtx_auth::AuthManager {
        self.inner
            .try_read()
            .map(|g| g.auth.clone())
            .expect("auth lock poisoned")
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn started(&self) -> OffsetDateTime {
        self.started
    }

    pub fn api_allow_origins(&self) -> Option<Vec<String>> {
        self.inner
            .try_read()
            .ok()
            .and_then(|g| config_view::api_allow_origins(&g.conf))
    }

    pub(crate) async fn paths(&self) -> Vec<PathInfo> {
        let guard = self.inner.read().await;
        guard.paths.list().into_iter().map(path_to_info).collect()
    }

    pub(crate) async fn path(&self, name: &str) -> Option<PathInfo> {
        let guard = self.inner.read().await;
        guard.paths.get(name).ok().map(path_to_info)
    }

    pub(crate) async fn patch_global(
        &self,
        patch: serde_json::Value,
    ) -> Result<(), config_view::ConfigViewError> {
        let mut guard = self.inner.write().await;
        config_view::patch_global(&mut guard.conf, patch)
    }

    pub(crate) async fn patch_path_defaults(
        &self,
        patch: serde_json::Value,
    ) -> Result<(), config_view::ConfigViewError> {
        let mut guard = self.inner.write().await;
        config_view::patch_path_defaults(&mut guard.conf, patch)
    }

    pub(crate) async fn config_path(&self, name: &str) -> Option<serde_json::Value> {
        let guard = self.inner.read().await;
        config_view::path_json(&guard.conf, name)
    }

    pub(crate) async fn config_paths_list(&self) -> Vec<serde_json::Value> {
        let guard = self.inner.read().await;
        config_view::paths_list_json(&guard.conf)
    }

    pub(crate) async fn add_path_config(
        &self,
        name: &str,
        body: serde_json::Value,
    ) -> Result<(), config_view::ConfigViewError> {
        let mut guard = self.inner.write().await;
        config_view::add_path(&mut guard.conf, name, body)?;
        guard.paths.upsert_conf(rmtx_path::PathConf::new(name));
        Ok(())
    }

    pub(crate) async fn patch_path_config(
        &self,
        name: &str,
        patch: serde_json::Value,
    ) -> Result<(), config_view::ConfigViewError> {
        let mut guard = self.inner.write().await;
        config_view::patch_path(&mut guard.conf, name, patch)
    }

    pub(crate) async fn replace_path_config(
        &self,
        name: &str,
        body: serde_json::Value,
    ) -> Result<(), config_view::ConfigViewError> {
        let mut guard = self.inner.write().await;
        config_view::replace_path(&mut guard.conf, name, body)?;
        guard.paths.upsert_conf(rmtx_path::PathConf::new(name));
        Ok(())
    }

    pub(crate) async fn delete_path_config(
        &self,
        name: &str,
    ) -> Result<(), config_view::ConfigViewError> {
        let mut guard = self.inner.write().await;
        config_view::remove_path(&mut guard.conf, name)?;
        guard.paths.remove_conf(name);
        Ok(())
    }

    pub(crate) async fn global_config(&self) -> serde_json::Value {
        let guard = self.inner.read().await;
        config_view::global_json(&guard.conf)
    }

    pub(crate) async fn path_defaults_config(&self) -> serde_json::Value {
        let guard = self.inner.read().await;
        config_view::path_defaults_json(&guard.conf)
    }

    /// Replaces the in-memory config snapshot (e.g. after a hot reload from disk).
    ///
    /// Phase 1: updates the conf held by the Control API only; existing stream
    /// clients and path runtime state are left unchanged.
    pub async fn replace_conf(&self, conf: Conf) {
        let (auth, api_auth_enabled) = auth_from_conf(&conf);
        let mut guard = self.inner.write().await;
        guard.conf = conf;
        guard.auth = auth;
        guard.api_auth_enabled = api_auth_enabled;
    }
}

fn path_to_info(path: rmtx_path::Path) -> PathInfo {
    PathInfo {
        name: path.name,
        conf_name: path.conf_name,
        ready: path.ready,
        ready_time: path.ready_time.map(|t| t.to_rfc3339()),
        available: path.available,
        available_time: path.available_time.map(|t| t.to_rfc3339()),
        online: path.online,
        online_time: path.online_time.map(|t| t.to_rfc3339()),
        source: path.source.map(|s| PathSource {
            kind: s.source_type,
            id: s.id,
        }),
        tracks: path.tracks,
        tracks2: Vec::new(),
        readers: path
            .readers
            .into_iter()
            .map(|r| PathReader {
                kind: r.reader_type,
                id: r.id,
            })
            .collect(),
        inbound_bytes: 0,
        outbound_bytes: 0,
        inbound_frames_in_error: 0,
        bytes_received: path.bytes_received,
        bytes_sent: path.bytes_sent,
    }
}
