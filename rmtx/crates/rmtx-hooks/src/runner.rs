//! Path ready / not-ready hook orchestration.

use std::collections::HashMap;

use parking_lot::Mutex;

use crate::env::{env_on_ready, PathBaseEnv};
use crate::executor::{spawn_hook, HookHandle};

/// Hook command strings for path ready transitions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PathReadyHooks {
    pub run_on_ready: String,
    pub run_on_ready_restart: bool,
    pub run_on_not_ready: String,
}

impl PathReadyHooks {
    pub fn is_empty(&self) -> bool {
        self.run_on_ready.is_empty() && self.run_on_not_ready.is_empty()
    }
}

/// Runs `runOnReady` / `runOnNotReady` for path lifecycle transitions.
#[derive(Default)]
pub struct HookRunner {
    rtsp_port: Mutex<String>,
    ready_handles: Mutex<HashMap<String, HookHandle>>,
    ready_env: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl HookRunner {
    pub fn new(rtsp_port: impl Into<String>) -> Self {
        Self {
            rtsp_port: Mutex::new(rtsp_port.into()),
            ready_handles: Mutex::new(HashMap::new()),
            ready_env: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_rtsp_port(&self, rtsp_port: impl Into<String>) {
        *self.rtsp_port.lock() = rtsp_port.into();
    }

    pub fn rtsp_port(&self) -> String {
        self.rtsp_port.lock().clone()
    }

    /// Starts `runOnReady` when configured. Stores env for a later `on_not_ready`.
    pub fn on_ready(
        &self,
        path_name: &str,
        hooks: &PathReadyHooks,
        query: &str,
        source_type: Option<&str>,
        source_id: Option<&str>,
    ) {
        if hooks.is_empty() {
            return;
        }

        let base = PathBaseEnv::new(path_name, self.rtsp_port());
        let env = env_on_ready(&base, query, source_type, source_id);
        self.ready_env
            .lock()
            .insert(path_name.to_owned(), env.clone());

        if hooks.run_on_ready.is_empty() {
            return;
        }

        tracing::info!(path = path_name, "runOnReady command started");
        let handle = spawn_hook(&hooks.run_on_ready, hooks.run_on_ready_restart, env);
        self.ready_handles
            .lock()
            .insert(path_name.to_owned(), handle);
    }

    /// Stops `runOnReady` and launches `runOnNotReady` when configured.
    pub fn on_not_ready(&self, path_name: &str, hooks: &PathReadyHooks) {
        if let Some(handle) = self.ready_handles.lock().remove(path_name) {
            handle.abort();
            tracing::info!(path = path_name, "runOnReady command stopped");
        }

        let env = self.ready_env.lock().remove(path_name).unwrap_or_else(|| {
            let base = PathBaseEnv::new(path_name, self.rtsp_port());
            env_on_ready(&base, "", None, None)
        });

        if hooks.run_on_not_ready.is_empty() {
            return;
        }

        tracing::info!(path = path_name, "runOnNotReady command launched");
        spawn_hook(&hooks.run_on_not_ready, false, env);
    }

    /// Aborts any in-flight ready hook without launching `runOnNotReady`.
    pub fn abort_path(&self, path_name: &str) {
        if let Some(handle) = self.ready_handles.lock().remove(path_name) {
            handle.abort();
        }
        self.ready_env.lock().remove(path_name);
    }
}

/// Extracts the port from a listen address (`:8554` → `8554`, `host:8554` → `8554`).
pub fn rtsp_port_from_address(addr: &str) -> String {
    match addr.rsplit_once(':') {
        Some((_, port)) => port.to_string(),
        None => addr.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_outfile(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rmtx-runner-{prefix}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("out.txt")
    }

    #[test]
    fn rtsp_port_from_address_parses_common_forms() {
        assert_eq!(rtsp_port_from_address(":8554"), "8554");
        assert_eq!(rtsp_port_from_address("0.0.0.0:8554"), "8554");
        assert_eq!(rtsp_port_from_address("8554"), "8554");
    }

    #[test]
    fn on_ready_builds_env_with_source() {
        let runner = HookRunner::new("8554");
        let hooks = PathReadyHooks {
            run_on_ready: String::new(),
            run_on_ready_restart: false,
            run_on_not_ready: "echo".into(),
        };
        runner.on_ready("live", &hooks, "q=1", Some("rtspSession"), Some("s1"));
        let env = runner.ready_env.lock().get("live").cloned().unwrap();
        assert_eq!(env["MTX_PATH"], "live");
        assert_eq!(env["MTX_QUERY"], "q=1");
        assert_eq!(env["MTX_SOURCE_TYPE"], "rtspSession");
        assert_eq!(env["MTX_SOURCE_ID"], "s1");
        assert_eq!(env["RTSP_PORT"], "8554");
    }

    #[tokio::test]
    async fn on_ready_spawns_command() {
        let outfile = temp_outfile("ready");
        let _ = fs::remove_file(&outfile);

        let runner = Arc::new(HookRunner::new("8554"));
        let hooks = PathReadyHooks {
            run_on_ready: format!("echo ready > {}", outfile.display()),
            run_on_ready_restart: false,
            run_on_not_ready: String::new(),
        };

        runner.on_ready("live", &hooks, "", Some("rtspSource"), Some(""));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let content = fs::read_to_string(&outfile).expect("hook output");
        assert!(content.contains("ready"));
    }

    #[tokio::test]
    async fn on_not_ready_aborts_ready_and_spawns() {
        let ready_file = temp_outfile("ready-abort");
        let not_ready_file = temp_outfile("not-ready");
        let _ = fs::remove_file(&ready_file);
        let _ = fs::remove_file(&not_ready_file);

        let runner = Arc::new(HookRunner::new("8554"));
        let hooks = PathReadyHooks {
            run_on_ready: format!("sleep 30 && echo ready > {}", ready_file.display()),
            run_on_ready_restart: false,
            run_on_not_ready: format!("echo not-ready > {}", not_ready_file.display()),
        };

        runner.on_ready("live", &hooks, "q=1", Some("rtspSource"), Some("id-1"));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        runner.on_not_ready("live", &hooks);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert!(
            !ready_file.exists()
                || fs::read_to_string(&ready_file)
                    .unwrap_or_default()
                    .is_empty()
        );
        let content = fs::read_to_string(&not_ready_file).expect("not-ready output");
        assert!(content.contains("not-ready"));
    }
}
