//! Always-available placeholder activation at startup / reload.

use std::sync::Arc;

use rmtx_conf::Conf;
use rmtx_path::PathManager;
use rmtx_stream::placeholder_from_tracks;
use tracing::info;

use crate::side_table::StaticSourceSideTable;

/// Ensures each configured always-available path has a placeholder stream when offline.
pub fn ensure_always_available(conf: &Conf, paths: Arc<PathManager>, side: &StaticSourceSideTable) {
    for (name, path_conf) in &conf.paths {
        let snapshot = rmtx_path::PathConf::from_conf_entry(name, &conf.path_defaults, path_conf);
        if !snapshot.always_available {
            continue;
        }
        if paths.is_online(name) {
            continue;
        }

        let stream = placeholder_from_tracks(&snapshot.always_available_tracks);
        let stream = side.attach_placeholder(name, stream);
        if let Err(err) = paths.activate_placeholder(name) {
            tracing::warn!(path = %name, error = %err, "failed to activate always-available placeholder");
            continue;
        }
        side.maybe_start_record_tap(name, stream, Arc::clone(&paths));
        info!(path = %name, "always-available placeholder active");
    }
}

/// Like [`ensure_always_available`] but accepts shared handles for spawn sites.
pub fn ensure_always_available_shared(
    conf: &Conf,
    paths: Arc<PathManager>,
    side: Arc<StaticSourceSideTable>,
) {
    ensure_always_available(conf, paths, &side);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use rmtx_conf::{AlwaysAvailableTrack, Path};

    #[tokio::test]
    async fn placeholder_stream_emits_media_units() {
        let paths = Arc::new(PathManager::new());
        let side = StaticSourceSideTable::new();
        let mut conf = rmtx_conf::Conf::default();
        conf.paths.insert(
            "placeholder".into(),
            Path {
                always_available: true,
                always_available_tracks: vec![AlwaysAvailableTrack {
                    codec: "H264".into(),
                    ..AlwaysAvailableTrack::default()
                }],
                ..Path::default()
            },
        );
        paths.load_from_conf(&conf);

        ensure_always_available(&conf, Arc::clone(&paths), &side);

        let stream = side.stream("placeholder").expect("placeholder stream");
        let mut sub = stream.subscribe();

        tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("timed out waiting for placeholder media unit")
            .expect("stream bus closed");
    }

    #[tokio::test]
    async fn activates_placeholder_for_always_available_path() {
        let paths = Arc::new(PathManager::new());
        let side = StaticSourceSideTable::new();
        let mut conf = rmtx_conf::Conf::default();
        conf.paths.insert(
            "placeholder".into(),
            Path {
                always_available: true,
                always_available_tracks: vec![AlwaysAvailableTrack {
                    codec: "H264".into(),
                    ..AlwaysAvailableTrack::default()
                }],
                ..Path::default()
            },
        );
        paths.load_from_conf(&conf);

        ensure_always_available(&conf, Arc::clone(&paths), &side);

        assert!(side.has_stream("placeholder"));
        let path = paths.get("placeholder").unwrap();
        assert!(path.available);
        assert!(!path.online);
    }

    #[tokio::test]
    async fn record_tap_appends_stream_units_to_segment() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let template = dir
            .path()
            .join("recordings/%path/%Y-%m-%d_%H-%M-%S-%f")
            .to_string_lossy()
            .into_owned();

        let paths = Arc::new(PathManager::new());
        let side = StaticSourceSideTable::new();

        let mut conf = rmtx_conf::Conf::default();
        conf.paths.insert(
            "rec".into(),
            Path {
                always_available: true,
                always_available_tracks: vec![AlwaysAvailableTrack {
                    codec: "H264".into(),
                    ..AlwaysAvailableTrack::default()
                }],
                record: true,
                record_path: template,
                record_format: "fmp4".into(),
                ..Path::default()
            },
        );
        paths.load_from_conf(&conf);

        ensure_always_available(&conf, Arc::clone(&paths), &side);

        let segment = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(seg) = paths.record_supervisor().current_segment("rec") {
                    let len = fs::metadata(&seg).map(|m| m.len()).unwrap_or(0);
                    if len > 40 {
                        return seg;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("segment did not grow from record tap");
        assert!(segment.is_file());
    }
}
