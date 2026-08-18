//! Minimal HLS HTTP server (Phase 2 scaffold).
//!
//! Serves stub playlists and placeholder segments for paths registered in
//! [`PathManager`]. Real MPEG-TS muxing from [`rmtx_stream::Stream`] / StreamBus
//! is deferred to a later phase.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rmtx_path::PathManager;
use rmtx_stream::StreamLookup;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::info;

use crate::cache::SharedHlsCache;
use crate::media::spawn_hls_pump_supervisor;

/// Shared HLS server state.
#[derive(Clone)]
pub struct HlsState {
    pub paths: Arc<PathManager>,
    pub cache: SharedHlsCache,
}

/// HLS listener bound via [`Self::bind`].
pub struct HlsServer {
    listener: TcpListener,
}

impl HlsServer {
    /// Binds a TCP listener on `addr` (e.g. `:8888` or `127.0.0.1:0`).
    ///
    /// Bare `:port` forms are normalized to `0.0.0.0:port` (MediaMTX-compatible).
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self, HlsServerError> {
        let addr = addr.as_ref();
        let normalized = if let Some(port) = addr.strip_prefix(':') {
            format!("0.0.0.0:{port}")
        } else {
            addr.to_owned()
        };
        let listener = TcpListener::bind(&normalized)
            .await
            .map_err(HlsServerError::Io)?;
        Ok(Self { listener })
    }

    /// Returns the local socket address (useful when binding `:0`).
    pub fn local_addr(&self) -> Result<SocketAddr, HlsServerError> {
        self.listener.local_addr().map_err(HlsServerError::Io)
    }

    /// Serves HLS playlists and segments until the listener is closed.
    pub async fn run(
        self,
        paths: Arc<PathManager>,
        streams: Arc<dyn StreamLookup>,
    ) -> Result<(), HlsServerError> {
        let addr = self.local_addr()?;
        let cache = Arc::new(crate::cache::HlsSegmentCache::new());
        let _pump = spawn_hls_pump_supervisor(Arc::clone(&paths), streams, Arc::clone(&cache));
        info!(%addr, "HLS server listening");
        let state = HlsState { paths, cache };
        axum::serve(self.listener, router(state))
            .await
            .map_err(HlsServerError::Serve)
    }
}

#[derive(Debug, Error)]
pub enum HlsServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("server error: {0}")]
    Serve(std::io::Error),
}

fn hls_headers(content_type: &'static str) -> [(axum::http::HeaderName, &'static str); 3] {
    [
        (header::CONTENT_TYPE, content_type),
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate"),
    ]
}

async fn hls_options() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "*"),
        ],
    )
        .into_response()
}

/// Builds the axum router (exported for unit tests).
pub fn router(state: HlsState) -> Router {
    Router::new()
        .route(
            "/{path}/index.m3u8",
            get(playlist_handler).options(hls_options),
        )
        .route("/{path}/{seg}", get(segment_handler).options(hls_options))
        .with_state(state)
}

async fn playlist_handler(
    State(state): State<HlsState>,
    Path(path): Path<String>,
) -> Response {
    if state.paths.get(&path).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let playlist = state
        .cache
        .playlist(&path)
        .unwrap_or_else(empty_live_playlist);

    (
        StatusCode::OK,
        hls_headers("application/vnd.apple.mpegurl"),
        playlist,
    )
        .into_response()
}

fn parse_seg_seq(seg: &str) -> Option<u64> {
    let name = seg.strip_suffix(".ts")?;
    let num = name.strip_prefix("seg")?;
    num.parse().ok()
}

async fn segment_handler(
    State(state): State<HlsState>,
    Path((path, seg)): Path<(String, String)>,
) -> Response {
    if state.paths.get(&path).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(seq) = parse_seg_seq(&seg) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(body) = state.cache.get_segment(&path, seq) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    (
        StatusCode::OK,
        hls_headers("video/mp2t"),
        body,
    )
        .into_response()
}

/// Live playlist with no segments yet (hls.js will poll).
pub fn empty_live_playlist() -> String {
    "#EXTM3U\n\
     #EXT-X-VERSION:3\n\
     #EXT-X-TARGETDURATION:2\n\
     #EXT-X-MEDIA-SEQUENCE:0\n"
        .to_owned()
}

/// Backward-compatible alias used by older tests / smoke checks.
pub fn stub_playlist() -> String {
    empty_live_playlist()
}

/// Dummy 188-byte MPEG-TS null packet.
pub fn stub_ts_segment() -> Vec<u8> {
    let mut packet = vec![0u8; 188];
    packet[0] = 0x47;
    packet
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rmtx_path::{PathConf, PathManager};
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn stub_playlist_contains_required_tags() {
        let body = stub_playlist();
        assert!(body.contains("#EXTM3U"));
        assert!(body.contains("#EXT-X-VERSION:3"));
        assert!(body.contains("#EXT-X-MEDIA-SEQUENCE"));
        assert!(!body.contains("#EXT-X-ENDLIST"));
    }

    #[test]
    fn stub_ts_segment_is_mpeg_ts_packet() {
        let seg = stub_ts_segment();
        assert_eq!(seg.len(), 188);
        assert_eq!(seg[0], 0x47);
    }

    use crate::cache::HlsSegmentCache;

    fn test_state() -> HlsState {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        HlsState {
            paths,
            cache: Arc::new(HlsSegmentCache::new()),
        }
    }

    #[tokio::test]
    async fn playlist_known_path_oneshot() {
        let app = router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/demo/index.m3u8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/vnd.apple.mpegurl"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("#EXTM3U"));
        assert!(text.contains("#EXT-X-MEDIA-SEQUENCE"));
    }

    #[tokio::test]
    async fn playlist_unknown_path_returns_404() {
        let paths = Arc::new(PathManager::new());
        let app = router(HlsState {
            paths,
            cache: Arc::new(HlsSegmentCache::new()),
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/missing/index.m3u8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn segment_known_path_oneshot() {
        let state = test_state();
        state.cache.push("demo", 1.0, stub_ts_segment());
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/demo/seg0.ts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "video/mp2t"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.len(), 188);
        assert_eq!(body[0], 0x47);
    }

    #[tokio::test]
    async fn hls_bound_listener() {
        let state = test_state();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{addr}/demo/index.m3u8"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = response.text().await.unwrap();
        assert!(text.contains("#EXTM3U"));
    }
}
