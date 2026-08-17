//! HLS / LL-HLS server
//!
//! Go counterpart: `internal/servers/hls`.
//!
//! Phase 2 scaffold: HTTP server that returns stub playlists and placeholder
//! MPEG-TS segments for paths known to [`rmtx_path::PathManager`].
//!
//! **Not yet implemented:** real HLS muxing from live media on
//! [`rmtx_stream::Stream`] / StreamBus (segment generation, playlist rolling,
//! LL-HLS parts, encryption, etc.).

#![forbid(unsafe_code)]

mod cache;
mod media;
mod server;

pub use cache::{HlsSegmentCache, SharedHlsCache};
pub use server::{router, stub_playlist, stub_ts_segment, HlsServer, HlsServerError, HlsState};
