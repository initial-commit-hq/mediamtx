//! Subscribes to StreamBus and writes rolling MPEG-TS HLS segments.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use rmtx_mux::TsMuxer;
use rmtx_path::PathManager;
use rmtx_stream::{Stream, StreamLookup};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::cache::SharedHlsCache;

const TARGET_SEG_US: u64 = 1_000_000;
const MIN_SEG_US: u64 = 400_000;

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

fn is_h264(codec: &str) -> bool {
    codec.eq_ignore_ascii_case("H264") || codec.eq_ignore_ascii_case("AVC")
}

fn spawn_path_hls_pump(path: String, stream: Arc<Stream>, cache: SharedHlsCache) {
    tokio::spawn(async move {
        let video_ids: Vec<u32> = stream
            .tracks()
            .iter()
            .filter(|t| is_h264(&t.codec))
            .map(|t| t.id)
            .collect();
        if video_ids.is_empty() {
            debug!(path = %path, "HLS pump: no H264 track, idle");
            return;
        }

        let mut sub = stream.subscribe();
        let mut mux = TsMuxer::new();
        let mut segment = Vec::new();
        let mut seg_start_pts: Option<u64> = None;
        let mut last_pts: u64 = 0;

        loop {
            match sub.recv().await {
                Ok(unit) => {
                    if !video_ids.contains(&unit.track_id) {
                        continue;
                    }
                    let annex = rmtx_mux::to_annex_b(unit.payload.as_ref());
                    // Skip tiny non-picture placeholders (AUD-only).
                    if annex.len() <= 8 && !rmtx_mux::h264_is_keyframe(&annex) {
                        continue;
                    }

                    let key = rmtx_mux::h264_is_keyframe(&annex);
                    if let (Some(start), true) = (seg_start_pts, key) {
                        if last_pts.saturating_sub(start) >= MIN_SEG_US && !segment.is_empty() {
                            let dur = last_pts.saturating_sub(start) as f64 / 1_000_000.0;
                            cache.push(&path, dur, std::mem::take(&mut segment));
                            mux.write_pat_pmt(&mut segment);
                            seg_start_pts = Some(unit.pts_us);
                        }
                    }

                    if segment.is_empty() {
                        mux.write_pat_pmt(&mut segment);
                        seg_start_pts = Some(unit.pts_us);
                    }

                    mux.write_h264_pes(&mut segment, &annex, unit.pts_us);
                    last_pts = unit.pts_us;

                    if let Some(start) = seg_start_pts {
                        if last_pts.saturating_sub(start) >= TARGET_SEG_US && !segment.is_empty() {
                            let dur = last_pts.saturating_sub(start) as f64 / 1_000_000.0;
                            cache.push(&path, dur, std::mem::take(&mut segment));
                            seg_start_pts = None;
                        }
                    }
                }
                Err(_) => {
                    if !segment.is_empty() {
                        let dur = match seg_start_pts {
                            Some(start) => last_pts.saturating_sub(start) as f64 / 1_000_000.0,
                            None => 1.0,
                        };
                        cache.push(&path, dur, segment);
                    }
                    debug!(path = %path, "HLS pump: stream bus closed");
                    break;
                }
            }
        }
    });
}
