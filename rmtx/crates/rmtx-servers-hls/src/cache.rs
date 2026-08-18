//! Rolling HLS MPEG-TS segments per path.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::RwLock;

const MAX_SEGMENTS: usize = 6;

/// One MPEG-TS HLS media segment.
#[derive(Debug, Clone)]
pub struct HlsSegment {
    pub seq: u64,
    pub duration_sec: f64,
    pub data: Vec<u8>,
}

/// Shared HLS segment cache filled by stream pumps.
#[derive(Debug, Default)]
pub struct HlsSegmentCache {
    windows: RwLock<HashMap<String, PathWindow>>,
}

#[derive(Debug, Default)]
struct PathWindow {
    next_seq: u64,
    segments: VecDeque<HlsSegment>,
}

impl HlsSegmentCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a closed MPEG-TS segment and drops the oldest beyond the window.
    pub fn push(&self, path: &str, duration_sec: f64, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        let mut windows = self.windows.write();
        let w = windows.entry(path.to_owned()).or_default();
        let seq = w.next_seq;
        w.next_seq += 1;
        w.segments.push_back(HlsSegment {
            seq,
            duration_sec: duration_sec.max(0.1),
            data,
        });
        while w.segments.len() > MAX_SEGMENTS {
            w.segments.pop_front();
        }
    }

    pub fn playlist(&self, path: &str) -> Option<String> {
        let windows = self.windows.read();
        let w = windows.get(path)?;
        if w.segments.is_empty() {
            return None;
        }
        let media_seq = w.segments.front().map(|s| s.seq).unwrap_or(0);
        let mut body = String::from(
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:",
        );
        body.push_str(&media_seq.to_string());
        body.push('\n');
        for seg in &w.segments {
            body.push_str(&format!("#EXTINF:{:.3},\nseg{}.ts\n", seg.duration_sec, seg.seq));
        }
        Some(body)
    }

    pub fn get_segment(&self, path: &str, seq: u64) -> Option<Vec<u8>> {
        let windows = self.windows.read();
        windows
            .get(path)?
            .segments
            .iter()
            .find(|s| s.seq == seq)
            .map(|s| s.data.clone())
    }

    pub fn has_media(&self, path: &str) -> bool {
        self.windows
            .read()
            .get(path)
            .is_some_and(|w| !w.segments.is_empty())
    }
}

pub type SharedHlsCache = Arc<HlsSegmentCache>;
