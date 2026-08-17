//! Lookup trait for per-path [`Stream`] buses (RTSP PLAY, HLS, WHEP, etc.).

use std::sync::Arc;

use crate::Stream;

use rmtx_path::PathManager;

/// Returns the active stream bus for a path name, if any.
pub trait StreamLookup: Send + Sync {
    fn stream(&self, path: &str) -> Option<Arc<Stream>>;

    /// Attaches a publisher-side stream bus when the path accepts a new source (RTSP/RTMP publish).
    fn attach_publisher(&self, path: &str) -> Option<Arc<Stream>> {
        let _ = path;
        None
    }

    /// Hook after a publisher stream is attached (record tap, etc.).
    fn after_publisher_attach(
        &self,
        _path: &str,
        _stream: &Arc<Stream>,
        _paths: &Arc<PathManager>,
    ) {
    }
}

/// No-op lookup for tests.
#[derive(Debug, Default)]
pub struct NoStreamLookup;

impl StreamLookup for NoStreamLookup {
    fn stream(&self, _path: &str) -> Option<Arc<Stream>> {
        None
    }
}
