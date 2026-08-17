//! RTSP ingest backends (Retina native vs ffmpeg fallback).

use async_trait::async_trait;

use crate::source::RtspSourceError;

/// RTSP client ingest interface.
///
/// Phase 1 implements DESCRIBE probing. PLAY/setup and media forwarding are TODO.
#[async_trait]
pub trait RtspIngest: Send + Sync {
    async fn describe(&self, url: &str) -> Result<(), RtspSourceError>;
}

/// Stub ingest when Retina is unavailable or disabled.
pub struct StubIngest;

#[async_trait]
impl RtspIngest for StubIngest {
    async fn describe(&self, url: &str) -> Result<(), RtspSourceError> {
        Err(RtspSourceError::NotImplemented(format!(
            "RTSP ingest for {url} requires the `retina` feature or ffmpeg fallback (see rmtx-ffmpeg)"
        )))
    }
}

/// Retina-based ingest (`Session::describe`).
#[cfg(feature = "retina")]
pub struct RetinaIngest;

#[cfg(feature = "retina")]
#[async_trait]
impl RtspIngest for RetinaIngest {
    async fn describe(&self, url: &str) -> Result<(), RtspSourceError> {
        use retina::client::{Session, SessionOptions};

        let parsed = url
            .parse()
            .map_err(|e| RtspSourceError::InvalidUrl(format!("{url}: {e}")))?;
        let session = Session::describe(parsed, SessionOptions::default())
            .await
            .map_err(|e| RtspSourceError::Retina(e.to_string()))?;
        tracing::info!(
            url,
            streams = session.streams().len(),
            "retina DESCRIBE succeeded"
        );
        Ok(())
    }
}

#[cfg(not(feature = "retina"))]
pub struct RetinaIngest;

/// FFmpeg subprocess fallback for Retina gaps (UDP reorder, ONVIF backchannel, minimal RTCP SR).
///
/// Phase 1 stub — spawns are implemented in `rmtx-ffmpeg`. When Retina fails or lacks a
/// capability, the path manager should start an `FfmpegPull` child that remuxes to a local
/// RTSP publisher endpoint.
pub struct FfmpegFallback {
    pub url: String,
}

impl FfmpegFallback {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Returns a not-implemented error; wiring to `rmtx_ffmpeg::FfmpegPull` is TODO.
    pub async fn describe(&self) -> Result<(), RtspSourceError> {
        Err(RtspSourceError::NotImplemented(format!(
            "ffmpeg fallback for {} not wired yet — see rmtx-ffmpeg::FfmpegPull::argv",
            self.url
        )))
    }
}
