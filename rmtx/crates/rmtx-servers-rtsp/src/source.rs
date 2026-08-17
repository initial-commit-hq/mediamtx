//! RTSP pull source (client ingest).

use thiserror::Error;
use tracing::info;

#[cfg(feature = "retina")]
use crate::ingest::RetinaIngest;
use crate::ingest::RtspIngest;
#[cfg(not(feature = "retina"))]
use crate::ingest::StubIngest;

/// A pull-based RTSP source identified by URL.
#[derive(Debug, Clone)]
pub struct RtspSource {
    pub url: String,
}

impl RtspSource {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Connect to the remote RTSP source (Phase 1: DESCRIBE probe only).
    pub async fn connect(&self) -> Result<(), RtspSourceError> {
        connect_source(&self.url).await
    }
}

/// RTSP source / ingest errors.
#[derive(Debug, Error)]
pub enum RtspSourceError {
    #[error("RTSP ingest not implemented: {0}")]
    NotImplemented(String),

    #[error("invalid RTSP URL: {0}")]
    InvalidUrl(String),

    #[error("retina DESCRIBE failed: {0}")]
    Retina(String),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl From<String> for RtspSourceError {
    fn from(value: String) -> Self {
        Self::NotImplemented(value)
    }
}

/// Probe an RTSP URL using Retina when the `retina` feature is enabled, otherwise stub.
pub async fn connect_source(url: &str) -> Result<(), RtspSourceError> {
    info!(%url, "connecting RTSP source");
    let ingest = select_ingest();
    ingest.describe(url).await
}

fn select_ingest() -> Box<dyn RtspIngest> {
    #[cfg(feature = "retina")]
    {
        return Box::new(RetinaIngest);
    }
    #[cfg(not(feature = "retina"))]
    {
        Box::new(StubIngest)
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn connect_source_without_retina_returns_not_implemented() {
        #[cfg(not(feature = "retina"))]
        {
            let err = super::connect_source("rtsp://127.0.0.1:8554/test")
                .await
                .unwrap_err();
            assert!(matches!(err, super::RtspSourceError::NotImplemented(_)));
        }
    }
}
