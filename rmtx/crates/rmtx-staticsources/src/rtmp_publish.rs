//! RTMP publisher attach — marks path ready and registers a Stream bus.

use std::sync::Arc;

use rmtx_path::{PathManager, PathSource};
use rmtx_stream::{Stream, Track};
use tracing::info;

use crate::side_table::StaticSourceSideTable;

/// Called when an RTMP client publishes to `path` (stream name maps to path).
pub fn on_rtmp_publish(path: &str, paths: Arc<PathManager>, side: Arc<StaticSourceSideTable>) {
    if paths.get(path).is_err() {
        tracing::debug!(path, "RTMP publish ignored: unknown path");
        return;
    }

    let stream = Stream::new(vec![Track {
        id: 0,
        codec: "H264".into(),
        clock_rate: 90_000,
    }]);
    let arc = side.attach_stream(path, stream);
    if let Err(err) = paths.set_ready(
        path,
        true,
        Some(PathSource {
            source_type: "rtmpConn".into(),
            id: String::new(),
        }),
    ) {
        tracing::warn!(path, error = %err, "RTMP publish: failed to mark path ready");
        return;
    }
    side.maybe_start_record_tap(path, arc, Arc::clone(&paths));
    info!(path, "RTMP publish attached (StreamBus scaffold; frame ingest Phase 3)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmtx_path::PathConf;

    #[test]
    fn publish_attaches_stream_and_marks_ready() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("live"));
        let side = Arc::new(StaticSourceSideTable::new());

        on_rtmp_publish("live", Arc::clone(&paths), Arc::clone(&side));

        assert!(side.has_stream("live"));
        let status = paths.get("live").unwrap();
        assert!(status.online);
        assert!(status.source.as_ref().is_some_and(|s| s.source_type == "rtmpConn"));
    }
}
