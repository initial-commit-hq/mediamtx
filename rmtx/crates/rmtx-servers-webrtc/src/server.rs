//! Minimal WHIP/WHEP HTTP server (Phase 2 scaffold).
//!
//! Serves POST `/{path}/whip` (publish) and POST `/{path}/whep` (play) for paths
//! registered in [`PathManager`]. With the `webrtc-rs` feature, returns an SDP answer
//! from the client's offer. Without it, returns HTTP 501 with a clear Phase 3 message
//! so the GUI gets a response instead of connection refused.

use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(feature = "webrtc-rs")]
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use rmtx_path::PathManager;
use rmtx_stream::StreamLookup;
#[cfg(feature = "webrtc-rs")]
use rmtx_path::PathSource;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::info;

#[cfg(feature = "webrtc-rs")]
use crate::whep::WhepNegotiationError;

/// Shared WebRTC server state.
#[derive(Clone)]
pub struct WebRtcState {
    pub paths: Arc<PathManager>,
    pub streams: Arc<dyn StreamLookup>,
}

/// WebRTC/WHEP listener bound via [`Self::bind`].
pub struct WebRtcServer {
    listener: TcpListener,
}

impl WebRtcServer {
    /// Binds a TCP listener on `addr` (e.g. `:8889` or `127.0.0.1:0`).
    ///
    /// Bare `:port` forms are normalized to `0.0.0.0:port` (MediaMTX-compatible).
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self, WebRtcServerError> {
        let addr = addr.as_ref();
        let normalized = if let Some(port) = addr.strip_prefix(':') {
            format!("0.0.0.0:{port}")
        } else {
            addr.to_owned()
        };
        let listener = TcpListener::bind(&normalized)
            .await
            .map_err(WebRtcServerError::Io)?;
        Ok(Self { listener })
    }

    /// Returns the local socket address (useful when binding `:0`).
    pub fn local_addr(&self) -> Result<SocketAddr, WebRtcServerError> {
        self.listener.local_addr().map_err(WebRtcServerError::Io)
    }

    /// Serves WHIP/WHEP endpoints until the listener is closed.
    pub async fn run(
        self,
        paths: Arc<PathManager>,
        streams: Arc<dyn StreamLookup>,
    ) -> Result<(), WebRtcServerError> {
        let addr = self.local_addr()?;
        info!(%addr, "WebRTC/WHIP/WHEP server listening");
        let state = WebRtcState { paths, streams };
        axum::serve(self.listener, router(state))
            .await
            .map_err(WebRtcServerError::Serve)
    }
}

#[derive(Debug, Error)]
pub enum WebRtcServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("server error: {0}")]
    Serve(std::io::Error),
}

/// Builds the axum router (exported for unit tests).
pub fn router(state: WebRtcState) -> Router {
    Router::new()
        .route(
            "/{path}/whip",
            post(whip_post_handler).options(whip_options_handler),
        )
        .route(
            "/{path}/whep",
            post(whep_post_handler).options(whep_options_handler),
        )
        .with_state(state)
}

fn start_whep_media_scaffold(state: &WebRtcState, path: &str) {
    if let Some(stream) = state.streams.stream(path) {
        crate::media::spawn_whep_stream_delivery(path.to_owned(), stream);
    }
}

fn cors_origin() -> (&'static str, &'static str) {
    ("Access-Control-Allow-Origin", "*")
}

fn apply_cors(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
}

fn webrtc_options_response() -> Response {
    apply_cors(
        (
            StatusCode::NO_CONTENT,
            [
                (header::ACCESS_CONTROL_ALLOW_METHODS, "OPTIONS, POST"),
                (
                    header::ACCESS_CONTROL_ALLOW_HEADERS,
                    "Authorization, Content-Type, If-Match",
                ),
                (
                    header::ACCESS_CONTROL_EXPOSE_HEADERS,
                    "Location, ETag, Link",
                ),
            ],
        )
            .into_response(),
    )
}

async fn whip_options_handler() -> Response {
    webrtc_options_response()
}

async fn whep_options_handler() -> Response {
    webrtc_options_response()
}

fn invalid_content_type_response() -> Response {
    apply_cors(
        (
            StatusCode::BAD_REQUEST,
            [cors_origin()],
            "invalid Content-Type; expected application/sdp",
        )
            .into_response(),
    )
}

fn validate_webrtc_post(
    paths: &PathManager,
    path: &str,
    headers: &axum::http::HeaderMap,
) -> Result<(), Response> {
    if paths.get(path).is_err() {
        return Err(apply_cors(StatusCode::NOT_FOUND.into_response()));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.starts_with("application/sdp") {
        return Err(invalid_content_type_response());
    }

    Ok(())
}

async fn whip_post_handler(
    State(state): State<WebRtcState>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = validate_webrtc_post(&state.paths, &path, &headers) {
        return response;
    }

    #[cfg(feature = "webrtc-rs")]
    {
        match crate::whip::create_answer(&body).await {
            Ok(answer_sdp) => {
                let session_id = session_id();
                let _ = state.paths.set_ready(
                    &path,
                    true,
                    Some(PathSource {
                        source_type: "webRTCSession".into(),
                        id: session_id.to_string(),
                    }),
                );
                let location = format!("/{path}/whip/{session_id}");
                apply_cors(
                    (
                        StatusCode::CREATED,
                        [
                            (header::CONTENT_TYPE, "application/sdp"),
                            (header::LOCATION, location.as_str()),
                            (
                                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                                "ETag, ID, Accept-Patch, Link, Location",
                            ),
                        ],
                        answer_sdp,
                    )
                        .into_response(),
                )
            }
            Err(crate::whip::WhipNegotiationError::EmptyOffer) => apply_cors(
                (StatusCode::BAD_REQUEST, [cors_origin()], "empty SDP offer").into_response(),
            ),
            Err(crate::whip::WhipNegotiationError::WebRtc(e)) => apply_cors(
                (
                    StatusCode::BAD_REQUEST,
                    [cors_origin()],
                    format!("WebRTC negotiation failed: {e}"),
                )
                    .into_response(),
            ),
        }
    }

    #[cfg(not(feature = "webrtc-rs"))]
    {
        let _ = body;
        apply_cors(
            (
                StatusCode::NOT_IMPLEMENTED,
                [(header::CONTENT_TYPE, "text/plain")],
                crate::whip::PHASE3_NOT_IMPLEMENTED,
            )
                .into_response(),
        )
    }
}

async fn whep_post_handler(
    State(state): State<WebRtcState>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = validate_webrtc_post(&state.paths, &path, &headers) {
        return response;
    }

    start_whep_media_scaffold(&state, &path);

    #[cfg(feature = "webrtc-rs")]
    {
        match crate::whep::create_answer(&body).await {
            Ok(answer_sdp) => {
                let session_id = session_id();
                let location = format!("/{path}/whep/{session_id}");
                apply_cors(
                    (
                        StatusCode::CREATED,
                        [
                            (header::CONTENT_TYPE, "application/sdp"),
                            (header::LOCATION, location.as_str()),
                            (
                                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                                "ETag, ID, Accept-Patch, Link, Location",
                            ),
                        ],
                        answer_sdp,
                    )
                        .into_response(),
                )
            }
            Err(WhepNegotiationError::EmptyOffer) => apply_cors(
                (StatusCode::BAD_REQUEST, [cors_origin()], "empty SDP offer").into_response(),
            ),
            Err(WhepNegotiationError::WebRtc(e)) => apply_cors(
                (
                    StatusCode::BAD_REQUEST,
                    [cors_origin()],
                    format!("WebRTC negotiation failed: {e}"),
                )
                    .into_response(),
            ),
        }
    }

    #[cfg(not(feature = "webrtc-rs"))]
    {
        let _ = body;
        apply_cors(
            (
                StatusCode::NOT_IMPLEMENTED,
                [(header::CONTENT_TYPE, "text/plain")],
                crate::whep::PHASE3_NOT_IMPLEMENTED,
            )
                .into_response(),
        )
    }
}

#[cfg(feature = "webrtc-rs")]
fn session_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use rmtx_path::{PathConf, PathManager};
    use tower::ServiceExt;

    use super::*;
    #[cfg(not(feature = "webrtc-rs"))]
    use crate::whep::PHASE3_NOT_IMPLEMENTED as WHEP_PHASE3_NOT_IMPLEMENTED;
    #[cfg(not(feature = "webrtc-rs"))]
    use crate::whip::PHASE3_NOT_IMPLEMENTED as WHIP_PHASE3_NOT_IMPLEMENTED;

    use rmtx_stream::NoStreamLookup;

    fn test_state(paths: Arc<PathManager>) -> WebRtcState {
        WebRtcState {
            paths,
            streams: Arc::new(NoStreamLookup),
        }
    }

    const MINIMAL_OFFER: &str = "v=0\r\n\
o=- 4611731400430051336 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
a=extmap-allow-mixed\r\n\
a=msid-semantic: WMS\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
c=IN IP4 0.0.0.0\r\n\
a=rtcp:9 IN IP4 0.0.0.0\r\n\
a=ice-ufrag:abcd\r\n\
a=ice-pwd:0123456789abcdef0123456789abcdef\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n\
a=setup:actpass\r\n\
a=mid:0\r\n\
a=recvonly\r\n\
a=rtcp-mux\r\n\
a=rtpmap:96 VP8/90000\r\n";

    const WHIP_MINIMAL_OFFER: &str = "v=0\r\n\
o=- 4611731400430051336 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
a=extmap-allow-mixed\r\n\
a=msid-semantic: WMS\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
c=IN IP4 0.0.0.0\r\n\
a=rtcp:9 IN IP4 0.0.0.0\r\n\
a=ice-ufrag:abcd\r\n\
a=ice-pwd:0123456789abcdef0123456789abcdef\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n\
a=setup:actpass\r\n\
a=mid:0\r\n\
a=sendonly\r\n\
a=rtcp-mux\r\n\
a=rtpmap:96 VP8/90000\r\n";

    #[tokio::test]
    async fn whep_options_returns_cors() {
        let paths = Arc::new(PathManager::new());
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/demo/whep")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("*"))
        );
    }

    #[tokio::test]
    async fn whep_unknown_path_returns_404() {
        let paths = Arc::new(PathManager::new());
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/missing/whep")
                    .header(header::CONTENT_TYPE, "application/sdp")
                    .body(Body::from("v=0\r\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn whep_known_path_post() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/demo/whep")
                    .header(header::CONTENT_TYPE, "application/sdp")
                    .body(Body::from(MINIMAL_OFFER))
                    .unwrap(),
            )
            .await
            .unwrap();

        #[cfg(feature = "webrtc-rs")]
        {
            assert_eq!(response.status(), StatusCode::CREATED);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "application/sdp"
            );
            assert!(response.headers().get(header::LOCATION).is_some());
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let sdp = String::from_utf8(body.to_vec()).unwrap();
            assert!(sdp.contains("v=0"));
        }

        #[cfg(not(feature = "webrtc-rs"))]
        {
            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let text = String::from_utf8(body.to_vec()).unwrap();
            assert!(text.contains(WHEP_PHASE3_NOT_IMPLEMENTED));
        }
    }

    #[tokio::test]
    async fn whip_options_returns_cors() {
        let paths = Arc::new(PathManager::new());
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/demo/whip")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("*"))
        );
    }

    #[tokio::test]
    async fn whip_unknown_path_returns_404() {
        let paths = Arc::new(PathManager::new());
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/missing/whip")
                    .header(header::CONTENT_TYPE, "application/sdp")
                    .body(Body::from("v=0\r\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn whip_known_path_post() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        let app = router(test_state(paths.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/demo/whip")
                    .header(header::CONTENT_TYPE, "application/sdp")
                    .body(Body::from(WHIP_MINIMAL_OFFER))
                    .unwrap(),
            )
            .await
            .unwrap();

        #[cfg(feature = "webrtc-rs")]
        {
            assert_eq!(response.status(), StatusCode::CREATED);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "application/sdp"
            );
            assert!(response.headers().get(header::LOCATION).is_some());
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let sdp = String::from_utf8(body.to_vec()).unwrap();
            assert!(sdp.contains("v=0"));

            let path = paths.get("demo").unwrap();
            assert!(path.ready);
            assert!(path.online);
            assert_eq!(
                path.source.as_ref().map(|s| s.source_type.as_str()),
                Some("webRTCSession")
            );
        }

        #[cfg(not(feature = "webrtc-rs"))]
        {
            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let text = String::from_utf8(body.to_vec()).unwrap();
            assert!(text.contains(WHIP_PHASE3_NOT_IMPLEMENTED));
        }
    }

    #[cfg(not(feature = "webrtc-rs"))]
    #[tokio::test]
    async fn whep_empty_offer_without_webrtc_rs_returns_501() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/demo/whep")
                    .header(header::CONTENT_TYPE, "application/sdp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[cfg(feature = "webrtc-rs")]
    #[tokio::test]
    async fn whep_empty_offer_with_webrtc_rs_returns_400() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        let app = router(test_state(paths));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/demo/whep")
                    .header(header::CONTENT_TYPE, "application/sdp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn webrtc_bound_listener() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router(test_state(paths))).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://{addr}/demo/whep"))
            .header("Content-Type", "application/sdp")
            .body(MINIMAL_OFFER)
            .send()
            .await
            .unwrap();

        #[cfg(feature = "webrtc-rs")]
        assert_eq!(response.status(), StatusCode::CREATED);

        #[cfg(not(feature = "webrtc-rs"))]
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|v| v.to_str().unwrap()),
            Some("*")
        );
    }
}
