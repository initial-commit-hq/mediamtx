//! Latest MPEG-TS-ish segment bytes per path (from StreamBus units).

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

const TS_PACKET: usize = 188;

/// Shared HLS segment cache filled by stream pumps.
#[derive(Debug, Default)]
pub struct HlsSegmentCache {
    segments: RwLock<HashMap<String, Vec<u8>>>,
}

impl HlsSegmentCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the cached segment for `path` (padded to whole TS packets).
    pub fn store(&self, path: &str, data: Bytes) {
        if data.is_empty() {
            return;
        }
        let mut out = data.to_vec();
        let rem = out.len() % TS_PACKET;
        if rem != 0 {
            out.resize(out.len() + (TS_PACKET - rem), 0);
        }
        if out[0] != 0x47 {
            out[0] = 0x47;
        }
        self.segments.write().insert(path.to_owned(), out);
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.segments.read().get(path).cloned()
    }
}

pub type SharedHlsCache = Arc<HlsSegmentCache>;
