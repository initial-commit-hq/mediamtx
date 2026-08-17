//! Config hot-reload loop (Phase 1: path registry + conf snapshot).

use std::sync::Arc;
use std::time::Duration;

use rmtx_api::AppState;
use rmtx_conf::{Conf, ConfWatcher};
use rmtx_hooks::rtsp_port_from_address;
use rmtx_path::PathManager;
use rmtx_staticsources::{
    ensure_always_available_shared, spawn_configured_sources, StaticSourceSideTable,
};
use tracing::{info, warn};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Applies a reloaded config: refresh path registry and optional API conf snapshot.
pub fn apply_conf_reload(
    paths: &PathManager,
    conf: &Conf,
    hook_runner: Option<&rmtx_hooks::HookRunner>,
) {
    if let Some(hooks) = hook_runner {
        hooks.set_rtsp_port(rtsp_port_from_address(&conf.rtsp_address));
    }
    info!(
        path_count = conf.paths.len(),
        "config hot reload: refreshing path registry (existing clients unchanged)"
    );
    paths.load_from_conf(conf);
}

/// Polls `watcher` on a ~1s interval until `shutdown` completes.
pub async fn run_conf_reload_loop(
    mut watcher: ConfWatcher,
    paths: Arc<PathManager>,
    hook_runner: Option<Arc<rmtx_hooks::HookRunner>>,
    api_state: Option<AppState>,
    source_side: Arc<StaticSourceSideTable>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) {
    if let Err(e) = watcher.seed() {
        warn!(error = %e, path = %watcher.path().display(), "config hot reload: failed to seed watcher");
    }

    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("config hot reload: stopping");
                break;
            }
            _ = interval.tick() => {
                match watcher.poll() {
                    Ok(Some(conf)) => {
                        info!(
                            path = %watcher.path().display(),
                            path_count = conf.paths.len(),
                            "config hot reload: detected change, applying snapshot"
                        );
                        apply_conf_reload(&paths, &conf, hook_runner.as_deref());
                        ensure_always_available_shared(
                            &conf,
                            Arc::clone(&paths),
                            Arc::clone(&source_side),
                        );
                        let _ = spawn_configured_sources(
                            &conf,
                            Arc::clone(&paths),
                            Arc::clone(&source_side),
                        );
                        if let Some(ref state) = api_state {
                            state.replace_conf(conf).await;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warn!(
                            error = %e,
                            path = %watcher.path().display(),
                            "config hot reload: poll failed"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn reload_loop_picks_up_file_change() {
        let dir = std::env::temp_dir().join(format!("rmtx-reload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mediamtx.yml");

        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "logLevel: info\npaths:\n  cam1:").unwrap();
        drop(file);

        let paths = Arc::new(PathManager::new());
        let conf = rmtx_conf::Conf::load_from_file(&path).unwrap();
        paths.load_from_conf(&conf);

        let state = AppState::for_test(conf);
        let watcher = ConfWatcher::new(&path);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let paths_clone = Arc::clone(&paths);
        let state_clone = state.clone();
        let side = Arc::new(StaticSourceSideTable::new());
        let handle = tokio::spawn(async move {
            run_conf_reload_loop(
                watcher,
                paths_clone,
                None,
                Some(state_clone),
                side,
                shutdown_rx,
            )
            .await;
        });

        // Yield so the reload loop seeds its baseline mtime before we rewrite the file.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(1100)).await;
        std::fs::write(&path, "logLevel: debug\npaths:\n  cam1:\n  cam2:").unwrap();

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if paths.list().iter().any(|p| p.name == "cam2") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("timed out waiting for config reload");

        let list = paths.list();
        assert!(list.iter().any(|p| p.name == "cam1"));
        assert!(list.iter().any(|p| p.name == "cam2"));

        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("reload loop did not stop")
            .unwrap();

        let _ = std::fs::remove_dir_all(dir);
    }
}
