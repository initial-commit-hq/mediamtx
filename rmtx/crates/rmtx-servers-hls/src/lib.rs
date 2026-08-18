//! HLS / MPEG-TS live playlists from [`rmtx_stream::Stream`] / StreamBus.
//!
//! Go counterpart: `internal/servers/hls`.

#![forbid(unsafe_code)]

mod cache;
mod media;
mod server;

pub use cache::{HlsSegment, HlsSegmentCache, SharedHlsCache};
pub use server::{
    empty_live_playlist, router, stub_playlist, stub_ts_segment, HlsServer, HlsServerError, HlsState,
};
