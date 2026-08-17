//! Thread-safe path registry and runtime state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use rmtx_hooks::HookRunner;

use crate::conf::PathConf;
use crate::record::RecordSupervisor;
use crate::status::{Path, PathSource};

/// Errors returned by [`PathManager`] lookups.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathError {
    #[error("path not found: {0}")]
    NotFound(String),
}

struct RuntimePath {
    conf_name: String,
    available: bool,
    available_time: Option<chrono::DateTime<Utc>>,
    online: bool,
    online_time: Option<chrono::DateTime<Utc>>,
    source: Option<PathSource>,
    reader_count: u64,
    bytes_received: AtomicU64,
    bytes_sent: AtomicU64,
}

impl RuntimePath {
    fn new(conf_name: String) -> Self {
        Self {
            conf_name,
            available: false,
            available_time: None,
            online: false,
            online_time: None,
            source: None,
            reader_count: 0,
            bytes_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
        }
    }

    fn to_status(&self, name: &str, tracks: Vec<String>) -> Path {
        Path {
            name: name.to_owned(),
            conf_name: self.conf_name.clone(),
            source: self.source.clone(),
            ready: self.available,
            ready_time: self.available_time,
            available: self.available,
            available_time: self.available_time,
            online: self.online,
            online_time: self.online_time,
            tracks,
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            readers: Vec::new(),
        }
    }
}

/// Holds configured paths and per-path runtime state.
pub struct PathManager {
    conf: RwLock<HashMap<String, PathConf>>,
    paths: RwLock<HashMap<String, RuntimePath>>,
    hooks: RwLock<Option<Arc<HookRunner>>>,
    record_supervisor: RecordSupervisor,
}

impl Default for PathManager {
    fn default() -> Self {
        Self {
            conf: RwLock::new(HashMap::new()),
            paths: RwLock::new(HashMap::new()),
            hooks: RwLock::new(None),
            record_supervisor: RecordSupervisor::new(),
        }
    }
}

impl PathManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the recording supervisor (Phase 2 stub).
    pub fn record_supervisor(&self) -> &RecordSupervisor {
        &self.record_supervisor
    }

    /// Attaches a shared hook runner used for ready / not-ready transitions.
    pub fn set_hook_runner(&self, hooks: Arc<HookRunner>) {
        *self.hooks.write() = Some(hooks);
    }

    /// Returns runtime status for every known path (sorted by name).
    pub fn list(&self) -> Vec<Path> {
        let paths = self.paths.read();
        let conf = self.conf.read();
        let mut names: Vec<_> = paths.keys().cloned().collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let tracks =
                    self.tracks_for_path(&name, paths.get(&name).unwrap(), conf.get(&name));
                paths.get(&name).unwrap().to_status(&name, tracks)
            })
            .collect()
    }

    /// Returns runtime status for a single path.
    pub fn get(&self, name: &str) -> Result<Path, PathError> {
        let paths = self.paths.read();
        let conf = self.conf.read();
        paths
            .get(name)
            .map(|p| {
                let tracks = self.tracks_for_path(name, p, conf.get(name));
                p.to_status(name, tracks)
            })
            .ok_or_else(|| PathError::NotFound(name.to_owned()))
    }

    fn tracks_for_path(
        &self,
        _name: &str,
        path: &RuntimePath,
        conf: Option<&PathConf>,
    ) -> Vec<String> {
        if !path.available {
            return Vec::new();
        }
        if path.online {
            return Vec::new();
        }
        if let Some(c) = conf {
            let labels: Vec<String> = c
                .always_available_tracks
                .iter()
                .map(|t| t.codec.clone())
                .filter(|codec| !codec.is_empty())
                .collect();
            if !labels.is_empty() {
                return labels;
            }
        }
        vec!["H264".to_owned()]
    }

    /// Returns path configuration when present.
    pub fn path_conf(&self, name: &str) -> Option<PathConf> {
        self.conf.read().get(name).cloned()
    }

    /// Whether the path currently has a real (non-placeholder) source attached.
    pub fn is_online(&self, name: &str) -> bool {
        self.paths.read().get(name).is_some_and(|p| p.online)
    }

    /// Marks an always-available path as serving its placeholder (available but not online).
    pub fn activate_placeholder(&self, name: &str) -> Result<(), PathError> {
        let path_conf = self.conf.read().get(name).cloned();
        let Some(conf) = path_conf else {
            return Err(PathError::NotFound(name.to_owned()));
        };
        if !conf.always_available {
            return Ok(());
        }

        let hook_runner = self.hooks.read().clone();
        let mut paths = self.paths.write();
        let path = paths
            .get_mut(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;

        if path.online {
            path.online = false;
            path.online_time = None;
            path.source = None;
            path.available = true;
            if path.available_time.is_none() {
                path.available_time = Some(Utc::now());
            }
            return Ok(());
        }

        let was_available = path.available;
        path.available = true;
        if path.available_time.is_none() {
            path.available_time = Some(Utc::now());
        }
        path.online = false;
        path.online_time = None;
        path.source = None;
        drop(paths);

        if !was_available {
            if conf.record {
                if let Err(err) = self.record_supervisor.start(name, &conf) {
                    tracing::warn!(path = name, error = %err, "failed starting recorder");
                }
            }
            if let Some(hooks) = hook_runner {
                if !conf.ready_hooks().is_empty() {
                    hooks.on_ready(name, &conf.ready_hooks(), "", None, None);
                }
            }
        }

        Ok(())
    }

    /// Inserts or replaces path configuration and ensures a runtime entry exists.
    pub fn upsert_conf(&self, conf: PathConf) {
        let name = conf.name.clone();
        self.conf.write().insert(name.clone(), conf);
        self.paths
            .write()
            .entry(name.clone())
            .or_insert_with(|| RuntimePath::new(name));
    }

    /// Removes path configuration and runtime state.
    pub fn remove_conf(&self, name: &str) -> bool {
        let was_available = self.paths.read().get(name).is_some_and(|p| p.available);
        let path_conf = self.conf.read().get(name).cloned();

        self.conf.write().remove(name);
        let removed = self.paths.write().remove(name).is_some();

        if removed && was_available {
            if let Some(conf) = path_conf {
                if let Err(err) = self.record_supervisor.stop(name) {
                    tracing::warn!(path = name, error = %err, "failed stopping recorder");
                }
                if let Some(hooks) = self.hooks.read().clone() {
                    hooks.on_not_ready(name, &conf.ready_hooks());
                }
            }
        } else if removed {
            if let Some(hooks) = self.hooks.read().clone() {
                hooks.abort_path(name);
            }
        }

        removed
    }

    /// Marks a path ready or not-ready, optionally recording source metadata.
    pub fn set_ready(
        &self,
        name: &str,
        ready: bool,
        source: Option<PathSource>,
    ) -> Result<(), PathError> {
        self.set_ready_with_query(name, ready, source, "")
    }

    /// Like [`Self::set_ready`] but includes the query string for hook env.
    pub fn set_ready_with_query(
        &self,
        name: &str,
        ready: bool,
        source: Option<PathSource>,
        query: &str,
    ) -> Result<(), PathError> {
        if !ready {
            if self
                .conf
                .read()
                .get(name)
                .is_some_and(|c| c.always_available)
            {
                return self.activate_placeholder(name);
            }
            return self.set_unavailable(name);
        }

        let path_conf = self.conf.read().get(name).cloned();
        let hook_runner = self.hooks.read().clone();

        let mut paths = self.paths.write();
        let path = paths
            .get_mut(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;

        let was_available = path.available;
        path.available = true;
        if path.available_time.is_none() {
            path.available_time = Some(Utc::now());
        }
        path.online = source.is_some();
        if path.online && path.online_time.is_none() {
            path.online_time = Some(Utc::now());
        }
        if !path.online {
            path.online_time = None;
        }
        path.source = source.clone();
        drop(paths);

        if !was_available {
            if let Some(ref conf) = path_conf {
                if conf.record {
                    if let Err(err) = self.record_supervisor.start(name, conf) {
                        tracing::warn!(path = name, error = %err, "failed starting recorder");
                    }
                }
            }

            if let (Some(hooks), Some(conf)) = (hook_runner, path_conf) {
                if !conf.ready_hooks().is_empty() {
                    hooks.on_ready(
                        name,
                        &conf.ready_hooks(),
                        query,
                        source.as_ref().map(|s| s.source_type.as_str()),
                        source.as_ref().map(|s| s.id.as_str()),
                    );
                }
            }
        }

        Ok(())
    }

    fn set_unavailable(&self, name: &str) -> Result<(), PathError> {
        let path_conf = self.conf.read().get(name).cloned();
        let hook_runner = self.hooks.read().clone();

        let mut paths = self.paths.write();
        let path = paths
            .get_mut(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;

        if !path.available {
            path.online = false;
            path.online_time = None;
            path.source = None;
            return Ok(());
        }

        path.available = false;
        path.available_time = None;
        path.online = false;
        path.online_time = None;
        path.source = None;
        drop(paths);

        if let Some(ref conf) = path_conf {
            if conf.record {
                if let Err(err) = self.record_supervisor.stop(name) {
                    tracing::warn!(path = name, error = %err, "failed stopping recorder");
                }
            }
        }

        if let (Some(hooks), Some(conf)) = (hook_runner, path_conf) {
            if !conf.ready_hooks().is_empty() {
                hooks.on_not_ready(name, &conf.ready_hooks());
            }
        }

        Ok(())
    }

    /// Increments the reader count for a path.
    pub fn add_reader(&self, name: &str) -> Result<(), PathError> {
        let mut paths = self.paths.write();
        let path = paths
            .get_mut(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;
        path.reader_count = path.reader_count.saturating_add(1);
        Ok(())
    }

    /// Decrements the reader count for a path.
    pub fn remove_reader(&self, name: &str) -> Result<(), PathError> {
        let mut paths = self.paths.write();
        let path = paths
            .get_mut(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;
        path.reader_count = path.reader_count.saturating_sub(1);
        Ok(())
    }

    /// Adds to the bytes-received counter.
    pub fn add_bytes_received(&self, name: &str, delta: u64) -> Result<(), PathError> {
        let paths = self.paths.read();
        let path = paths
            .get(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;
        path.bytes_received.fetch_add(delta, Ordering::Relaxed);
        Ok(())
    }

    /// Feeds a media payload into the path recorder when `record: yes` is active.
    pub fn record_media_part(&self, name: &str, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if let Err(err) = self.record_supervisor.on_media_part(name, data) {
            tracing::warn!(path = name, error = %err, "failed to append recording part");
        }
    }

    /// Adds to the bytes-sent counter.
    pub fn add_bytes_sent(&self, name: &str, delta: u64) -> Result<(), PathError> {
        let paths = self.paths.read();
        let path = paths
            .get(name)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))?;
        path.bytes_sent.fetch_add(delta, Ordering::Relaxed);
        Ok(())
    }

    /// Returns configured path names.
    pub fn conf_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.conf.read().keys().cloned().collect();
        names.sort();
        names
    }

    /// Returns the reader count for a path (internal metric; not exposed in API yet).
    pub fn reader_count(&self, name: &str) -> Result<u64, PathError> {
        let paths = self.paths.read();
        paths
            .get(name)
            .map(|p| p.reader_count)
            .ok_or_else(|| PathError::NotFound(name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conf::PathConf;

    #[test]
    fn create_and_list_paths() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf::new("live"));
        mgr.upsert_conf(PathConf::new("cam1"));

        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "cam1");
        assert_eq!(list[0].conf_name, "cam1");
        assert!(!list[0].ready);
        assert!(!list[0].available);
        assert!(!list[0].online);
        assert!(list[0].source.is_none());
        assert!(list[0].tracks.is_empty());
        assert!(list[0].readers.is_empty());
    }

    #[test]
    fn get_path() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf::new("live"));

        let path = mgr.get("live").unwrap();
        assert_eq!(path.name, "live");
        assert_eq!(
            mgr.get("missing"),
            Err(PathError::NotFound("missing".into()))
        );
    }

    #[test]
    fn ready_transitions() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf::new("live"));

        mgr.set_ready(
            "live",
            true,
            Some(PathSource {
                source_type: "rtspSession".into(),
                id: "sess-1".into(),
            }),
        )
        .unwrap();

        let ready = mgr.get("live").unwrap();
        assert!(ready.ready);
        assert!(ready.available);
        assert!(ready.online);
        assert!(ready.ready_time.is_some());
        assert!(ready.available_time.is_some());
        assert!(ready.online_time.is_some());
        assert_eq!(ready.source.as_ref().unwrap().source_type, "rtspSession");

        mgr.set_ready("live", false, None).unwrap();
        let not_ready = mgr.get("live").unwrap();
        assert!(!not_ready.ready);
        assert!(!not_ready.available);
        assert!(!not_ready.online);
        assert!(not_ready.ready_time.is_none());
        assert!(not_ready.source.is_none());
    }

    #[test]
    fn always_available_placeholder_reports_available_not_online() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf {
            name: "placeholder".into(),
            always_available: true,
            always_available_tracks: vec![rmtx_conf::AlwaysAvailableTrack {
                codec: "H264".into(),
                ..rmtx_conf::AlwaysAvailableTrack::default()
            }],
            ..PathConf::default()
        });

        mgr.activate_placeholder("placeholder").unwrap();

        let path = mgr.get("placeholder").unwrap();
        assert!(path.ready);
        assert!(path.available);
        assert!(!path.online);
        assert!(path.available_time.is_some());
        assert!(path.online_time.is_none());
        assert!(path.source.is_none());
        assert_eq!(path.tracks, vec!["H264".to_string()]);
    }

    #[test]
    fn always_available_falls_back_to_placeholder_when_source_disconnects() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf {
            name: "placeholder".into(),
            always_available: true,
            ..PathConf::default()
        });

        mgr.activate_placeholder("placeholder").unwrap();
        mgr.set_ready(
            "placeholder",
            true,
            Some(PathSource {
                source_type: "rtspSession".into(),
                id: "s1".into(),
            }),
        )
        .unwrap();

        let online = mgr.get("placeholder").unwrap();
        assert!(online.online);
        assert!(online.source.is_some());

        mgr.set_ready("placeholder", false, None).unwrap();

        let fallback = mgr.get("placeholder").unwrap();
        assert!(fallback.ready);
        assert!(fallback.available);
        assert!(!fallback.online);
        assert!(fallback.source.is_none());
    }

    #[test]
    fn upsert_remove_conf() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf::new("live"));
        assert!(mgr.remove_conf("live"));
        assert!(mgr.get("live").is_err());
        assert!(!mgr.remove_conf("live"));
    }

    #[test]
    fn reader_and_byte_counters() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf::new("live"));

        mgr.add_reader("live").unwrap();
        mgr.add_reader("live").unwrap();
        assert_eq!(mgr.reader_count("live").unwrap(), 2);

        mgr.remove_reader("live").unwrap();
        assert_eq!(mgr.reader_count("live").unwrap(), 1);

        mgr.add_bytes_received("live", 100).unwrap();
        mgr.add_bytes_sent("live", 50).unwrap();

        let path = mgr.get("live").unwrap();
        assert_eq!(path.bytes_received, 100);
        assert_eq!(path.bytes_sent, 50);
    }

    #[test]
    fn path_serializes_to_openapi_shape() {
        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf::new("live"));
        mgr.set_ready(
            "live",
            true,
            Some(PathSource {
                source_type: "rtmpConn".into(),
                id: "id-1".into(),
            }),
        )
        .unwrap();

        let json = serde_json::to_value(mgr.get("live").unwrap()).unwrap();
        assert_eq!(json["name"], "live");
        assert_eq!(json["confName"], "live");
        assert_eq!(json["ready"], true);
        assert_eq!(json["available"], true);
        assert_eq!(json["online"], true);
        assert_eq!(json["source"]["type"], "rtmpConn");
        assert_eq!(json["bytesReceived"], 0);
        assert_eq!(json["bytesSent"], 0);
        assert!(json["tracks"].as_array().unwrap().is_empty());
        assert!(json["readers"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_ready_fires_run_on_ready_hook() {
        use std::fs;
        use std::path::PathBuf;
        use std::sync::Arc;

        fn temp_outfile(prefix: &str) -> PathBuf {
            let dir = std::env::temp_dir()
                .join(format!("rmtx-path-hook-{prefix}-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            dir.join("out.txt")
        }

        let outfile = temp_outfile("ready");
        let _ = fs::remove_file(&outfile);

        let hooks = Arc::new(HookRunner::new("8554"));
        let mgr = PathManager::new();
        mgr.set_hook_runner(Arc::clone(&hooks));
        mgr.upsert_conf(PathConf {
            name: "live".into(),
            run_on_ready: format!("echo $MTX_PATH $MTX_SOURCE_TYPE > {}", outfile.display()),
            run_on_ready_restart: false,
            run_on_not_ready: String::new(),
            ..PathConf::default()
        });

        mgr.set_ready(
            "live",
            true,
            Some(PathSource {
                source_type: "rtspSource".into(),
                id: String::new(),
            }),
        )
        .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Ok(content) = fs::read_to_string(&outfile) {
                    if content.contains("live") && content.contains("rtspSource") {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("timed out waiting for runOnReady hook");
    }

    #[test]
    fn set_ready_starts_recording_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let template = dir
            .path()
            .join("recordings/%path/%Y-%m-%d_%H-%M-%S-%f")
            .to_string_lossy()
            .into_owned();

        let mgr = PathManager::new();
        mgr.upsert_conf(PathConf {
            name: "cam".into(),
            record: true,
            record_path: template,
            record_format: "fmp4".into(),
            ..PathConf::default()
        });

        mgr.set_ready(
            "cam",
            true,
            Some(PathSource {
                source_type: "rtspSession".into(),
                id: "s1".into(),
            }),
        )
        .unwrap();

        let segment = mgr
            .record_supervisor()
            .current_segment("cam")
            .expect("segment path");
        assert!(segment.is_file());
        assert!(segment.to_string_lossy().contains("/recordings/cam/"));
        assert!(segment.extension().is_some_and(|ext| ext == "mp4"));

        mgr.set_ready("cam", false, None).unwrap();
        assert!(mgr.record_supervisor().current_segment("cam").is_none());
    }
}
