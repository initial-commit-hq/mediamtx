//! Subscribes to StreamBus and updates [`HlsSegmentCache`].

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use rmtx_path::PathManager;
use rmtx_stream::{Stream, StreamLookup};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::cache::SharedHlsCache;

const SEGMENT_ACCUM_LIMIT: usize = 512 * 1024;

/// Periodically starts HLS pumps for paths that gain a stream bus.
pub fn spawn_hls_pump_supervisor(
    paths: Arc<PathManager>,
    streams: Arc<dyn StreamLookup>,
    cache: SharedHlsCache,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut active = HashSet::new();
        loop {
            for name in paths.conf_names() {
                if active.contains(&name) {
                    continue;
                }
                if let Some(stream) = streams.stream(&name) {
                    active.insert(name.clone());
                    spawn_path_hls_pump(name, stream, Arc::clone(&cache));
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

fn spawn_path_hls_pump(path: String, stream: Arc<Stream>, cache: SharedHlsCache) {
    tokio::spawn(async move {
        let mut sub = stream.subscribe();
        let mut segment = Vec::new();
        loop {
            match sub.recv().await {
                Ok(unit) => {
                    rmtx_mux::append_media_to_segment(
                        &mut segment,
                        unit.payload.as_ref(),
                        unit.pts_us,
                    );
                    if segment.len() >= SEGMENT_ACCUM_LIMIT {
                        segment = segment.split_off(segment.len() - SEGMENT_ACCUM_LIMIT / 2);
                    }
                    cache.store(&path, bytes::Bytes::from(segment.clone()));
                }
                Err(_) => {
                    debug!(path = %path, "HLS pump: stream bus closed");
                    break;
                }
            }
        }
    });
}
