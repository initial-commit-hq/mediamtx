//! Side table holding pull-session streams keyed by path name (Phase 2).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use rmtx_path::PathManager;
use rmtx_servers_rtsp::RtspPullSession;
use rmtx_stream::{spawn_placeholder_ticker, Stream, StreamLookup, Track};
use tokio::task::JoinHandle;

use crate::record_tap::spawn_record_tap;

/// Holds active RTSP pull sessions and their stream buses by path name.
///
/// Phase 2: streams are not yet attached to [`rmtx_path::PathManager`]; consumers
/// look up via this table until a `set_stream` API lands.
pub struct StaticSourceSideTable {
    streams: RwLock<HashMap<String, Arc<Stream>>>,
    sessions: RwLock<HashMap<String, RtspPullSession>>,
    placeholder_tickers: RwLock<HashMap<String, JoinHandle<()>>>,
    record_taps: RwLock<HashMap<String, JoinHandle<()>>>,
}

impl Default for StaticSourceSideTable {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticSourceSideTable {
    pub fn new() -> Self {
        Self {
            streams: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            placeholder_tickers: RwLock::new(HashMap::new()),
            record_taps: RwLock::new(HashMap::new()),
        }
    }

    fn abort_record_tap(&self, path: &str) {
        if let Some(handle) = self.record_taps.write().remove(path) {
            handle.abort();
        }
    }

    fn abort_placeholder_ticker(&self, path: &str) {
        if let Some(handle) = self.placeholder_tickers.write().remove(path) {
            handle.abort();
        }
    }

    /// Returns the stream bus for a path, if a pull session is active.
    pub fn stream(&self, path: &str) -> Option<Arc<Stream>> {
        self.streams.read().get(path).cloned()
    }

    /// Stores a pull session and its stream; replaces any prior session for the path.
    pub fn attach(&self, path: impl Into<String>, session: RtspPullSession) -> Arc<Stream> {
        let path = path.into();
        self.abort_placeholder_ticker(&path);
        self.abort_record_tap(&path);
        if let Some(old) = self.sessions.write().remove(&path) {
            old.abort();
        }
        let stream = Arc::new(session.stream().clone());
        self.streams
            .write()
            .insert(path.clone(), Arc::clone(&stream));
        self.sessions.write().insert(path, session);
        stream
    }

    /// Stores a publisher stream (RTMP / ffmpeg pipe) without a pull session.
    pub fn attach_stream(&self, path: impl Into<String>, stream: Stream) -> Arc<Stream> {
        let path = path.into();
        self.abort_placeholder_ticker(&path);
        self.abort_record_tap(&path);
        if let Some(old) = self.sessions.write().remove(&path) {
            old.abort();
        }
        let arc = Arc::new(stream);
        self.streams.write().insert(path, arc.clone());
        arc
    }

    /// Stores a placeholder stream for an always-available path (no pull session).
    pub fn attach_placeholder(&self, path: impl Into<String>, stream: Stream) -> Arc<Stream> {
        let path = path.into();
        self.abort_placeholder_ticker(&path);
        self.abort_record_tap(&path);
        let arc = Arc::new(stream);
        self.streams.write().insert(path.clone(), Arc::clone(&arc));
        let ticker = spawn_placeholder_ticker(Arc::clone(&arc));
        self.placeholder_tickers.write().insert(path, ticker);
        arc
    }

    /// Returns whether a path has a stream entry (pull session or placeholder).
    pub fn has_stream(&self, path: &str) -> bool {
        self.streams.read().contains_key(path)
    }

    /// Aborts and removes the pull session for a path.
    pub fn detach(&self, path: &str) {
        self.abort_placeholder_ticker(path);
        self.abort_record_tap(path);
        if let Some(session) = self.sessions.write().remove(path) {
            session.abort();
        }
        self.streams.write().remove(path);
    }

    /// Starts a recorder tap when `path` has `record: yes` in [`PathManager`].
    pub fn maybe_start_record_tap(&self, path: &str, stream: Arc<Stream>, paths: Arc<PathManager>) {
        let Some(conf) = paths.path_conf(path) else {
            return;
        };
        if !conf.record {
            return;
        }

        self.abort_record_tap(path);
        let handle = spawn_record_tap(path.to_owned(), stream, paths);
        self.record_taps.write().insert(path.to_owned(), handle);
    }
}

impl StreamLookup for StaticSourceSideTable {
    fn stream(&self, path: &str) -> Option<Arc<Stream>> {
        self.streams.read().get(path).cloned()
    }

    fn attach_publisher(&self, path: &str) -> Option<Arc<Stream>> {
        let stream = Stream::new(vec![Track {
            id: 0,
            codec: "H264".into(),
            clock_rate: 90_000,
        }]);
        Some(self.attach_stream(path, stream))
    }

    fn after_publisher_attach(
        &self,
        path: &str,
        stream: &Arc<Stream>,
        paths: &Arc<PathManager>,
    ) {
        self.maybe_start_record_tap(path, Arc::clone(stream), Arc::clone(paths));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmtx_stream::Track;

    #[test]
    fn attach_and_lookup_stream() {
        let table = StaticSourceSideTable::new();
        let stream = Stream::new(vec![Track {
            id: 0,
            codec: "H264".into(),
            clock_rate: 90_000,
        }]);

        // Simulate attach without a live Retina session by inserting stream directly.
        table.streams.write().insert("cam".into(), Arc::new(stream));

        let got = table.stream("cam").expect("stream should exist");
        assert_eq!(got.tracks()[0].codec, "H264");
        assert!(table.stream("missing").is_none());
    }
}
