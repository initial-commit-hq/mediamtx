//! Pull-based static sources
//!
//! Go counterpart: `internal/staticsources`.
//!
//! Phase 1: classify `source` URLs and delegate RTSP to `rmtx-servers-rtsp`.

#![forbid(unsafe_code)]

mod always_available;
mod ffmpeg_pipe;
mod record_tap;
mod rtmp_publish;
mod side_table;
mod spawn;

use thiserror::Error;

pub use always_available::{ensure_always_available, ensure_always_available_shared};
pub use rtmp_publish::on_rtmp_publish;
pub use side_table::StaticSourceSideTable;
pub use spawn::spawn_configured_sources;
use tracing::debug;

/// Pull source kind derived from the path `source` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticSource {
    Rtsp(String),
    Rtsps(String),
    Rtmp(String),
    RtmpS(String),
    Hls(String),
    HttpsHls(String),
    /// Non-pull sources (`publisher`, `redirect`, `rpiCamera`, etc.).
    Other(String),
}

impl StaticSource {
    /// Parse a MediaMTX `source` string into a pull source variant.
    pub fn from_source_field(source: &str) -> Self {
        match source {
            s if s.starts_with("rtsp://") => Self::Rtsp(s.to_owned()),
            s if s.starts_with("rtsps://") => Self::Rtsps(s.to_owned()),
            s if s.starts_with("rtmp://") => Self::Rtmp(s.to_owned()),
            s if s.starts_with("rtmps://") => Self::RtmpS(s.to_owned()),
            s if s.starts_with("http://") && s.contains(".m3u8") => Self::Hls(s.to_owned()),
            s if s.starts_with("https://") && s.contains(".m3u8") => Self::HttpsHls(s.to_owned()),
            other => Self::Other(other.to_owned()),
        }
    }

    /// Whether this source requires a pull ingest path.
    pub fn is_pull(&self) -> bool {
        !matches!(self, Self::Other(_))
    }

    /// Connect / probe the remote source (Phase 1: RTSP only).
    pub async fn connect(&self) -> Result<(), StaticSourceError> {
        debug!(?self, "static source connect");
        match self {
            Self::Rtsp(url) | Self::Rtsps(url) => rmtx_servers_rtsp::connect_source(url)
                .await
                .map_err(StaticSourceError::Rtsp),
            Self::Rtmp(_) | Self::RtmpS(_) => Err(StaticSourceError::NotImplemented(
                "RTMP pull ingest is Phase 2 (Xiu)".into(),
            )),
            Self::Hls(_) | Self::HttpsHls(_) => Err(StaticSourceError::NotImplemented(
                "HLS pull ingest is Phase 2".into(),
            )),
            Self::Other(source) => Err(StaticSourceError::NotPull(source.clone())),
        }
    }
}

/// Static source errors.
#[derive(Debug, Error)]
pub enum StaticSourceError {
    #[error("source is not a pull URL: {0}")]
    NotPull(String),

    #[error("RTSP ingest error: {0}")]
    Rtsp(#[from] rmtx_servers_rtsp::RtspSourceError),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}
